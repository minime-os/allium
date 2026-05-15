#include "minarch.h"

Core core;
int state_slot = 0;
struct retro_disk_control_ext_callback disk_control_ext;

///////////////////////////////////////

static void Core_resolveSaveId(char *out_save_id, size_t out_save_id_size)
{
	if (!out_save_id || out_save_id_size == 0)
		return;
	out_save_id[0] = '\0';
	if (CORE_REGISTRY_resolveSaveIdForLaunchTag(core.tag, out_save_id,
			out_save_id_size) != 0) {
		snprintf(out_save_id, out_save_id_size, "%s", core.name);
	}
}

///////////////////////////////////////

static void SRAM_getPath(char *filename)
{
	sprintf(filename, "%s/%s.sav", core.saves_dir, game.name);
}

void SRAM_read(void)
{
	size_t sram_size = core.get_memory_size(RETRO_MEMORY_SAVE_RAM);
	if (!sram_size)
		return;

	char filename[MAX_PATH];
	SRAM_getPath(filename);
	printf("sav path (read): %s\n", filename);

	FILE *sram_file = fopen(filename, "r");
	if (!sram_file)
		return;

	void *sram = core.get_memory_data(RETRO_MEMORY_SAVE_RAM);
	if (!sram || !fread(sram, 1, sram_size, sram_file)) {
		LOG_error("Error reading SRAM data\n");
	}

	fclose(sram_file);
}

void SRAM_write(void)
{
	size_t sram_size = core.get_memory_size(RETRO_MEMORY_SAVE_RAM);
	if (!sram_size)
		return;

	char filename[MAX_PATH];
	SRAM_getPath(filename);
	printf("sav path (write): %s\n", filename);

	FILE *sram_file = fopen(filename, "w");
	if (!sram_file) {
		LOG_error("Error opening SRAM file: %s\n", strerror(errno));
		return;
	}

	void *sram = core.get_memory_data(RETRO_MEMORY_SAVE_RAM);
	if (!sram || sram_size != fwrite(sram, 1, sram_size, sram_file)) {
		LOG_error("Error writing SRAM data to file\n");
	}

	fclose(sram_file);
	sync();
}

///////////////////////////////////////

static void RTC_getPath(char *filename)
{
	sprintf(filename, "%s/%s.rtc", core.saves_dir, game.name);
}

void RTC_read(void)
{
	size_t rtc_size = core.get_memory_size(RETRO_MEMORY_RTC);
	if (!rtc_size)
		return;

	char filename[MAX_PATH];
	RTC_getPath(filename);
	printf("rtc path (read): %s\n", filename);

	FILE *rtc_file = fopen(filename, "r");
	if (!rtc_file)
		return;

	void *rtc = core.get_memory_data(RETRO_MEMORY_RTC);
	if (!rtc || !fread(rtc, 1, rtc_size, rtc_file)) {
		LOG_error("Error reading RTC data\n");
	}

	fclose(rtc_file);
}

void RTC_write(void)
{
	size_t rtc_size = core.get_memory_size(RETRO_MEMORY_RTC);
	if (!rtc_size)
		return;

	char filename[MAX_PATH];
	RTC_getPath(filename);
	printf("rtc path (write) size(%zu): %s\n", rtc_size, filename);

	FILE *rtc_file = fopen(filename, "w");
	if (!rtc_file) {
		LOG_error("Error opening RTC file: %s\n", strerror(errno));
		return;
	}

	void *rtc = core.get_memory_data(RETRO_MEMORY_RTC);
	if (!rtc || rtc_size != fwrite(rtc, 1, rtc_size, rtc_file)) {
		LOG_error("Error writing RTC data to file\n");
	}

	fclose(rtc_file);
	sync();
}

///////////////////////////////////////

void State_getPath(char *filename)
{
	sprintf(filename, "%s/%s.st%i", core.states_dir, game.name, state_slot);
}

void State_read(void)
{
	size_t state_size = core.serialize_size();
	FILE *state_file = NULL;
	int restored = 0;

	if (!state_size)
		return;

	int was_ff = fast_forward;
	fast_forward = 0;

	void *state = calloc(1, state_size);
	if (!state) {
		LOG_error("Couldn't allocate memory for state\n");
		goto error;
	}

	char filename[MAX_PATH];
	State_getPath(filename);

	state_file = fopen(filename, "r");
	if (!state_file) {
		if (state_slot != 8) {
			LOG_error("Error opening state file: %s (%s)\n",
				filename, strerror(errno));
		}
		goto error;
	}

	if (state_size < fread(state, 1, state_size, state_file)) {
		LOG_error("Error reading state data from file: %s (%s)\n",
			filename, strerror(errno));
		goto error;
	}

	if (!core.unserialize(state, state_size)) {
		LOG_error("Error restoring save state: %s (%s)\n",
			filename, strerror(errno));
		goto error;
	}
	restored = 1;

error:
	if (state)
		free(state);
	if (state_file)
		fclose(state_file);

	fast_forward = was_ff;
	if (restored)
		Rewind_onStateChange();
}

void State_write(void)
{
	size_t state_size = core.serialize_size();
	FILE *state_file = NULL;

	if (!state_size)
		return;

	int was_ff = fast_forward;
	fast_forward = 0;

	void *state = calloc(1, state_size);
	if (!state) {
		LOG_error("Couldn't allocate memory for state\n");
		goto error;
	}

	char filename[MAX_PATH];
	State_getPath(filename);

	state_file = fopen(filename, "w");
	if (!state_file) {
		LOG_error("Error opening state file: %s (%s)\n",
			filename, strerror(errno));
		goto error;
	}

	if (!core.serialize(state, state_size)) {
		LOG_error("Error creating save state: %s (%s)\n",
			filename, strerror(errno));
		goto error;
	}

	if (state_size != fwrite(state, 1, state_size, state_file)) {
		LOG_error("Error writing state data to file: %s (%s)\n",
			filename, strerror(errno));
		goto error;
	}

error:
	if (state)
		free(state);
	if (state_file)
		fclose(state_file);

	sync();
	fast_forward = was_ff;
}

void State_autosave(void)
{
	int last_state_slot = state_slot;

	state_slot = AUTO_RESUME_SLOT;
	State_write();
	state_slot = last_state_slot;
}

void State_resume(void)
{
	if (!exists(RESUME_SLOT_PATH))
		return;

	int last_state_slot = state_slot;

	state_slot = getInt(RESUME_SLOT_PATH);
	unlink(RESUME_SLOT_PATH);
	State_read();
	state_slot = last_state_slot;
}

///////////////////////////////////////

static bool set_rumble_state(unsigned port, enum retro_rumble_effect effect,
	uint16_t strength)
{
	VIB_setStrength(strength);
	return 1;
}

bool environment_callback(unsigned cmd, void *data)
{
	switch (cmd) {
	case RETRO_ENVIRONMENT_GET_OVERSCAN: {
		bool *out = (bool *)data;
		if (out)
			*out = true;
		break;
	}
	case RETRO_ENVIRONMENT_GET_CAN_DUPE: {
		bool *out = (bool *)data;
		if (out)
			*out = true;
		break;
	}
	case RETRO_ENVIRONMENT_SET_MESSAGE: {
		const struct retro_message *message =
			(const struct retro_message *)data;
		if (message)
			LOG_info("%s\n", message->msg);
		break;
	}
	case RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL: {
	}
	case RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY: {
		const char **out = (const char **)data;
		if (out)
			*out = core.bios_dir;
		break;
	}
	case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: {
		const enum retro_pixel_format *format =
			(enum retro_pixel_format *)data;

		if (*format != RETRO_PIXEL_FORMAT_RGB565)
			return false;
		break;
	}
	case RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS: {
		Input_init((const struct retro_input_descriptor *)data);
		return false;
	}
	case RETRO_ENVIRONMENT_SET_DISK_CONTROL_INTERFACE: {
		const struct retro_disk_control_callback *var =
			(const struct retro_disk_control_callback *)data;
		if (var) {
			memset(&disk_control_ext, 0, sizeof(disk_control_ext));
			memcpy(&disk_control_ext, var,
				sizeof(struct retro_disk_control_callback));
		}
		break;
	}
	case RETRO_ENVIRONMENT_GET_VARIABLE: {
		struct retro_variable *var = (struct retro_variable *)data;
		if (var && var->key)
			var->value = OptionList_getOptionValue(&config.core,
				var->key);
		break;
	}
	case RETRO_ENVIRONMENT_SET_VARIABLES: {
		const struct retro_variable *vars =
			(const struct retro_variable *)data;
		if (vars) {
			OptionList_reset();
			OptionList_vars(vars);
		}
		break;
	}
	case RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME: {
		bool flag = *(bool *)data;
		(void)flag;
		break;
	}
	case RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE: {
		bool *out = (bool *)data;
		if (out) {
			*out = config.core.changed;
			config.core.changed = 0;
		}
		break;
	}
	case RETRO_ENVIRONMENT_SET_FRAME_TIME_CALLBACK:
	case RETRO_ENVIRONMENT_SET_AUDIO_CALLBACK:
		break;
	case RETRO_ENVIRONMENT_SET_SERIALIZATION_QUIRKS: {
		const uint64_t *quirks = (const uint64_t *)data;
		if (quirks)
			core.serialization_quirks = *quirks;
		break;
	}
	case RETRO_ENVIRONMENT_GET_RUMBLE_INTERFACE: {
		struct retro_rumble_interface *iface =
			(struct retro_rumble_interface *)data;
		if (iface)
			iface->set_rumble_state = set_rumble_state;
		break;
	}
	case RETRO_ENVIRONMENT_GET_INPUT_DEVICE_CAPABILITIES: {
		unsigned *out = (unsigned *)data;
		if (out)
			*out = (1 << RETRO_DEVICE_JOYPAD) |
				(1 << RETRO_DEVICE_ANALOG);
		break;
	}
	case RETRO_ENVIRONMENT_GET_LOG_INTERFACE: {
		struct retro_log_callback *log_cb =
			(struct retro_log_callback *)data;
		if (log_cb)
			log_cb->log = (void (*)(enum retro_log_level,
				const char *, ...))LOG_note;
		break;
	}
	case RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY: {
		const char **out = (const char **)data;
		if (out)
			*out = core.saves_dir;
		break;
	}
	case RETRO_ENVIRONMENT_SET_CONTROLLER_INFO: {
		const struct retro_controller_info *infos =
			(const struct retro_controller_info *)data;
		if (infos) {
			const struct retro_controller_info *info = &infos[0];
			for (int i = 0; i < info->num_types; i++) {
				const struct retro_controller_description *type =
					&info->types[i];
				if (!exactMatch((char *)type->desc, "dualshock"))
					continue;
				has_custom_controllers = 1;
				break;
			}
		}
		fflush(stdout);
		return false;
	}
	case RETRO_ENVIRONMENT_GET_CURRENT_SOFTWARE_FRAMEBUFFER:
		break;
	case RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE: {
		enum retro_av_enable_flags *out_p =
			(enum retro_av_enable_flags *)data;
		if (out_p) {
			int out = 0;
			out |= RETRO_AV_ENABLE_VIDEO;
			out |= RETRO_AV_ENABLE_AUDIO;
			*out_p = out;
		}
		break;
	}
	case RETRO_ENVIRONMENT_GET_SAVESTATE_CONTEXT: {
		enum retro_savestate_context *out =
			(enum retro_savestate_context *)data;
		if (out)
			*out = Rewind_getSavestateContext();
		break;
	}
	case RETRO_ENVIRONMENT_GET_INPUT_BITMASKS: {
		bool *out = (bool *)data;
		if (out)
			*out = true;
		break;
	}
	case RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION: {
		unsigned *out = (unsigned *)data;
		if (out)
			*out = 1;
		break;
	}
	case RETRO_ENVIRONMENT_SET_CORE_OPTIONS: {
		if (data) {
			OptionList_reset();
			OptionList_init(
				(const struct retro_core_option_definition *)data);
		}
		break;
	}
	case RETRO_ENVIRONMENT_SET_CORE_OPTIONS_INTL: {
		const struct retro_core_options_intl *options =
			(const struct retro_core_options_intl *)data;
		if (options && options->us) {
			OptionList_reset();
			OptionList_init(options->us);
		}
		break;
	}
	case RETRO_ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY:
		break;
	case RETRO_ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION: {
		unsigned *out = (unsigned *)data;
		if (out)
			*out = 1;
		break;
	}
	case RETRO_ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE: {
		const struct retro_disk_control_ext_callback *var =
			(const struct retro_disk_control_ext_callback *)data;
		if (var)
			memcpy(&disk_control_ext, var, sizeof(disk_control_ext));
		break;
	}
	case RETRO_ENVIRONMENT_SET_CONTENT_INFO_OVERRIDE:
		break;
	case RETRO_ENVIRONMENT_SET_VARIABLE: {
		const struct retro_variable *var =
			(const struct retro_variable *)data;
		if (var && var->key) {
			OptionList_setOptionValue(&config.core, var->key,
				var->value);
			break;
		}

		int *out = (int *)data;
		if (out)
			*out = 1;
		break;
	}
	default:
		return false;
	}
	return true;
}

///////////////////////////////////////

void audio_sample_callback(int16_t left, int16_t right)
{
	if (!fast_forward && (!rewinding || Rewind_audioEnabled()))
		SND_batchSamples(&(const SND_Frame){left, right}, 1);
}

size_t audio_sample_batch_callback(const int16_t *data, size_t frames)
{
	if (!fast_forward && (!rewinding || Rewind_audioEnabled()))
		return SND_batchSamples((const SND_Frame *)data, frames);
	return frames;
}

///////////////////////////////////////

void Core_getName(char *in_name, char *out_name)
{
	strcpy(out_name, basename(in_name));
	char *tmp = strrchr(out_name, '_');
	tmp[0] = '\0';
}

void Core_open(const char *core_path, const char *tag_name)
{
	LOG_info("Core_open\n");
	memset(&core, 0, sizeof(core));
	core.handle = dlopen(core_path, RTLD_LAZY);

	if (!core.handle)
		LOG_error("%s\n", dlerror());

	core.init = dlsym(core.handle, "retro_init");
	core.deinit = dlsym(core.handle, "retro_deinit");
	core.get_system_info = dlsym(core.handle, "retro_get_system_info");
	core.get_system_av_info = dlsym(core.handle, "retro_get_system_av_info");
	core.set_controller_port_device = dlsym(core.handle,
		"retro_set_controller_port_device");
	core.reset = dlsym(core.handle, "retro_reset");
	core.run = dlsym(core.handle, "retro_run");
	core.serialize_size = dlsym(core.handle, "retro_serialize_size");
	core.serialize = dlsym(core.handle, "retro_serialize");
	core.unserialize = dlsym(core.handle, "retro_unserialize");
	core.load_game = dlsym(core.handle, "retro_load_game");
	core.load_game_special = dlsym(core.handle, "retro_load_game_special");
	core.unload_game = dlsym(core.handle, "retro_unload_game");
	core.get_region = dlsym(core.handle, "retro_get_region");
	core.get_memory_data = dlsym(core.handle, "retro_get_memory_data");
	core.get_memory_size = dlsym(core.handle, "retro_get_memory_size");

	void (*set_environment_callback)(retro_environment_t);
	void (*set_video_refresh_callback)(retro_video_refresh_t);
	void (*set_audio_sample_callback)(retro_audio_sample_t);
	void (*set_audio_sample_batch_callback)(retro_audio_sample_batch_t);
	void (*set_input_poll_callback)(retro_input_poll_t);
	void (*set_input_state_callback)(retro_input_state_t);

	set_environment_callback = dlsym(core.handle, "retro_set_environment");
	set_video_refresh_callback = dlsym(core.handle, "retro_set_video_refresh");
	set_audio_sample_callback = dlsym(core.handle, "retro_set_audio_sample");
	set_audio_sample_batch_callback = dlsym(core.handle,
		"retro_set_audio_sample_batch");
	set_input_poll_callback = dlsym(core.handle, "retro_set_input_poll");
	set_input_state_callback = dlsym(core.handle, "retro_set_input_state");

	struct retro_system_info info = {};
	core.get_system_info(&info);

	Core_getName((char *)core_path, core.name);
	sprintf(core.version, "%s (%s)", info.library_name, info.library_version);
	strcpy(core.tag, tag_name);
	strcpy(core.extensions, info.valid_extensions);
	core.need_fullpath = info.need_fullpath;
	core.game_loaded = 0;

	LOG_info("core: %s version: %s tag: %s (valid_extensions: %s "
		"need_fullpath: %i)\n", core.name, core.version, core.tag,
		info.valid_extensions, info.need_fullpath);

	snprintf(core.config_dir, sizeof(core.config_dir), USERDATA_PATH "/%s-%s",
		core.tag, core.name);
	core.states_dir[0] = '\0';
	core.saves_dir[0] = '\0';
	{
		const char *bios_dir = getenv("MINUI_BIOS_DIR");
		if (!bios_dir || !bios_dir[0])
			bios_dir = getenv("BIOS_PATH");
		if (bios_dir && bios_dir[0]) {
			snprintf(core.bios_dir, sizeof(core.bios_dir), "%s",
				bios_dir);
		} else {
			snprintf(core.bios_dir, sizeof(core.bios_dir), "%s",
				SDCARD_PATH "/bios");
		}
	}
	if (ensure_dir_recursive(core.config_dir, 0755) != 0) {
		LOG_error("Error creating core config dir: %s (%s)\n",
			core.config_dir, strerror(errno));
	}

	set_environment_callback(environment_callback);
	set_video_refresh_callback(video_refresh_callback);
	set_audio_sample_callback(audio_sample_callback);
	set_audio_sample_batch_callback(audio_sample_batch_callback);
	set_input_poll_callback(input_poll_callback);
	set_input_state_callback(input_state_callback);
}

void Core_init(void)
{
	LOG_info("Core_init\n");
	core.init();
	core.initialized = 1;
}

static void Core_refreshGameStoragePaths(void)
{
	const char *storage_root = Game_storageRoot();
	char save_id[32];

	Core_resolveSaveId(save_id, sizeof(save_id));
	snprintf(core.states_dir, sizeof(core.states_dir), "%s/saves/%s/%s",
		storage_root, save_id, game.name);
	snprintf(core.saves_dir, sizeof(core.saves_dir), "%s/sram",
		core.states_dir);
	if (ensure_dir_recursive(core.config_dir, 0755) != 0) {
		LOG_error("Error creating core config dir: %s (%s)\n",
			core.config_dir, strerror(errno));
	}
	if (ensure_dir_recursive(core.states_dir, 0755) != 0) {
		LOG_error("Error creating states dir: %s (%s)\n",
			core.states_dir, strerror(errno));
	}
	if (ensure_dir_recursive(core.saves_dir, 0755) != 0) {
		LOG_error("Error creating saves dir: %s (%s)\n",
			core.saves_dir, strerror(errno));
	}
}

int Core_load(void)
{
	LOG_info("Core_load\n");
	struct retro_game_info game_info;

	game_info.path = game.tmp_path[0] ? game.tmp_path : game.path;
	game_info.data = game.data;
	game_info.size = game.size;
	Core_refreshGameStoragePaths();
	LOG_info("game path: %s (%i)\n", game_info.path, game.size);

	if (!core.load_game(&game_info)) {
		LOG_error("retro_load_game failed for %s\n", game_info.path);
		return -1;
	}
	core.game_loaded = 1;

	SRAM_read();
	RTC_read();

	struct retro_system_av_info av_info = {};
	core.get_system_av_info(&av_info);
	core.set_controller_port_device(0, RETRO_DEVICE_JOYPAD);

	core.fps = av_info.timing.fps;
	core.sample_rate = av_info.timing.sample_rate;
	double a = av_info.geometry.aspect_ratio;
	if (a <= 0)
		a = (double)av_info.geometry.base_width /
			av_info.geometry.base_height;
	core.aspect_ratio = a;

	LOG_info("aspect_ratio: %f (%ix%i) fps: %f\n", a,
		av_info.geometry.base_width, av_info.geometry.base_height,
		core.fps);

	return 0;
}

void Core_reset(void)
{
	core.reset();
	Rewind_onStateChange();
}

void Core_unload(void)
{
	SND_quit();
}

void Core_quit(void)
{
	if (core.initialized) {
		if (core.game_loaded) {
			SRAM_write();
			RTC_write();
			core.unload_game();
			core.game_loaded = 0;
		}
		core.deinit();
		core.initialized = 0;
	}
}

void Core_close(void)
{
	if (core.handle)
		dlclose(core.handle);
}
