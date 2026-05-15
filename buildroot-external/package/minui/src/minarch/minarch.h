#ifndef MINARCH_H
#define MINARCH_H

#include <ctype.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <libgen.h>
#include <msettings.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#include <zlib.h>

#include <libretro.h>

#include "defines.h"
#include "api.h"
#include "core_registry.h"
#include "scaler.h"
#include "utils.h"

enum {
	SCALE_NATIVE,
	SCALE_ASPECT,
	SCALE_FULLSCREEN,
	SCALE_CROPPED,
	SCALE_COUNT,
};

enum {
	FE_OPT_SCALING,
	FE_OPT_EFFECT,
	FE_OPT_SHARPNESS,
	FE_OPT_TEARING,
	FE_OPT_OVERCLOCK,
	FE_OPT_THREAD,
	FE_OPT_DEBUG,
	FE_OPT_MAXFF,
	FE_OPT_REWIND_ENABLE,
	FE_OPT_REWIND_BUFFER,
	FE_OPT_REWIND_CAPTURE,
	FE_OPT_REWIND_KEYFRAME,
	FE_OPT_REWIND_COMPRESSION,
	FE_OPT_REWIND_AUDIO,
	FE_OPT_COUNT,
};

enum {
	SHORTCUT_SAVE_STATE,
	SHORTCUT_LOAD_STATE,
	SHORTCUT_RESET_GAME,
	SHORTCUT_SAVE_QUIT,
	SHORTCUT_CYCLE_SCALE,
	SHORTCUT_CYCLE_EFFECT,
	SHORTCUT_TOGGLE_FF,
	SHORTCUT_HOLD_FF,
	SHORTCUT_TOGGLE_REWIND,
	SHORTCUT_HOLD_REWIND,
	SHORTCUT_COUNT,
};

enum {
	CONFIG_NONE,
	CONFIG_CONSOLE,
	CONFIG_GAME,
};

enum {
	CONFIG_WRITE_ALL,
	CONFIG_WRITE_GAME,
};

#define LOCAL_BUTTON_COUNT 16
#define RETRO_BUTTON_COUNT 16

typedef struct Option {
	char *key;
	char *name;
	char *desc;
	char *full;
	char *var;
	int default_value;
	int value;
	int count;
	int lock;
	char **values;
	char **labels;
} Option;

typedef struct OptionList {
	int count;
	int changed;
	Option *options;
	int enabled_count;
	Option **enabled_options;
} OptionList;

typedef struct ButtonMapping {
	char *name;
	int retro;
	int local;
	int mod;
	int default_;
	int ignore;
} ButtonMapping;

typedef struct Core {
	int initialized;
	int game_loaded;
	int need_fullpath;
	char tag[8];
	char name[128];
	char version[128];
	char extensions[128];
	char config_dir[MAX_PATH];
	char states_dir[MAX_PATH];
	char saves_dir[MAX_PATH];
	char bios_dir[MAX_PATH];
	double fps;
	double sample_rate;
	double aspect_ratio;
	void *handle;
	void (*init)(void);
	void (*deinit)(void);
	void (*get_system_info)(struct retro_system_info *info);
	void (*get_system_av_info)(struct retro_system_av_info *info);
	void (*set_controller_port_device)(unsigned port, unsigned device);
	void (*reset)(void);
	void (*run)(void);
	size_t (*serialize_size)(void);
	bool (*serialize)(void *data, size_t size);
	bool (*unserialize)(const void *data, size_t size);
	bool (*load_game)(const struct retro_game_info *game);
	bool (*load_game_special)(unsigned game_type,
		const struct retro_game_info *info, size_t num_info);
	void (*unload_game)(void);
	unsigned (*get_region)(void);
	void *(*get_memory_data)(unsigned id);
	size_t (*get_memory_size)(unsigned id);
	uint64_t serialization_quirks;
} Core;

typedef struct Game {
	char path[MAX_PATH];
	char name[MAX_PATH];
	char m3u_path[MAX_PATH];
	char tmp_path[MAX_PATH];
	void *data;
	size_t size;
	int is_open;
} Game;

typedef struct Config {
	char *system_cfg;
	char *default_cfg;
	char *user_cfg;
	char *device_tag;
	OptionList frontend;
	OptionList core;
	ButtonMapping *controls;
	ButtonMapping *shortcuts;
	int loaded;
	int initialized;
} Config;

extern SDL_Surface *screen;
extern int quit;
extern int show_menu;
extern int simple_mode;
extern int thread_video;
extern int was_threaded;
extern int should_run_core;
extern pthread_t core_pt;
extern pthread_mutex_t core_mx;
extern pthread_cond_t core_rq;
extern SDL_Surface *backbuffer;

extern int screen_scaling;
extern int screen_sharpness;
extern int screen_effect;
extern int prevent_tearing;
extern int show_debug;
extern int max_ff_speed;
extern int fast_forward;
extern int rewind_pressed;
extern int rewind_toggle;
extern int rewinding;
extern int overclock;
extern int has_custom_controllers;
extern int gamepad_type;
extern int downsample;
extern int DEVICE_WIDTH;
extern int DEVICE_HEIGHT;
extern int DEVICE_PITCH;
extern int toggle_thread;
extern uint32_t sec_start;

extern GFX_Renderer renderer;
extern Core core;
extern Game game;
extern Config config;
extern int state_slot;
extern struct retro_disk_control_ext_callback disk_control_ext;

extern ButtonMapping default_button_mapping[];
extern ButtonMapping button_label_mapping[];
extern ButtonMapping core_button_mapping[];
extern const char *device_button_names[LOCAL_BUTTON_COUNT];
extern char *button_labels[];
extern char *gamepad_labels[];
extern char *gamepad_values[];

int ensure_dir_recursive(const char *path, mode_t mode);

void Game_open(char *path);
void Game_close(void);
void Game_changeDisc(char *path);
const char *Game_storageRoot(void);

void SRAM_read(void);
void SRAM_write(void);
void RTC_read(void);
void RTC_write(void);
void State_getPath(char *filename);
void State_read(void);
void State_write(void);
void State_autosave(void);
void State_resume(void);

void Core_getName(char *in_name, char *out_name);
void Core_open(const char *core_path, const char *tag_name);
void Core_init(void);
int Core_load(void);
void Core_reset(void);
void Core_unload(void);
void Core_quit(void);
void Core_close(void);
bool environment_callback(unsigned cmd, void *data);
void audio_sample_callback(int16_t left, int16_t right);
size_t audio_sample_batch_callback(const int16_t *data, size_t frames);

void Config_syncFrontend(char *key, int value);
void Config_init(void);
void Config_quit(void);
void Config_readOptionsString(char *cfg);
void Config_readControlsString(char *cfg);
void Config_load(void);
void Config_free(void);
void Config_readOptions(void);
void Config_readControls(void);
void Config_write(int override);
void Config_restore(void);
void setOverclock(int i);

void OptionList_init(const struct retro_core_option_definition *defs);
void OptionList_vars(const struct retro_variable *vars);
void OptionList_reset(void);
Option *OptionList_getOption(OptionList *list, const char *key);
char *OptionList_getOptionValue(OptionList *list, const char *key);
void OptionList_setOptionRawValue(OptionList *list, const char *key, int value);
void OptionList_setOptionValue(OptionList *list, const char *key,
	const char *value);
void Special_init(void);
void Special_render(void);
void Special_quit(void);

int setFastForward(int enable);
int setRewindToggle(int enable);
int setRewindPressed(int enable);
void Rewind_init(void);
void Rewind_quit(void);
void Rewind_applyConfig(void);
void Rewind_afterFrame(void);
int Rewind_processFrame(void);
void Rewind_onStateChange(void);
enum retro_savestate_context Rewind_getSavestateContext(void);
int Rewind_audioEnabled(void);
void input_poll_callback(void);
int16_t input_state_callback(unsigned port, unsigned device, unsigned index,
	unsigned id);
void Input_init(const struct retro_input_descriptor *vars);

void hdmimon(void);
void MSG_init(void);
void MSG_quit(void);
void selectScaler(int src_w, int src_h, int src_p);
void video_refresh_callback_main(const void *data, unsigned width,
	unsigned height, size_t pitch);
void video_refresh_callback(const void *data, unsigned width, unsigned height,
	size_t pitch);
void trackFPS(void);
void limitFF(void);
void buffer_dealloc(void);

void Menu_init(void);
void Menu_quit(void);
void Menu_beforeSleep(void);
void Menu_afterSleep(void);
void Menu_setCoreVersion(const char *version);
void Menu_initState(void);
void Menu_saveState(void);
void Menu_loadState(void);
void Menu_loop(void);

#endif
