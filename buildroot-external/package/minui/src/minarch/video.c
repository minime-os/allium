#include "minarch.h"

int DEVICE_WIDTH = 0;
int DEVICE_HEIGHT = 0;
int DEVICE_PITCH = 0;
uint32_t sec_start = 0;
GFX_Renderer renderer;

static int cpu_ticks = 0;
static int fps_ticks = 0;
static int use_ticks = 0;
static double fps_double = 0;
static double cpu_double = 0;
static double use_double = 0;

#ifdef USES_SWSCALER
static int fit = 1;
#else
static int fit = 0;
#endif

static void *buffer = NULL;
static SDL_Surface *digits;

#define DIGIT_WIDTH 9
#define DIGIT_HEIGHT 8
#define DIGIT_TRACKING -2

enum {
	DIGIT_SLASH = 10,
	DIGIT_DOT,
	DIGIT_PERCENT,
	DIGIT_X,
	DIGIT_OP,
	DIGIT_CP,
	DIGIT_COUNT,
};

#define DIGIT_SPACE DIGIT_COUNT

static const char *bitmap_font[] = {
	['0'] =
		" 111 "
		"1   1"
		"1   1"
		"1  11"
		"1 1 1"
		"11  1"
		"1   1"
		"1   1"
		" 111 ",
	['1'] =
		"   1 "
		" 111 "
		"   1 "
		"   1 "
		"   1 "
		"   1 "
		"   1 "
		"   1 "
		"   1 ",
	['2'] =
		" 111 "
		"1   1"
		"    1"
		"   1 "
		"  1  "
		" 1   "
		"1    "
		"1    "
		"11111",
	['3'] =
		" 111 "
		"1   1"
		"    1"
		"    1"
		" 111 "
		"    1"
		"    1"
		"1   1"
		" 111 ",
	['4'] =
		"1   1"
		"1   1"
		"1   1"
		"1   1"
		"1   1"
		"1   1"
		"11111"
		"    1"
		"    1",
	['5'] =
		"11111"
		"1    "
		"1    "
		"1111 "
		"    1"
		"    1"
		"    1"
		"1   1"
		" 111 ",
	['6'] =
		" 111 "
		"1    "
		"1    "
		"1111 "
		"1   1"
		"1   1"
		"1   1"
		"1   1"
		" 111 ",
	['7'] =
		"11111"
		"    1"
		"    1"
		"   1 "
		"  1  "
		"  1  "
		"  1  "
		"  1  "
		"  1  ",
	['8'] =
		" 111 "
		"1   1"
		"1   1"
		"1   1"
		" 111 "
		"1   1"
		"1   1"
		"1   1"
		" 111 ",
	['9'] =
		" 111 "
		"1   1"
		"1   1"
		"1   1"
		"1   1"
		" 1111"
		"    1"
		"    1"
		" 111 ",
	['.'] =
		"     "
		"     "
		"     "
		"     "
		"     "
		"     "
		"     "
		" 11  "
		" 11  ",
	[','] =
		"     "
		"     "
		"     "
		"     "
		"     "
		"     "
		"  1  "
		"  1  "
		" 1   ",
	[' '] =
		"     "
		"     "
		"     "
		"     "
		"     "
		"     "
		"     "
		"     "
		"     ",
	['('] =
		"   1 "
		"  1  "
		" 1   "
		" 1   "
		" 1   "
		" 1   "
		" 1   "
		"  1  "
		"   1 ",
	[')'] =
		" 1   "
		"  1  "
		"   1 "
		"   1 "
		"   1 "
		"   1 "
		"   1 "
		"  1  "
		" 1   ",
	['/'] =
		"   1 "
		"   1 "
		"   1 "
		"  1  "
		"  1  "
		"  1  "
		" 1   "
		" 1   "
		" 1   ",
	['x'] =
		"     "
		"     "
		"1   1"
		"1   1"
		" 1 1 "
		"  1  "
		" 1 1 "
		"1   1"
		"1   1",
	['%'] =
		" 1   "
		"1 1  "
		"1 1 1"
		" 1 1 "
		"  1  "
		" 1 1 "
		"1 1 1"
		"  1 1"
		"   1 ",
	['-'] =
		"     "
		"     "
		"     "
		"     "
		" 111 "
		"     "
		"     "
		"     "
		"     ",
};

void hdmimon(void)
{
	static int had_hdmi = -1;
	int has_hdmi = GetHDMI();

	if (had_hdmi == -1)
		had_hdmi = has_hdmi;
	if (has_hdmi != had_hdmi) {
		had_hdmi = has_hdmi;
		LOG_info("restarting after HDMI change...\n");
		Menu_beforeSleep();
		sleep(4);
		show_menu = 0;
		quit = 1;
	}
}

void MSG_init(void)
{
	digits = SDL_CreateRGBSurface(SDL_SWSURFACE,
		SCALE2(DIGIT_WIDTH * DIGIT_COUNT, DIGIT_HEIGHT),
		FIXED_DEPTH, 0, 0, 0, 0);
	SDL_FillRect(digits, NULL, RGB_BLACK);

	SDL_Surface *digit;
	char *chars[] = {
		"0", "1", "2", "3", "4", "5", "6", "7",
		"8", "9", "/", ".", "%", "x", "(", ")", NULL
	};
	char *c;
	int i = 0;

	while ((c = chars[i])) {
		digit = TTF_RenderUTF8_Blended(font.tiny, c, COLOR_WHITE);
		SDL_BlitSurface(digit, NULL, digits, &(SDL_Rect){
			(i * SCALE1(DIGIT_WIDTH)) +
				(SCALE1(DIGIT_WIDTH) - digit->w) / 2,
			(SCALE1(DIGIT_HEIGHT) - digit->h) / 2
		});
		SDL_FreeSurface(digit);
		i += 1;
	}
}

static int MSG_blitChar(int n, int x, int y)
{
	if (n != DIGIT_SPACE) {
		SDL_BlitSurface(digits, &(SDL_Rect){
			n * SCALE1(DIGIT_WIDTH), 0,
			SCALE2(DIGIT_WIDTH, DIGIT_HEIGHT)
		}, screen, &(SDL_Rect){x, y});
	}
	return x + SCALE1(DIGIT_WIDTH + DIGIT_TRACKING);
}

static int MSG_blitInt(int num, int x, int y)
{
	int i = num;
	int n;

	if (i > 999) {
		n = i / 1000;
		i -= n * 1000;
		x = MSG_blitChar(n, x, y);
	}
	if (i > 99) {
		n = i / 100;
		i -= n * 100;
		x = MSG_blitChar(n, x, y);
	} else if (num > 99) {
		x = MSG_blitChar(0, x, y);
	}
	if (i > 9) {
		n = i / 10;
		i -= n * 10;
		x = MSG_blitChar(n, x, y);
	} else if (num > 9) {
		x = MSG_blitChar(0, x, y);
	}

	n = i;
	x = MSG_blitChar(n, x, y);
	return x;
}

static int MSG_blitDouble(double num, int x, int y)
{
	int i = num;
	int r = (num - i) * 10;
	int n;

	x = MSG_blitInt(i, x, y);
	n = DIGIT_DOT;
	x = MSG_blitChar(n, x, y);
	n = r;
	x = MSG_blitChar(n, x, y);
	return x;
}

void MSG_quit(void)
{
	SDL_FreeSurface(digits);
}

static void blitBitmapText(char *text, int ox, int oy, uint16_t *data,
	int stride, int width, int height)
{
#define CHAR_WIDTH 5
#define CHAR_HEIGHT 9
#define LETTERSPACING 1
	int len = strlen(text);
	int w = ((CHAR_WIDTH + LETTERSPACING) * len) - 1;
	int h = CHAR_HEIGHT;

	if (ox < 0)
		ox = width - w + ox;
	if (oy < 0)
		oy = height - h + oy;

	data += oy * stride + ox;
	uint16_t *row = data - stride;
	memset(row - 1, 0, (w + 2) * 2);
	for (int y = 0; y < CHAR_HEIGHT; y++) {
		row = data + y * stride;
		memset(row - 1, 0, (w + 2) * 2);
		for (int i = 0; i < len; i++) {
			const char *c = bitmap_font[(int)text[i]];
			for (int x = 0; x < CHAR_WIDTH; x++) {
				int j = y * CHAR_WIDTH + x;
				if (c[j] == '1')
					*row = 0xffff;
				row++;
			}
			row += LETTERSPACING;
		}
	}
	row = data + CHAR_HEIGHT * stride;
	memset(row - 1, 0, (w + 2) * 2);
}

void buffer_dealloc(void)
{
	if (!buffer)
		return;
	free(buffer);
	buffer = NULL;
}

static void buffer_realloc(int w, int h, int p)
{
	(void)p;
	buffer_dealloc();
	buffer = malloc((w * FIXED_BPP) * h);
}

static void buffer_downsample(const void *data, unsigned width, unsigned height,
	size_t pitch)
{
	const uint32_t *input = data;
	uint16_t *output = buffer;
	size_t extra = pitch / sizeof(uint32_t) - width;

	for (int y = 0; y < (int)height; y++) {
		for (int x = 0; x < (int)width; x++) {
			*output = (*input & 0xF80000) >> 8;
			*output |= (*input & 0xFC00) >> 5;
			*output |= (*input & 0xF8) >> 3;
			input++;
			output++;
		}
		input += extra;
	}
}

void selectScaler(int src_w, int src_h, int src_p)
{
	LOG_info("selectScaler\n");

	if (downsample)
		buffer_realloc(src_w, src_h, src_p);

	int src_x;
	int src_y;
	int dst_x;
	int dst_y;
	int dst_w;
	int dst_h;
	int dst_p;
	int scale;
	int aspect_w = src_w;
	int aspect_h = CEIL_DIV(aspect_w, core.aspect_ratio);

	if (aspect_h < src_h) {
		aspect_h = src_h;
		aspect_w = aspect_h * core.aspect_ratio;
		aspect_w += aspect_w % 2;
	}

	char scaler_name[16];
	src_x = 0;
	src_y = 0;
	dst_x = 0;
	dst_y = 0;
	renderer.true_w = src_w;
	renderer.true_h = src_h;

	int scaling = screen_scaling;
	if (scaling == SCALE_CROPPED && DEVICE_WIDTH == HDMI_WIDTH)
		scaling = SCALE_NATIVE;

	if (scaling == SCALE_NATIVE || scaling == SCALE_CROPPED) {
		scale = MIN(DEVICE_WIDTH / src_w, DEVICE_HEIGHT / src_h);
		if (!scale) {
			sprintf(scaler_name, "forced crop");
			dst_w = DEVICE_WIDTH;
			dst_h = DEVICE_HEIGHT;
			dst_p = DEVICE_PITCH;

			int ox = (DEVICE_WIDTH - src_w) / 2;
			int oy = (DEVICE_HEIGHT - src_h) / 2;

			if (ox < 0)
				src_x = -ox;
			else
				dst_x = ox;

			if (oy < 0)
				src_y = -oy;
			else
				dst_y = oy;
		} else if (scaling == SCALE_CROPPED) {
			int scale_x = CEIL_DIV(DEVICE_WIDTH, src_w);
			int scale_y = CEIL_DIV(DEVICE_HEIGHT, src_h);
			scale = MIN(scale_x, scale_y);

			sprintf(scaler_name, "cropped");
			dst_w = DEVICE_WIDTH;
			dst_h = DEVICE_HEIGHT;
			dst_p = DEVICE_PITCH;

			int scaled_w = src_w * scale;
			int scaled_h = src_h * scale;
			int ox = (DEVICE_WIDTH - scaled_w) / 2;
			int oy = (DEVICE_HEIGHT - scaled_h) / 2;

			if (ox < 0) {
				src_x = -ox / scale;
				src_w -= src_x * 2;
			} else {
				dst_x = ox;
			}

			if (oy < 0) {
				src_y = -oy / scale;
				src_h -= src_y * 2;
			} else {
				dst_y = oy;
			}
		} else {
			sprintf(scaler_name, "integer");
			int scaled_w = src_w * scale;
			int scaled_h = src_h * scale;
			dst_w = DEVICE_WIDTH;
			dst_h = DEVICE_HEIGHT;
			dst_p = DEVICE_PITCH;
			dst_x = (DEVICE_WIDTH - scaled_w) / 2;
			dst_y = (DEVICE_HEIGHT - scaled_h) / 2;
		}
	} else if (fit) {
		if (scaling == SCALE_FULLSCREEN) {
			sprintf(scaler_name, "full fit");
			dst_w = DEVICE_WIDTH;
			dst_h = DEVICE_HEIGHT;
			dst_p = DEVICE_PITCH;
			scale = -1;
		} else {
			double scale_f = MIN(((double)DEVICE_WIDTH) / aspect_w,
				((double)DEVICE_HEIGHT) / aspect_h);
			LOG_info("scale_f:%f\n", scale_f);

			sprintf(scaler_name, "aspect fit");
			dst_w = aspect_w * scale_f;
			dst_h = aspect_h * scale_f;
			dst_p = DEVICE_PITCH;
			dst_x = (DEVICE_WIDTH - dst_w) / 2;
			dst_y = (DEVICE_HEIGHT - dst_h) / 2;
			scale = (scale_f == 1.0 && dst_w == src_w &&
				dst_h == src_h) ? 1 : -1;
		}
	} else {
		int scale_x = CEIL_DIV(DEVICE_WIDTH, src_w);
		int scale_y = CEIL_DIV(DEVICE_HEIGHT, src_h);
		int r = (DEVICE_HEIGHT - src_h) % 8;

		if (r && r < 8)
			scale_y -= 1;

		scale = MAX(scale_x, scale_y);

		int scaled_w = src_w * scale;
		int scaled_h = src_h * scale;

		if (scaling == SCALE_FULLSCREEN) {
			sprintf(scaler_name, "full%i", scale);
			dst_w = scaled_w;
			dst_h = scaled_h;
			dst_p = dst_w * FIXED_BPP;
		} else {
			double fixed_aspect_ratio =
				((double)DEVICE_WIDTH) / DEVICE_HEIGHT;
			int core_aspect = core.aspect_ratio * 1000;
			int fixed_aspect = fixed_aspect_ratio * 1000;

			if (core_aspect > fixed_aspect) {
				sprintf(scaler_name, "aspect%iL", scale);
				int aspect_h2 = DEVICE_WIDTH / core.aspect_ratio;
				double aspect_hr =
					((double)aspect_h2) / DEVICE_HEIGHT;
				dst_w = scaled_w;
				dst_h = scaled_h / aspect_hr;
				dst_y = (dst_h - scaled_h) / 2;
			} else if (core_aspect < fixed_aspect) {
				sprintf(scaler_name, "aspect%iP", scale);
				aspect_w = DEVICE_HEIGHT * core.aspect_ratio;
				double aspect_wr =
					((double)aspect_w) / DEVICE_WIDTH;
				dst_w = scaled_w / aspect_wr;
				dst_h = scaled_h;
				dst_w = (dst_w / 8) * 8;
				dst_x = (dst_w - scaled_w) / 2;
			} else {
				sprintf(scaler_name, "aspect%iM", scale);
				dst_w = scaled_w;
				dst_h = scaled_h;
			}
			dst_p = dst_w * FIXED_BPP;
		}
	}

	renderer.src_x = src_x;
	renderer.src_y = src_y;
	renderer.src_w = src_w;
	renderer.src_h = src_h;
	renderer.src_p = src_p;
	renderer.dst_x = dst_x;
	renderer.dst_y = dst_y;
	renderer.dst_w = dst_w;
	renderer.dst_h = dst_h;
	renderer.dst_p = dst_p;
	renderer.scale = scale;
	renderer.aspect = (scaling == SCALE_NATIVE ||
		scaling == SCALE_CROPPED) ? 0 :
		(scaling == SCALE_FULLSCREEN ? -1 : core.aspect_ratio);
	LOG_info("aspect: %f\n", renderer.aspect);
	renderer.blit = GFX_getScaler(&renderer);

	if (fit) {
		dst_w = DEVICE_WIDTH;
		dst_h = DEVICE_HEIGHT;
	}

	screen = GFX_resize(dst_w, dst_h, dst_p);
	(void)scaler_name;
}

void video_refresh_callback_main(const void *data, unsigned width,
	unsigned height, size_t pitch)
{
	Special_render();

	static uint32_t last_flip_time = 0;

	if (fast_forward && SDL_GetTicks() - last_flip_time < 10)
		return;
	if (!data)
		return;

	fps_ticks += 1;
	if (downsample)
		pitch /= 2;

	if (renderer.dst_p == 0 || width != (unsigned)renderer.true_w ||
		height != (unsigned)renderer.true_h) {
		selectScaler(width, height, pitch);
		GFX_clearAll();
	}

	if (show_debug) {
		int x = 2 + renderer.src_x;
		int y = 2 + renderer.src_y;
		char debug_text[128];
		int scale = renderer.scale;

		if (scale == -1)
			scale = 1;

		sprintf(debug_text, "%ix%i %ix", renderer.src_w, renderer.src_h,
			scale);
		blitBitmapText(debug_text, x, y, (uint16_t *)data, pitch / 2,
			width, height);

		sprintf(debug_text, "%i,%i %ix%i", renderer.dst_x, renderer.dst_y,
			renderer.src_w * scale, renderer.src_h * scale);
		blitBitmapText(debug_text, -x, y, (uint16_t *)data, pitch / 2,
			width, height);

		sprintf(debug_text, "%.01f/%.01f %i%%", fps_double, cpu_double,
			(int)use_double);
		blitBitmapText(debug_text, x, -y, (uint16_t *)data, pitch / 2,
			width, height);

		sprintf(debug_text, "%ix%i", renderer.dst_w, renderer.dst_h);
		blitBitmapText(debug_text, -x, -y, (uint16_t *)data, pitch / 2,
			width, height);
	}

	if (downsample) {
		buffer_downsample(data, width, height, pitch * 2);
		renderer.src = buffer;
	} else {
		renderer.src = (void *)data;
	}
	renderer.dst = screen->pixels;

	GFX_blitRenderer(&renderer);
	if (!thread_video)
		GFX_flip(screen);
	last_flip_time = SDL_GetTicks();
}

void video_refresh_callback(const void *data, unsigned width, unsigned height,
	size_t pitch)
{
	if (!data)
		return;

	if (thread_video) {
		pthread_mutex_lock(&core_mx);

		if (backbuffer && (backbuffer->w != (int)width ||
			backbuffer->h != (int)height ||
			backbuffer->pitch != (int)pitch)) {
			free(backbuffer->pixels);
			SDL_FreeSurface(backbuffer);
			backbuffer = NULL;
		}
		if (!backbuffer) {
			uint16_t *pixels = malloc(height * pitch);
			backbuffer = SDL_CreateRGBSurfaceFrom(pixels, width, height,
				FIXED_DEPTH, pitch, RGBA_MASK_565);
		}

		memcpy(backbuffer->pixels, data, backbuffer->h * backbuffer->pitch);
		pthread_cond_signal(&core_rq);
		pthread_mutex_unlock(&core_mx);
	} else {
		video_refresh_callback_main(data, width, height, pitch);
	}
}

static unsigned getUsage(void)
{
	long unsigned ticks = 0;
	long ticksps = 0;
	FILE *file = fopen("/proc/self/stat", "r");

	if (!file)
		goto finish;
	if (!fscanf(file, "%*d %*s %*c %*d %*d %*d %*d %*d %*u %*u "
		"%*u %*u %*u %lu", &ticks)) {
		goto finish;
	}

	ticksps = sysconf(_SC_CLK_TCK);
	if (ticksps)
		ticks = ticks * 100 / ticksps;

finish:
	if (file)
		fclose(file);
	return ticks;
}

void trackFPS(void)
{
	cpu_ticks += 1;
	static int last_use_ticks = 0;
	uint32_t now = SDL_GetTicks();

	if (now - sec_start >= 1000) {
		double last_time = (double)(now - sec_start) / 1000;
		fps_double = fps_ticks / last_time;
		cpu_double = cpu_ticks / last_time;
		use_ticks = getUsage();
		if (use_ticks && last_use_ticks)
			use_double = (use_ticks - last_use_ticks) / last_time;
		last_use_ticks = use_ticks;
		sec_start = now;
		cpu_ticks = 0;
		fps_ticks = 0;
	}
}

void limitFF(void)
{
	static uint64_t ff_frame_time = 0;
	static uint64_t last_time = 0;
	static int last_max_speed = -1;

	if (last_max_speed != max_ff_speed) {
		last_max_speed = max_ff_speed;
		ff_frame_time = 1000000 / (core.fps * (max_ff_speed + 1));
	}

	uint64_t now = getMicroseconds();
	if (fast_forward && max_ff_speed) {
		if (last_time == 0)
			last_time = now;
		int elapsed = now - last_time;
		if (elapsed > 0 && elapsed < 0x80000) {
			if (elapsed < (int)ff_frame_time) {
				int delay = (ff_frame_time - elapsed) / 1000;
				if (delay > 0 && delay < 17)
					SDL_Delay(delay);
			}
			last_time += ff_frame_time;
			return;
		}
	}
	last_time = now;
}
