#include "minarch.h"

#include <lz4.h>

#define REWIND_ALLOC_PADDING_DIVISOR 5
#define REWIND_ENTRY_SIZE_HINT 4096
#define REWIND_MIN_ENTRIES 64
#define REWIND_MIN_BUFFER_MB 16
#define REWIND_MAX_BUFFER_MB 256
#define REWIND_MAX_LZ4_ACCELERATION 64
#define REWIND_POOL_SIZE_SMALL 3
#define REWIND_POOL_SIZE_LARGE 4
#define REWIND_LARGE_STATE_THRESHOLD (2 * 1024 * 1024)

typedef struct RewindEntry {
	size_t offset;
	size_t size;
	size_t state_size;
	uint8_t is_keyframe;
} RewindEntry;

typedef struct RewindContext {
	int enabled;
	int audio;
	int buffer_mb;
	int capture_interval_ms;
	int keyframe_interval_ms;
	int lz4_acceleration;
	size_t capacity;
	size_t state_size;
	size_t alloc_size;
	size_t scratch_size;
	uint8_t *buffer;
	RewindEntry *entries;
	int entry_capacity;
	int entry_head;
	int entry_tail;
	int entry_count;
	size_t head;
	size_t tail;
	uint8_t *state_buf;
	uint8_t *delta_buf;
	uint8_t *scratch;
	uint8_t *prev_state_enc;
	size_t prev_state_size;
	int has_prev_enc;
	uint32_t last_capture_ms;
	uint32_t last_keyframe_ms;
	uint32_t last_step_ms;
	int play_index;
	size_t play_state_size;
	enum retro_savestate_context savestate_context;
	pthread_t worker;
	pthread_mutex_t lock;
	pthread_mutex_t queue_mx;
	pthread_cond_t queue_cv;
	int locks_ready;
	int worker_running;
	int worker_stop;
	int pool_size;
	uint8_t **capture_pool;
	size_t *capture_sizes;
	uint8_t *capture_keyframe;
	unsigned int *capture_gen;
	uint8_t *capture_busy;
	int *free_stack;
	int free_count;
	int *queue;
	int queue_capacity;
	int queue_head;
	int queue_tail;
	int queue_count;
	unsigned int generation;
} RewindContext;

static RewindContext rewind_ctx = {0};

int rewind_pressed = 0;
int rewind_toggle = 0;
int rewinding = 0;

///////////////////////////////////////

static int Rewind_getOptionInt(int id, int fallback)
{
	Option *option = &config.frontend.options[id];
	char *value;

	if (option->value < 0 || option->value >= option->count)
		return fallback;

	value = option->values[option->value];
	if (!value)
		return fallback;

	return (int)strtol(value, NULL, 0);
}

static int Rewind_getEnable(void)
{
	return config.frontend.options[FE_OPT_REWIND_ENABLE].value;
}

static size_t Rewind_getAllocSize(size_t state_size)
{
	size_t extra = state_size / REWIND_ALLOC_PADDING_DIVISOR;
	size_t minimum = 64 * 1024;

	if (extra < minimum)
		extra = minimum;
	return state_size + extra;
}

static int Rewind_getPoolSize(size_t state_size)
{
	if (state_size > REWIND_LARGE_STATE_THRESHOLD)
		return REWIND_POOL_SIZE_LARGE;
	return REWIND_POOL_SIZE_SMALL;
}

static int Rewind_entryIndexLocked(int logical_index)
{
	return (rewind_ctx.entry_tail + logical_index) % rewind_ctx.entry_capacity;
}

static RewindEntry *Rewind_entryLocked(int logical_index)
{
	return &rewind_ctx.entries[Rewind_entryIndexLocked(logical_index)];
}

static size_t Rewind_entryEnd(const RewindEntry *entry)
{
	size_t end = entry->offset + entry->size;

	if (end >= rewind_ctx.capacity)
		end -= rewind_ctx.capacity;
	return end;
}

static size_t Rewind_freeSpaceLocked(void)
{
	if (!rewind_ctx.entry_count)
		return rewind_ctx.capacity;
	if (rewind_ctx.head == rewind_ctx.tail)
		return 0;
	if (rewind_ctx.head >= rewind_ctx.tail)
		return rewind_ctx.capacity - (rewind_ctx.head - rewind_ctx.tail);
	return rewind_ctx.tail - rewind_ctx.head;
}

static void Rewind_dropOldestLocked(void)
{
	RewindEntry *entry;

	if (!rewind_ctx.entry_count)
		return;

	entry = &rewind_ctx.entries[rewind_ctx.entry_tail];
	rewind_ctx.tail = Rewind_entryEnd(entry);
	rewind_ctx.entry_tail =
		(rewind_ctx.entry_tail + 1) % rewind_ctx.entry_capacity;
	rewind_ctx.entry_count -= 1;
	if (!rewind_ctx.entry_count) {
		rewind_ctx.head = 0;
		rewind_ctx.tail = 0;
	}
}

static void Rewind_dropLeadingDeltasLocked(void)
{
	while (rewind_ctx.entry_count) {
		RewindEntry *entry = &rewind_ctx.entries[rewind_ctx.entry_tail];

		if (entry->is_keyframe)
			break;
		Rewind_dropOldestLocked();
	}
}

static int Rewind_entryOverlapsRangeLocked(int entry_index, size_t start,
	size_t end)
{
	RewindEntry *entry = &rewind_ctx.entries[entry_index];
	size_t entry_end = entry->offset + entry->size;

	return entry->offset < end && start < entry_end;
}

static void Rewind_clearQueueLocked(void)
{
	int i;

	rewind_ctx.queue_head = 0;
	rewind_ctx.queue_tail = 0;
	rewind_ctx.queue_count = 0;
	rewind_ctx.free_count = 0;

	for (i = 0; i < rewind_ctx.pool_size; i++) {
		rewind_ctx.capture_busy[i] = 0;
		rewind_ctx.free_stack[rewind_ctx.free_count++] = i;
	}
}

static void Rewind_waitForWorkerIdle(void)
{
	if (!rewind_ctx.worker_running || !rewind_ctx.pool_size)
		return;

	pthread_mutex_lock(&rewind_ctx.queue_mx);
	while (rewind_ctx.queue_count > 0 ||
		rewind_ctx.free_count < rewind_ctx.pool_size) {
		pthread_mutex_unlock(&rewind_ctx.queue_mx);
		SDL_Delay(1);
		pthread_mutex_lock(&rewind_ctx.queue_mx);
	}
	pthread_mutex_unlock(&rewind_ctx.queue_mx);
}

static void *Rewind_workerThread(void *arg);

static void Rewind_free(void)
{
	int i;

	if (rewind_ctx.worker_running) {
		pthread_mutex_lock(&rewind_ctx.queue_mx);
		rewind_ctx.worker_stop = 1;
		pthread_cond_signal(&rewind_ctx.queue_cv);
		pthread_mutex_unlock(&rewind_ctx.queue_mx);
		pthread_join(rewind_ctx.worker, NULL);
		rewind_ctx.worker_running = 0;
	}

	if (rewind_ctx.capture_pool) {
		for (i = 0; i < rewind_ctx.pool_size; i++)
			free(rewind_ctx.capture_pool[i]);
	}
	free(rewind_ctx.capture_pool);
	free(rewind_ctx.capture_sizes);
	free(rewind_ctx.capture_keyframe);
	free(rewind_ctx.capture_gen);
	free(rewind_ctx.capture_busy);
	free(rewind_ctx.free_stack);
	free(rewind_ctx.queue);
	free(rewind_ctx.buffer);
	free(rewind_ctx.entries);
	free(rewind_ctx.state_buf);
	free(rewind_ctx.delta_buf);
	free(rewind_ctx.scratch);
	free(rewind_ctx.prev_state_enc);

	if (rewind_ctx.locks_ready) {
		pthread_mutex_destroy(&rewind_ctx.lock);
		pthread_mutex_destroy(&rewind_ctx.queue_mx);
		pthread_cond_destroy(&rewind_ctx.queue_cv);
	}

	memset(&rewind_ctx, 0, sizeof(rewind_ctx));
	rewind_pressed = 0;
	rewind_toggle = 0;
	rewinding = 0;
}

static void Rewind_resetHistory(void)
{
	if (!rewind_ctx.enabled)
		return;

	Rewind_waitForWorkerIdle();

	pthread_mutex_lock(&rewind_ctx.lock);
	rewind_ctx.entry_head = 0;
	rewind_ctx.entry_tail = 0;
	rewind_ctx.entry_count = 0;
	rewind_ctx.head = 0;
	rewind_ctx.tail = 0;
	rewind_ctx.has_prev_enc = 0;
	rewind_ctx.prev_state_size = 0;
	rewind_ctx.play_index = -1;
	rewind_ctx.play_state_size = 0;
	pthread_mutex_unlock(&rewind_ctx.lock);

	rewind_ctx.last_capture_ms = 0;
	rewind_ctx.last_keyframe_ms = 0;
	rewind_ctx.last_step_ms = 0;
	pthread_mutex_lock(&rewind_ctx.queue_mx);
	rewind_ctx.generation += 1;
	if (!rewind_ctx.generation)
		rewind_ctx.generation = 1;
	if (rewind_ctx.pool_size)
		Rewind_clearQueueLocked();
	pthread_mutex_unlock(&rewind_ctx.queue_mx);

	rewinding = 0;
}

static int Rewind_writeEntryLocked(const uint8_t *src, size_t src_size,
	size_t state_size, int is_keyframe)
{
	size_t write_offset;

	if (!src_size || src_size >= rewind_ctx.capacity)
		return 0;

	if (rewind_ctx.entry_count == rewind_ctx.entry_capacity)
		Rewind_dropOldestLocked();

	write_offset = rewind_ctx.head;
	if (write_offset + src_size > rewind_ctx.capacity) {
		write_offset = 0;
		rewind_ctx.head = 0;
	}

	while (rewind_ctx.entry_count) {
		int oldest = rewind_ctx.entry_tail;

		if (!Rewind_entryOverlapsRangeLocked(oldest, write_offset,
			write_offset + src_size)) {
			break;
		}
		Rewind_dropOldestLocked();
	}

	while (rewind_ctx.entry_count &&
		Rewind_freeSpaceLocked() <= src_size) {
		Rewind_dropOldestLocked();
	}

	if (rewind_ctx.entry_count &&
		Rewind_freeSpaceLocked() <= src_size) {
		return 0;
	}

	memcpy(rewind_ctx.buffer + write_offset, src, src_size);

	rewind_ctx.entries[rewind_ctx.entry_head] = (RewindEntry){
		.offset = write_offset,
		.size = src_size,
		.state_size = state_size,
		.is_keyframe = is_keyframe ? 1 : 0,
	};

	rewind_ctx.head = write_offset + src_size;
	if (rewind_ctx.head >= rewind_ctx.capacity)
		rewind_ctx.head = 0;

	rewind_ctx.entry_head =
		(rewind_ctx.entry_head + 1) % rewind_ctx.entry_capacity;
	rewind_ctx.entry_count += 1;
	Rewind_dropLeadingDeltasLocked();
	return 1;
}

static int Rewind_compressStateLocked(const uint8_t *src, size_t state_size,
	int force_keyframe, size_t *dest_size, int *is_keyframe)
{
	const uint8_t *compress_src = src;
	int keyframe = force_keyframe;
	int result;
	size_t i;

	if (!rewind_ctx.prev_state_enc || !rewind_ctx.scratch || !dest_size)
		return 0;

	if (!keyframe && (!rewind_ctx.has_prev_enc ||
		rewind_ctx.prev_state_size != state_size)) {
		keyframe = 1;
	}

	if (!keyframe) {
		for (i = 0; i < state_size; i++) {
			rewind_ctx.delta_buf[i] =
				src[i] ^ rewind_ctx.prev_state_enc[i];
		}
		compress_src = rewind_ctx.delta_buf;
	}

	result = LZ4_compress_fast((const char *)compress_src,
		(char *)rewind_ctx.scratch, (int)state_size,
		(int)rewind_ctx.scratch_size, rewind_ctx.lz4_acceleration);
	if (result <= 0)
		return 0;

	memcpy(rewind_ctx.prev_state_enc, src, state_size);
	rewind_ctx.prev_state_size = state_size;
	rewind_ctx.has_prev_enc = 1;
	*dest_size = (size_t)result;
	if (is_keyframe)
		*is_keyframe = keyframe;
	return 1;
}

static int Rewind_captureState(uint8_t *state, size_t state_size)
{
	int ok;

	rewind_ctx.savestate_context =
		RETRO_SAVESTATE_CONTEXT_RUNAHEAD_SAME_INSTANCE;
	ok = core.serialize(state, state_size);
	rewind_ctx.savestate_context = RETRO_SAVESTATE_CONTEXT_NORMAL;
	return ok;
}

static int Rewind_captureSync(int force_keyframe)
{
	size_t state_size;
	size_t dest_size = 0;
	int is_keyframe = 1;
	uint32_t now_ms = SDL_GetTicks();

	if (!rewind_ctx.enabled)
		return 0;

	state_size = core.serialize_size();
	if (!state_size)
		return 0;

	if (state_size != rewind_ctx.state_size)
		return 0;
	if (!Rewind_captureState(rewind_ctx.state_buf, state_size))
		return 0;

	pthread_mutex_lock(&rewind_ctx.lock);
	if (!Rewind_compressStateLocked(rewind_ctx.state_buf, state_size,
		force_keyframe, &dest_size, &is_keyframe)) {
		pthread_mutex_unlock(&rewind_ctx.lock);
		return 0;
	}
	if (!Rewind_writeEntryLocked(rewind_ctx.scratch, dest_size, state_size,
		is_keyframe)) {
		pthread_mutex_unlock(&rewind_ctx.lock);
		return 0;
	}
	pthread_mutex_unlock(&rewind_ctx.lock);

	rewind_ctx.last_capture_ms = now_ms;
	if (is_keyframe)
		rewind_ctx.last_keyframe_ms = now_ms;
	return 1;
}

static int Rewind_shouldCapture(uint32_t now_ms)
{
	if (!rewind_ctx.last_capture_ms)
		return 1;
	return (int)(now_ms - rewind_ctx.last_capture_ms) >=
		rewind_ctx.capture_interval_ms;
}

static int Rewind_shouldKeyframe(uint32_t now_ms)
{
	if (!rewind_ctx.last_keyframe_ms)
		return 1;
	return (int)(now_ms - rewind_ctx.last_keyframe_ms) >=
		rewind_ctx.keyframe_interval_ms;
}

static int Rewind_queueCapture(size_t state_size, int force_keyframe,
	uint32_t now_ms)
{
	int slot = -1;

	if (!rewind_ctx.worker_running || !rewind_ctx.pool_size)
		return Rewind_captureSync(force_keyframe);

	pthread_mutex_lock(&rewind_ctx.queue_mx);
	if (rewind_ctx.free_count && rewind_ctx.queue_count <
		rewind_ctx.queue_capacity) {
		slot = rewind_ctx.free_stack[--rewind_ctx.free_count];
		rewind_ctx.capture_busy[slot] = 1;
	}
	pthread_mutex_unlock(&rewind_ctx.queue_mx);

	if (slot < 0)
		return 0;

	if (!Rewind_captureState(rewind_ctx.capture_pool[slot], state_size)) {
		pthread_mutex_lock(&rewind_ctx.queue_mx);
		rewind_ctx.capture_busy[slot] = 0;
		rewind_ctx.free_stack[rewind_ctx.free_count++] = slot;
		pthread_mutex_unlock(&rewind_ctx.queue_mx);
		return 0;
	}

	rewind_ctx.capture_sizes[slot] = state_size;
	rewind_ctx.capture_keyframe[slot] = force_keyframe ? 1 : 0;
	rewind_ctx.capture_gen[slot] = rewind_ctx.generation;

	pthread_mutex_lock(&rewind_ctx.queue_mx);
	rewind_ctx.queue[rewind_ctx.queue_tail] = slot;
	rewind_ctx.queue_tail =
		(rewind_ctx.queue_tail + 1) % rewind_ctx.queue_capacity;
	rewind_ctx.queue_count += 1;
	pthread_cond_signal(&rewind_ctx.queue_cv);
	pthread_mutex_unlock(&rewind_ctx.queue_mx);

	rewind_ctx.last_capture_ms = now_ms;
	if (force_keyframe)
		rewind_ctx.last_keyframe_ms = now_ms;
	return 1;
}

static int Rewind_buildStateLocked(int target_index, size_t *state_size_out)
{
	int i;
	int keyframe_index = -1;
	size_t state_size;

	if (target_index < 0 || target_index >= rewind_ctx.entry_count)
		return 0;

	for (i = target_index; i >= 0; i--) {
		RewindEntry *entry = Rewind_entryLocked(i);

		if (entry->is_keyframe) {
			keyframe_index = i;
			break;
		}
	}
	if (keyframe_index < 0)
		return 0;

	state_size = Rewind_entryLocked(keyframe_index)->state_size;
	if (!state_size || state_size > rewind_ctx.alloc_size)
		return 0;

	for (i = keyframe_index; i <= target_index; i++) {
		RewindEntry *entry = Rewind_entryLocked(i);
		int result;
		size_t j;

		if (entry->state_size != state_size)
			return 0;

		if (entry->offset + entry->size > rewind_ctx.capacity)
			return 0;

		result = LZ4_decompress_safe((const char *)rewind_ctx.buffer +
			entry->offset, (char *)(entry->is_keyframe ?
			rewind_ctx.state_buf : rewind_ctx.delta_buf),
			(int)entry->size, (int)state_size);
		if (result != (int)state_size)
			return 0;

		if (!entry->is_keyframe) {
			for (j = 0; j < state_size; j++) {
				rewind_ctx.state_buf[j] ^=
					rewind_ctx.delta_buf[j];
			}
		}
	}

	*state_size_out = state_size;
	return 1;
}

static int Rewind_applyState(const uint8_t *state, size_t state_size)
{
	int ok;

	rewind_ctx.savestate_context =
		RETRO_SAVESTATE_CONTEXT_RUNAHEAD_SAME_INSTANCE;
	ok = core.unserialize(state, state_size);
	rewind_ctx.savestate_context = RETRO_SAVESTATE_CONTEXT_NORMAL;
	return ok;
}

static void Rewind_trimPlayback(void)
{
	RewindEntry *entry;
	int keep_count;

	if (!rewinding || rewind_ctx.play_index < 0 || !rewind_ctx.entry_count)
		goto finish;

	pthread_mutex_lock(&rewind_ctx.lock);

	keep_count = rewind_ctx.play_index + 1;
	if (keep_count <= 0) {
		rewind_ctx.entry_head = 0;
		rewind_ctx.entry_tail = 0;
		rewind_ctx.entry_count = 0;
		rewind_ctx.head = 0;
		rewind_ctx.tail = 0;
		rewind_ctx.has_prev_enc = 0;
		rewind_ctx.prev_state_size = 0;
		pthread_mutex_unlock(&rewind_ctx.lock);
		goto finish;
	}

	entry = Rewind_entryLocked(keep_count - 1);
	rewind_ctx.entry_count = keep_count;
	rewind_ctx.entry_head =
		(rewind_ctx.entry_tail + keep_count) % rewind_ctx.entry_capacity;
	rewind_ctx.head = Rewind_entryEnd(entry);
	memcpy(rewind_ctx.prev_state_enc, rewind_ctx.state_buf,
		rewind_ctx.play_state_size);
	rewind_ctx.prev_state_size = rewind_ctx.play_state_size;
	rewind_ctx.has_prev_enc = 1;

	pthread_mutex_unlock(&rewind_ctx.lock);

finish:
	rewind_ctx.play_index = -1;
	rewind_ctx.play_state_size = 0;
	rewind_ctx.last_capture_ms = 0;
	rewind_ctx.last_keyframe_ms = 0;
	rewind_ctx.last_step_ms = 0;
	rewinding = 0;
}

static int Rewind_initInternal(size_t state_size)
{
	int buffer_mb;
	int capture_ms;
	int keyframe_ms;
	int lz4_accel;
	int pool_size;
	int entry_capacity;
	int i;

	Rewind_free();

	if (!Rewind_getEnable())
		return 0;
	if (!core.serialize_size || !core.serialize || !core.unserialize)
		return 0;
	if (!state_size)
		return 0;

	buffer_mb = Rewind_getOptionInt(FE_OPT_REWIND_BUFFER, REWIND_MIN_BUFFER_MB);
	capture_ms = Rewind_getOptionInt(FE_OPT_REWIND_CAPTURE, 16);
	keyframe_ms = Rewind_getOptionInt(FE_OPT_REWIND_KEYFRAME, 250);
	lz4_accel = Rewind_getOptionInt(FE_OPT_REWIND_COMPRESSION, 2);

	if (buffer_mb < REWIND_MIN_BUFFER_MB)
		buffer_mb = REWIND_MIN_BUFFER_MB;
	if (buffer_mb > REWIND_MAX_BUFFER_MB)
		buffer_mb = REWIND_MAX_BUFFER_MB;
	if (capture_ms < 1)
		capture_ms = 1;
	if (keyframe_ms < capture_ms)
		keyframe_ms = capture_ms;
	if (lz4_accel < 1)
		lz4_accel = 1;
	if (lz4_accel > REWIND_MAX_LZ4_ACCELERATION)
		lz4_accel = REWIND_MAX_LZ4_ACCELERATION;

	rewind_ctx.enabled = 1;
	rewind_ctx.audio = config.frontend.options[FE_OPT_REWIND_AUDIO].value;
	rewind_ctx.buffer_mb = buffer_mb;
	rewind_ctx.capture_interval_ms = capture_ms;
	rewind_ctx.keyframe_interval_ms = keyframe_ms;
	rewind_ctx.lz4_acceleration = lz4_accel;
	rewind_ctx.capacity = (size_t)buffer_mb * 1024 * 1024;
	rewind_ctx.state_size = state_size;
	rewind_ctx.alloc_size = Rewind_getAllocSize(state_size);
	rewind_ctx.scratch_size = LZ4_compressBound((int)rewind_ctx.alloc_size);
	rewind_ctx.play_index = -1;
	rewind_ctx.savestate_context = RETRO_SAVESTATE_CONTEXT_NORMAL;
	rewind_ctx.generation = 1;

	rewind_ctx.buffer = calloc(1, rewind_ctx.capacity);
	rewind_ctx.state_buf = calloc(1, rewind_ctx.alloc_size);
	rewind_ctx.delta_buf = calloc(1, rewind_ctx.alloc_size);
	rewind_ctx.scratch = calloc(1, rewind_ctx.scratch_size);
	rewind_ctx.prev_state_enc = calloc(1, rewind_ctx.alloc_size);
	if (!rewind_ctx.buffer || !rewind_ctx.state_buf ||
		!rewind_ctx.delta_buf || !rewind_ctx.scratch ||
		!rewind_ctx.prev_state_enc) {
		Rewind_free();
		return 0;
	}

	entry_capacity = (int)(rewind_ctx.capacity / REWIND_ENTRY_SIZE_HINT);
	if (entry_capacity < REWIND_MIN_ENTRIES)
		entry_capacity = REWIND_MIN_ENTRIES;
	rewind_ctx.entries = calloc(entry_capacity, sizeof(RewindEntry));
	if (!rewind_ctx.entries) {
		Rewind_free();
		return 0;
	}
	rewind_ctx.entry_capacity = entry_capacity;

	pthread_mutex_init(&rewind_ctx.lock, NULL);
	pthread_mutex_init(&rewind_ctx.queue_mx, NULL);
	pthread_cond_init(&rewind_ctx.queue_cv, NULL);
	rewind_ctx.locks_ready = 1;

	pool_size = Rewind_getPoolSize(state_size);
	rewind_ctx.capture_pool = calloc(pool_size, sizeof(uint8_t *));
	rewind_ctx.capture_sizes = calloc(pool_size, sizeof(size_t));
	rewind_ctx.capture_keyframe = calloc(pool_size, sizeof(uint8_t));
	rewind_ctx.capture_gen = calloc(pool_size, sizeof(unsigned int));
	rewind_ctx.capture_busy = calloc(pool_size, sizeof(uint8_t));
	rewind_ctx.free_stack = calloc(pool_size, sizeof(int));
	rewind_ctx.queue = calloc(pool_size, sizeof(int));
	if (!rewind_ctx.capture_pool || !rewind_ctx.capture_sizes ||
		!rewind_ctx.capture_keyframe || !rewind_ctx.capture_gen ||
		!rewind_ctx.capture_busy || !rewind_ctx.free_stack ||
		!rewind_ctx.queue) {
		Rewind_free();
		return 0;
	}

	for (i = 0; i < pool_size; i++) {
		rewind_ctx.capture_pool[i] = calloc(1, rewind_ctx.alloc_size);
		if (!rewind_ctx.capture_pool[i]) {
			Rewind_free();
			return 0;
		}
	}

	rewind_ctx.pool_size = pool_size;
	rewind_ctx.queue_capacity = pool_size;
	pthread_mutex_lock(&rewind_ctx.queue_mx);
	Rewind_clearQueueLocked();
	pthread_mutex_unlock(&rewind_ctx.queue_mx);

	if (pthread_create(&rewind_ctx.worker, NULL, Rewind_workerThread,
		NULL) == 0) {
		rewind_ctx.worker_running = 1;
	} else {
		LOG_warn("Rewind: worker thread unavailable, using sync path\n");
	}

	LOG_info("Rewind: enabled state=%zu alloc=%zu buffer=%iMB capture=%ims "
		"keyframe=%ims accel=%i quirks=0x%llx\n", rewind_ctx.state_size,
		rewind_ctx.alloc_size, rewind_ctx.buffer_mb,
		rewind_ctx.capture_interval_ms, rewind_ctx.keyframe_interval_ms,
		rewind_ctx.lz4_acceleration,
		(unsigned long long)core.serialization_quirks);
	return 1;
}

static int Rewind_reinitForStateSize(size_t state_size)
{
	if (!Rewind_initInternal(state_size))
		return 0;

	Rewind_resetHistory();
	return Rewind_captureSync(1);
}

static void *Rewind_workerThread(void *arg)
{
	(void)arg;

	while (1) {
		int slot;
		size_t state_size;
		size_t dest_size = 0;
		int is_keyframe = 0;
		unsigned int generation;
		unsigned int current_generation;

		pthread_mutex_lock(&rewind_ctx.queue_mx);
		while (!rewind_ctx.worker_stop && !rewind_ctx.queue_count)
			pthread_cond_wait(&rewind_ctx.queue_cv, &rewind_ctx.queue_mx);
		if (rewind_ctx.worker_stop && !rewind_ctx.queue_count) {
			pthread_mutex_unlock(&rewind_ctx.queue_mx);
			break;
		}

		slot = rewind_ctx.queue[rewind_ctx.queue_head];
		rewind_ctx.queue_head =
			(rewind_ctx.queue_head + 1) % rewind_ctx.queue_capacity;
		rewind_ctx.queue_count -= 1;
		state_size = rewind_ctx.capture_sizes[slot];
		generation = rewind_ctx.capture_gen[slot];
		current_generation = rewind_ctx.generation;
		pthread_mutex_unlock(&rewind_ctx.queue_mx);

		if (generation == current_generation) {
			pthread_mutex_lock(&rewind_ctx.lock);
			if (Rewind_compressStateLocked(rewind_ctx.capture_pool[slot],
				state_size, rewind_ctx.capture_keyframe[slot],
				&dest_size, &is_keyframe)) {
				Rewind_writeEntryLocked(rewind_ctx.scratch, dest_size,
					state_size, is_keyframe);
			}
			pthread_mutex_unlock(&rewind_ctx.lock);
		}

		pthread_mutex_lock(&rewind_ctx.queue_mx);
		rewind_ctx.capture_busy[slot] = 0;
		rewind_ctx.free_stack[rewind_ctx.free_count++] = slot;
		pthread_mutex_unlock(&rewind_ctx.queue_mx);
	}

	return NULL;
}

///////////////////////////////////////

int setRewindToggle(int enable)
{
	if (enable)
		setFastForward(0);
	rewind_toggle = enable ? 1 : 0;
	if (!rewind_toggle && !rewind_pressed)
		rewinding = 0;
	return rewind_toggle;
}

int setRewindPressed(int enable)
{
	if (enable)
		setFastForward(0);
	rewind_pressed = enable ? 1 : 0;
	return rewind_pressed;
}

void Rewind_init(void)
{
	size_t state_size = 0;

	if (!Rewind_getEnable()) {
		Rewind_free();
		return;
	}

	if (core.serialize_size)
		state_size = core.serialize_size();
	Rewind_initInternal(state_size);
}

void Rewind_quit(void)
{
	Rewind_free();
}

void Rewind_applyConfig(void)
{
	if (!core.initialized) {
		Rewind_init();
		return;
	}

	if (!core.serialize_size) {
		Rewind_quit();
		return;
	}

	Rewind_reinitForStateSize(core.serialize_size());
}

void Rewind_afterFrame(void)
{
	uint32_t now_ms;
	size_t state_size;
	int force_keyframe;

	if (!rewind_ctx.enabled || rewinding)
		return;
	if (!Rewind_getEnable())
		return;

	now_ms = SDL_GetTicks();
	if (!Rewind_shouldCapture(now_ms))
		return;

	state_size = core.serialize_size();
	if (!state_size)
		return;
	if (state_size != rewind_ctx.state_size) {
		Rewind_reinitForStateSize(state_size);
		return;
	}

	force_keyframe = Rewind_shouldKeyframe(now_ms);
	if (!Rewind_queueCapture(state_size, force_keyframe, now_ms)) {
		LOG_warn("Rewind: dropped capture\n");
	}
}

int Rewind_processFrame(void)
{
	int target_index;
	uint32_t now_ms;

	/* During rewind playback retro_run() is skipped, so refresh frontend
	 * input here to observe hold-release events and avoid latching rewind. */
	if (rewinding || rewind_pressed || rewind_toggle)
		input_poll_callback();

	if (!rewind_ctx.enabled || !Rewind_getEnable()) {
		if (rewinding)
			Rewind_trimPlayback();
		return 0;
	}

	if (!rewind_pressed && !rewind_toggle) {
		if (rewinding)
			Rewind_trimPlayback();
		return 0;
	}

	if (fast_forward)
		setFastForward(0);

	if (!rewinding)
		Rewind_waitForWorkerIdle();

	now_ms = SDL_GetTicks();
	if (rewind_ctx.last_step_ms &&
		(int)(now_ms - rewind_ctx.last_step_ms) <
		rewind_ctx.capture_interval_ms) {
		return 1;
	}

	pthread_mutex_lock(&rewind_ctx.lock);
	if (rewind_ctx.entry_count < 2) {
		pthread_mutex_unlock(&rewind_ctx.lock);
		if (rewind_toggle)
			rewind_toggle = 0;
		return 1;
	}

	target_index = rewinding ? (rewind_ctx.play_index - 1) :
		(rewind_ctx.entry_count - 2);
	if (target_index < 0) {
		pthread_mutex_unlock(&rewind_ctx.lock);
		if (rewind_toggle)
			rewind_toggle = 0;
		return 1;
	}

	if (!Rewind_buildStateLocked(target_index, &rewind_ctx.play_state_size)) {
		pthread_mutex_unlock(&rewind_ctx.lock);
		if (rewind_toggle)
			rewind_toggle = 0;
		return 1;
	}

	rewind_ctx.play_index = target_index;
	pthread_mutex_unlock(&rewind_ctx.lock);

	if (!Rewind_applyState(rewind_ctx.state_buf, rewind_ctx.play_state_size)) {
		if (rewind_toggle)
			rewind_toggle = 0;
		return 1;
	}

	rewinding = 1;
	rewind_ctx.last_step_ms = now_ms;

	/* Restoring a savestate does not refresh the framebuffer by itself.
	 * Run one frame while rewind is active so the core emits video for the
	 * rewound state without re-capturing history. */
	core.run();
	return 1;
}

void Rewind_onStateChange(void)
{
	size_t state_size;

	if (!rewind_ctx.enabled)
		return;

	state_size = core.serialize_size();
	if (!state_size)
		return;
	if (state_size != rewind_ctx.state_size) {
		Rewind_reinitForStateSize(state_size);
		return;
	}

	Rewind_resetHistory();
	Rewind_captureSync(1);
}

enum retro_savestate_context Rewind_getSavestateContext(void)
{
	return rewind_ctx.savestate_context;
}

int Rewind_audioEnabled(void)
{
	return rewind_ctx.audio;
}
