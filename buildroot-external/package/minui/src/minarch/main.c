#include "minarch.h"

SDL_Surface *screen;
int quit = 0;
int show_menu = 0;
int simple_mode = 0;
int thread_video = 0;
int was_threaded = 0;
int should_run_core = 1;
pthread_t core_pt;
pthread_mutex_t core_mx;
pthread_cond_t core_rq;
SDL_Surface *backbuffer = NULL;

int ensure_dir_recursive(const char *path, mode_t mode)
{
	char tmp[MAX_PATH];
	char *p;
	size_t len;

	if (!path || !path[0])
		return -1;
	if (snprintf(tmp, sizeof(tmp), "%s", path) >= (int)sizeof(tmp))
		return -1;

	len = strlen(tmp);
	if (len > 1 && tmp[len - 1] == '/')
		tmp[len - 1] = '\0';

	for (p = tmp + 1; *p; p++) {
		if (*p != '/')
			continue;
		*p = '\0';
		if (mkdir(tmp, mode) != 0 && errno != EEXIST) {
			*p = '/';
			return -1;
		}
		*p = '/';
	}
	if (mkdir(tmp, mode) != 0 && errno != EEXIST)
		return -1;
	return 0;
}

static void *coreThread(void *arg)
{
	(void)arg;
	GFX_clearAll();
	GFX_flip(screen);

	while (!quit) {
		int run = 0;
		pthread_mutex_lock(&core_mx);
		run = should_run_core;
		pthread_mutex_unlock(&core_mx);

		if (run) {
			if (!Rewind_processFrame()) {
				core.run();
				Rewind_afterFrame();
				limitFF();
				trackFPS();
			}
		}
	}
	pthread_exit(NULL);
}

int main(int argc, char *argv[])
{
	LOG_info("MinArch\n");

	setOverclock(overclock);

	char core_path[MAX_PATH];
	char rom_path[MAX_PATH];
	char tag_name[MAX_PATH];

	strcpy(core_path, argv[1]);
	strcpy(rom_path, argv[2]);
	getEmuName(rom_path, tag_name);

	LOG_info("rom_path: %s\n", rom_path);

	screen = GFX_init(MODE_MENU);
	PAD_init();
	DEVICE_WIDTH = screen->w;
	DEVICE_HEIGHT = screen->h;
	DEVICE_PITCH = screen->pitch;

	VIB_init();
	PWR_init();
	if (!HAS_POWER_BUTTON)
		PWR_disableSleep();
	MSG_init();

	Core_open(core_path, tag_name);
	Game_open(rom_path);
	if (!game.is_open)
		goto finish;

	simple_mode = exists(SIMPLE_MODE_PATH);

	Config_load();
	Config_init();
	Config_readOptions();
	setOverclock(overclock);
	GFX_setVsync(prevent_tearing);

	Core_init();
	Menu_setCoreVersion(core.version);

	if (Core_load() != 0)
		goto finish;
	Input_init(NULL);
	Config_readOptions();
	Config_readControls();
	Config_free();

	SND_init(core.sample_rate, core.fps);
	InitSettings();
	Menu_init();
	State_resume();
	Rewind_init();
	Rewind_onStateChange();
	Menu_initState();

	if (thread_video) {
		core_mx = (pthread_mutex_t)PTHREAD_MUTEX_INITIALIZER;
		core_rq = (pthread_cond_t)PTHREAD_COND_INITIALIZER;
		pthread_create(&core_pt, NULL, &coreThread, NULL);
	}

	PWR_warn(1);
	PWR_disableAutosleep();

	GFX_clearAll();
	GFX_flip(screen);

	Special_init();

	sec_start = SDL_GetTicks();
	while (!quit) {
		GFX_startFrame();

		if (!thread_video) {
			if (!Rewind_processFrame()) {
				core.run();
				Rewind_afterFrame();
				limitFF();
				trackFPS();
			}
		}

		if (thread_video && !quit) {
			pthread_mutex_lock(&core_mx);
			pthread_cond_wait(&core_rq, &core_mx);

			if (backbuffer) {
				video_refresh_callback_main(backbuffer->pixels,
					backbuffer->w, backbuffer->h, backbuffer->pitch);
				GFX_flip(screen);
			}
			core_rq = (pthread_cond_t)PTHREAD_COND_INITIALIZER;
			pthread_mutex_unlock(&core_mx);
		}

		if (show_menu)
			Menu_loop();

		if (toggle_thread) {
			toggle_thread = 0;
			if (was_threaded && !thread_video) {
				was_threaded = 0;
				thread_video = !thread_video;
			}
			thread_video = !thread_video;
			if (thread_video) {
				core_mx = (pthread_mutex_t)PTHREAD_MUTEX_INITIALIZER;
				core_rq = (pthread_cond_t)PTHREAD_COND_INITIALIZER;
				pthread_create(&core_pt, NULL, &coreThread, NULL);
			} else {
				pthread_cancel(core_pt);
				pthread_join(core_pt, NULL);
				GFX_clearAll();
				GFX_flip(screen);
			}
		}

		hdmimon();
	}

	Menu_quit();
	QuitSettings();

finish:
	Rewind_quit();
	Game_close();
	Core_unload();
	Core_quit();
	Core_close();
	Config_quit();
	Special_quit();
	MSG_quit();
	PWR_quit();
	VIB_quit();
	SND_quit();
	PAD_quit();
	GFX_quit();
	buffer_dealloc();

	return EXIT_SUCCESS;
}
