#include "minarch.h"

int screen_scaling = SCALE_ASPECT;
int screen_sharpness = SHARPNESS_SOFT;
int screen_effect = EFFECT_NONE;
int prevent_tearing = 1;
int show_debug = 0;
int max_ff_speed = 3;
int fast_forward = 0;
int overclock = 1;
int has_custom_controllers = 0;
int gamepad_type = 0;
int downsample = 0;
int toggle_thread = 0;

static char *onoff_labels[] = {
	"Off",
	"On",
	NULL
};

static char *scaling_labels[] = {
	"Native",
	"Aspect",
	"Fullscreen",
	"Cropped",
	NULL
};

static char *effect_labels[] = {
	"None",
	"Line",
	"Grid",
	NULL
};

static char *sharpness_labels[] = {
	"Sharp",
	"Crisp",
	"Soft",
	NULL
};

static char *tearing_labels[] = {
	"Off",
	"Lenient",
	"Strict",
	NULL
};

static char *max_ff_labels[] = {
	"None",
	"2x",
	"3x",
	"4x",
	"5x",
	"6x",
	"7x",
	"8x",
	NULL,
};

static char *overclock_labels[] = {
	"Powersave",
	"Normal",
	"Performance",
	NULL,
};

static char *rewind_buffer_labels[] = {
	"16",
	"32",
	"64",
	"96",
	"128",
	"192",
	"256",
	NULL,
};

static char *rewind_capture_values[] = {
	"16",
	"22",
	"25",
	"33",
	"50",
	"66",
	"100",
	"150",
	"200",
	NULL,
};

static char *rewind_capture_labels[] = {
	"16 ms (~60 fps)",
	"22 ms (~45 fps)",
	"25 ms (~40 fps)",
	"33 ms (~30 fps)",
	"50 ms (~20 fps)",
	"66 ms (~15 fps)",
	"100 ms (~10 fps)",
	"150 ms (~7 fps)",
	"200 ms (~5 fps)",
	NULL,
};

static char *rewind_keyframe_values[] = {
	"125",
	"250",
	"500",
	"1000",
	"2000",
	NULL,
};

static char *rewind_keyframe_labels[] = {
	"125 ms",
	"250 ms",
	"500 ms",
	"1000 ms",
	"2000 ms",
	NULL,
};

static char *rewind_compression_values[] = {
	"1",
	"2",
	"4",
	"8",
	"12",
	NULL,
};

static char *rewind_compression_labels[] = {
	"1 (best ratio)",
	"2 (default)",
	"4 (fast)",
	"8 (faster)",
	"12 (fastest)",
	NULL,
};

Config config = {
	.frontend = {
		.count = FE_OPT_COUNT,
		.options = (Option[]){
			[FE_OPT_SCALING] = {
				.key = "minarch_screen_scaling",
				.name = "Screen Scaling",
				.default_value = 1,
				.value = 1,
				.count = 3,
				.values = scaling_labels,
				.labels = scaling_labels,
			},
			[FE_OPT_EFFECT] = {
				.key = "minarch_screen_effect",
				.name = "Screen Effect",
				.desc = "Grid simulates an LCD grid.\nLine simulates CRT "
					"scanlines.\nEffects usually look best at native "
					"scaling.",
				.default_value = 0,
				.value = 0,
				.count = 3,
				.values = effect_labels,
				.labels = effect_labels,
			},
			[FE_OPT_SHARPNESS] = {
				.key = "minarch_screen_sharpness",
				.name = "Screen Sharpness",
				.desc = "Sharp uses nearest neighbor sampling.\nCrisp "
					"integer upscales before linear sampling.\nSoft uses "
					"linear sampling.",
				.default_value = 2,
				.value = 2,
				.count = 3,
				.values = sharpness_labels,
				.labels = sharpness_labels,
			},
			[FE_OPT_TEARING] = {
				.key = "minarch_prevent_tearing",
				.name = "Prevent Tearing",
				.desc = "Wait for vsync before drawing the next frame.\n"
					"Lenient only waits when within frame budget.\n"
					"Strict always waits.",
				.default_value = VSYNC_LENIENT,
				.value = VSYNC_LENIENT,
				.count = 3,
				.values = tearing_labels,
				.labels = tearing_labels,
			},
			[FE_OPT_OVERCLOCK] = {
				.key = "minarch_cpu_speed",
				.name = "CPU Speed",
				.desc = "Over- or underclock the CPU to prioritize\n"
					"pure performance or power savings.",
				.default_value = 1,
				.value = 1,
				.count = 3,
				.values = overclock_labels,
				.labels = overclock_labels,
			},
			[FE_OPT_THREAD] = {
				.key = "minarch_thread_video",
				.name = "Prioritize Audio",
				.desc = "Can eliminate crackle but\nmay cause dropped "
					"frames.\nOnly turn on if necessary.",
				.default_value = 0,
				.value = 0,
				.count = 2,
				.values = onoff_labels,
				.labels = onoff_labels,
			},
			[FE_OPT_DEBUG] = {
				.key = "minarch_debug_hud",
				.name = "Debug HUD",
				.desc = "Show frames per second, cpu load,\nresolution, "
					"and scaler information.",
				.default_value = 0,
				.value = 0,
				.count = 2,
				.values = onoff_labels,
				.labels = onoff_labels,
			},
			[FE_OPT_MAXFF] = {
				.key = "minarch_max_ff_speed",
				.name = "Max FF Speed",
				.desc = "Fast forward will not exceed the\nselected speed "
					"(but may be less\ndepending on game and emulator).",
				.default_value = 3,
				.value = 3,
				.count = 8,
				.values = max_ff_labels,
				.labels = max_ff_labels,
			},
			[FE_OPT_REWIND_ENABLE] = {
				.key = "minarch_rewind_enable",
				.name = "Rewind Enable",
				.desc = "Keep a compressed in-memory rewind history.\n"
					"Requires a rewind shortcut binding.",
				.default_value = 0,
				.value = 0,
				.count = 2,
				.values = onoff_labels,
				.labels = onoff_labels,
			},
			[FE_OPT_REWIND_BUFFER] = {
				.key = "minarch_rewind_buffer_mb",
				.name = "Rewind Buffer",
				.desc = "Memory reserved for rewind history.\nLarger "
					"buffers extend rewind time.",
				.default_value = 1,
				.value = 1,
				.count = 7,
				.values = rewind_buffer_labels,
				.labels = rewind_buffer_labels,
			},
			[FE_OPT_REWIND_CAPTURE] = {
				.key = "minarch_rewind_capture_ms",
				.name = "Rewind Capture",
				.desc = "Interval between rewind captures.\nLower values "
					"improve smoothness but cost more CPU.",
				.default_value = 0,
				.value = 0,
				.count = 9,
				.values = rewind_capture_values,
				.labels = rewind_capture_labels,
			},
			[FE_OPT_REWIND_KEYFRAME] = {
				.key = "minarch_rewind_keyframe_ms",
				.name = "Rewind Keyframe",
				.desc = "How often rewind stores a full keyframe.\nLower "
					"values improve seek cost but use more memory.",
				.default_value = 1,
				.value = 1,
				.count = 5,
				.values = rewind_keyframe_values,
				.labels = rewind_keyframe_labels,
			},
			[FE_OPT_REWIND_COMPRESSION] = {
				.key = "minarch_rewind_compression_speed",
				.name = "Rewind Compression",
				.desc = "LZ4 acceleration for rewind history.\nLower "
					"values compress more but use more CPU.",
				.default_value = 1,
				.value = 1,
				.count = 5,
				.values = rewind_compression_values,
				.labels = rewind_compression_labels,
			},
			[FE_OPT_REWIND_AUDIO] = {
				.key = "minarch_rewind_audio",
				.name = "Rewind Audio",
				.desc = "Play audio while rewinding instead of muting it.",
				.default_value = 0,
				.value = 0,
				.count = 2,
				.values = onoff_labels,
				.labels = onoff_labels,
			},
			[FE_OPT_COUNT] = {NULL}
		}
	},
	.core = {
		.count = 0,
		.options = (Option[]){
			{NULL},
		},
	},
	.controls = default_button_mapping,
	.shortcuts = (ButtonMapping[]){
		[SHORTCUT_SAVE_STATE] = {"Save State", -1, BTN_ID_NONE, 0},
		[SHORTCUT_LOAD_STATE] = {"Load State", -1, BTN_ID_NONE, 0},
		[SHORTCUT_RESET_GAME] = {"Reset Game", -1, BTN_ID_NONE, 0},
		[SHORTCUT_SAVE_QUIT] = {"Save & Quit", -1, BTN_ID_NONE, 0},
		[SHORTCUT_CYCLE_SCALE] = {"Cycle Scaling", -1, BTN_ID_NONE, 0},
		[SHORTCUT_CYCLE_EFFECT] = {"Cycle Effect", -1, BTN_ID_NONE, 0},
		[SHORTCUT_TOGGLE_FF] = {"Toggle FF", -1, BTN_ID_NONE, 0},
		[SHORTCUT_HOLD_FF] = {"Hold FF", -1, BTN_ID_NONE, 0},
		[SHORTCUT_TOGGLE_REWIND] = {"Toggle Rewind", -1, BTN_ID_NONE,
			0},
		[SHORTCUT_HOLD_REWIND] = {"Hold Rewind", -1, BTN_ID_NONE, 0},
		{NULL}
	},
};

static inline char *getScreenScalingDesc(void)
{
	if (GFX_supportsOverscan()) {
		return "Native uses integer scaling. Aspect uses core\nreported "
			"aspect ratio. Fullscreen has non-square\npixels. Cropped "
			"is integer scaled then cropped.";
	}
	return "Native uses integer scaling.\nAspect uses core reported "
		"aspect ratio.\nFullscreen has non-square pixels.";
}

static inline int getScreenScalingCount(void)
{
	return GFX_supportsOverscan() ? 4 : 3;
}

static void Config_setRewindDefaults(void)
{
	int buffer_value = 1;
	int capture_value = 0;
	int keyframe_value = 1;
	int compression_value = 1;

	if (exactMatch(core.tag, "PS")) {
		buffer_value = 4;
		capture_value = 3;
		keyframe_value = 2;
		compression_value = 4;
	} else if (exactMatch(core.tag, "GBA")) {
		buffer_value = 2;
		capture_value = 0;
		keyframe_value = 1;
		compression_value = 1;
	}

	config.frontend.options[FE_OPT_REWIND_ENABLE].default_value = 0;
	config.frontend.options[FE_OPT_REWIND_ENABLE].value = 0;
	config.frontend.options[FE_OPT_REWIND_BUFFER].default_value =
		buffer_value;
	config.frontend.options[FE_OPT_REWIND_BUFFER].value = buffer_value;
	config.frontend.options[FE_OPT_REWIND_CAPTURE].default_value =
		capture_value;
	config.frontend.options[FE_OPT_REWIND_CAPTURE].value = capture_value;
	config.frontend.options[FE_OPT_REWIND_KEYFRAME].default_value =
		keyframe_value;
	config.frontend.options[FE_OPT_REWIND_KEYFRAME].value = keyframe_value;
	config.frontend.options[FE_OPT_REWIND_COMPRESSION].default_value =
		compression_value;
	config.frontend.options[FE_OPT_REWIND_COMPRESSION].value =
		compression_value;
	config.frontend.options[FE_OPT_REWIND_AUDIO].default_value = 0;
	config.frontend.options[FE_OPT_REWIND_AUDIO].value = 0;
}

static int Config_getValue(char *cfg, const char *key, char *out_value,
	int *lock)
{
	char *tmp = cfg;

	while ((tmp = strstr(tmp, key))) {
		if (lock != NULL && tmp > cfg && *(tmp - 1) == '-')
			*lock = 1;
		tmp += strlen(key);
		if (!strncmp(tmp, " = ", 3))
			break;
	}
	if (!tmp)
		return 0;
	tmp += 3;

	strncpy(out_value, tmp, 256);
	out_value[256 - 1] = '\0';
	tmp = strchr(out_value, '\n');
	if (!tmp)
		tmp = strchr(out_value, '\r');
	if (tmp)
		*tmp = '\0';

	return 1;
}

void setOverclock(int i)
{
	overclock = i;
	switch (i) {
	case 0: PWR_setCPUSpeed(CPU_SPEED_POWERSAVE); break;
	case 1: PWR_setCPUSpeed(CPU_SPEED_NORMAL); break;
	case 2: PWR_setCPUSpeed(CPU_SPEED_PERFORMANCE); break;
	}
}

void Config_syncFrontend(char *key, int value)
{
	int i = -1;

	if (exactMatch(key, config.frontend.options[FE_OPT_SCALING].key)) {
		screen_scaling = value;
		if (screen_scaling == SCALE_NATIVE)
			GFX_setSharpness(SHARPNESS_SHARP);
		else
			GFX_setSharpness(screen_sharpness);
		renderer.dst_p = 0;
		i = FE_OPT_SCALING;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_EFFECT].key)) {
		screen_effect = value;
		GFX_setEffect(value);
		renderer.dst_p = 0;
		i = FE_OPT_EFFECT;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_SHARPNESS].key)) {
		screen_sharpness = value;
		if (screen_scaling == SCALE_NATIVE)
			GFX_setSharpness(SHARPNESS_SHARP);
		else
			GFX_setSharpness(screen_sharpness);
		renderer.dst_p = 0;
		i = FE_OPT_SHARPNESS;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_TEARING].key)) {
		prevent_tearing = value;
		i = FE_OPT_TEARING;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_THREAD].key)) {
		int old_value = thread_video || was_threaded;
		toggle_thread = old_value != value;
		i = FE_OPT_THREAD;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_OVERCLOCK].key)) {
		overclock = value;
		i = FE_OPT_OVERCLOCK;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_DEBUG].key)) {
		show_debug = value;
		i = FE_OPT_DEBUG;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_MAXFF].key)) {
		max_ff_speed = value;
		i = FE_OPT_MAXFF;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_REWIND_ENABLE].key)) {
		i = FE_OPT_REWIND_ENABLE;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_REWIND_BUFFER].key)) {
		i = FE_OPT_REWIND_BUFFER;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_REWIND_CAPTURE].key)) {
		i = FE_OPT_REWIND_CAPTURE;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_REWIND_KEYFRAME].key)) {
		i = FE_OPT_REWIND_KEYFRAME;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_REWIND_COMPRESSION].key)) {
		i = FE_OPT_REWIND_COMPRESSION;
	} else if (exactMatch(key,
			config.frontend.options[FE_OPT_REWIND_AUDIO].key)) {
		i = FE_OPT_REWIND_AUDIO;
	}
	if (i == -1)
		return;
	config.frontend.options[i].value = value;
	if (i >= FE_OPT_REWIND_ENABLE && i <= FE_OPT_REWIND_AUDIO &&
		core.initialized) {
		Rewind_applyConfig();
	}
}

static void Config_getPath(char *filename, int override)
{
	char device_tag[64] = {0};

	if (config.device_tag)
		sprintf(device_tag, "-%s", config.device_tag);
	if (override)
		sprintf(filename, "%s/%s%s.cfg", core.config_dir, game.name,
			device_tag);
	else
		sprintf(filename, "%s/minarch%s.cfg", core.config_dir, device_tag);
	LOG_info("Config_getPath %s\n", filename);
}

void Config_init(void)
{
	if (!config.default_cfg || config.initialized)
		return;

	LOG_info("Config_init\n");
	char *tmp = config.default_cfg;
	char *tmp2;
	char *key;
	char button_name[128];
	char button_id[128];
	int i = 0;

	while ((tmp = strstr(tmp, "bind "))) {
		tmp += 5;
		key = tmp;
		tmp = strstr(tmp, " = ");
		if (!tmp)
			break;

		int len = tmp - key;
		strncpy(button_name, key, len);
		button_name[len] = '\0';

		tmp += 3;
		strncpy(button_id, tmp, 128);
		tmp2 = strchr(button_id, '\n');
		if (!tmp2)
			tmp2 = strchr(button_id, '\r');
		if (tmp2)
			*tmp2 = '\0';

		int retro_id = -1;
		int local_id = -1;

		tmp2 = strrchr(button_id, ':');
		if (tmp2) {
			for (int j = 0; button_label_mapping[j].name; j++) {
				ButtonMapping *button = &button_label_mapping[j];
				if (!strcmp(tmp2 + 1, button->name)) {
					retro_id = button->retro;
					break;
				}
			}
			*tmp2 = '\0';
		}
		for (int j = 0; button_label_mapping[j].name; j++) {
			ButtonMapping *button = &button_label_mapping[j];
			if (!strcmp(button_id, button->name)) {
				local_id = button->local;
				if (retro_id == -1)
					retro_id = button->retro;
				break;
			}
		}

		tmp += strlen(button_id);

		LOG_info("\tbind %s (%s) %i:%i\n", button_name, button_id,
			local_id, retro_id);

		tmp2 = calloc(strlen(button_name) + 1, sizeof(char));
		strcpy(tmp2, button_name);
		ButtonMapping *button = &core_button_mapping[i++];
		button->name = tmp2;
		button->retro = retro_id;
		button->local = local_id;
	}

	config.initialized = 1;
}

void Config_quit(void)
{
	if (!config.initialized)
		return;
	for (int i = 0; core_button_mapping[i].name; i++) {
		free(core_button_mapping[i].name);
	}
}

void Config_readOptionsString(char *cfg)
{
	if (!cfg)
		return;

	LOG_info("Config_readOptions\n");
	char value[256];

	for (int i = 0; config.frontend.options[i].key; i++) {
		Option *option = &config.frontend.options[i];
		if (!Config_getValue(cfg, option->key, value, &option->lock))
			continue;
		OptionList_setOptionValue(&config.frontend, option->key, value);
		Config_syncFrontend(option->key, option->value);
	}

	if (has_custom_controllers &&
		Config_getValue(cfg, "minarch_gamepad_type", value, NULL)) {
		gamepad_type = strtol(value, NULL, 0);
		int device = strtol(gamepad_values[gamepad_type], NULL, 0);
		core.set_controller_port_device(0, device);
	}

	for (int i = 0; config.core.options[i].key; i++) {
		Option *option = &config.core.options[i];
		if (!Config_getValue(cfg, option->key, value, &option->lock))
			continue;
		OptionList_setOptionValue(&config.core, option->key, value);
	}
}

void Config_readControlsString(char *cfg)
{
	if (!cfg)
		return;

	LOG_info("Config_readControlsString\n");

	char key[256];
	char value[256];
	char *tmp;

	for (int i = 0; config.controls[i].name; i++) {
		ButtonMapping *mapping = &config.controls[i];
		sprintf(key, "bind %s", mapping->name);
		sprintf(value, "NONE");
		if (!Config_getValue(cfg, key, value, NULL))
			continue;
		if ((tmp = strrchr(value, ':')))
			*tmp = '\0';

		int id = -1;
		for (int j = 0; button_labels[j]; j++) {
			if (!strcmp(button_labels[j], value)) {
				id = j - 1;
				break;
			}
		}

		int mod = 0;
		if (id >= LOCAL_BUTTON_COUNT) {
			id -= LOCAL_BUTTON_COUNT;
			mod = 1;
		}

		mapping->local = id;
		mapping->mod = mod;
	}

	for (int i = 0; config.shortcuts[i].name; i++) {
		ButtonMapping *mapping = &config.shortcuts[i];
		sprintf(key, "bind %s", mapping->name);
		sprintf(value, "NONE");
		if (!Config_getValue(cfg, key, value, NULL))
			continue;

		int id = -1;
		for (int j = 0; button_labels[j]; j++) {
			if (!strcmp(button_labels[j], value)) {
				id = j - 1;
				break;
			}
		}

		int mod = 0;
		if (id >= LOCAL_BUTTON_COUNT) {
			id -= LOCAL_BUTTON_COUNT;
			mod = 1;
		}

		mapping->local = id;
		mapping->mod = mod;
	}
}

void Config_load(void)
{
	LOG_info("Config_load\n");

	config.device_tag = getenv("DEVICE");
	LOG_info("config.device_tag %s\n", config.device_tag);

	Option *scaling_option = &config.frontend.options[FE_OPT_SCALING];
	scaling_option->desc = getScreenScalingDesc();
	scaling_option->count = getScreenScalingCount();
	if (!GFX_supportsOverscan())
		scaling_labels[3] = NULL;
	Config_setRewindDefaults();

	char *system_path = SYSTEM_PATH "/system.cfg";
	char device_system_path[MAX_PATH] = {0};
	if (config.device_tag)
		sprintf(device_system_path, SYSTEM_PATH "/system-%s.cfg",
			config.device_tag);

	if (config.device_tag && exists(device_system_path)) {
		LOG_info("usng device_system_path: %s\n", device_system_path);
		config.system_cfg = allocFile(device_system_path);
	} else if (exists(system_path)) {
		config.system_cfg = allocFile(system_path);
	} else {
		config.system_cfg = NULL;
	}

	char tag_id[64];
	for (size_t i = 0; i < sizeof(tag_id); i++)
		tag_id[i] = '\0';
	snprintf(tag_id, sizeof(tag_id), "%s", core.tag);
	for (char *p = tag_id; *p; p++) {
		*p = (char)tolower((unsigned char)*p);
	}

	char default_path[MAX_PATH];
	snprintf(default_path, sizeof(default_path), "%s/default-%s.cfg",
		CORE_CONFIGS_PATH, tag_id);

	char device_default_path[MAX_PATH] = {0};
	if (config.device_tag) {
		snprintf(device_default_path, sizeof(device_default_path),
			"%s/default-%s-%s.cfg", CORE_CONFIGS_PATH, tag_id,
			config.device_tag);
	}

	if (config.device_tag && exists(device_default_path)) {
		LOG_info("usng device_default_path: %s\n", device_default_path);
		config.default_cfg = allocFile(device_default_path);
	} else if (exists(default_path)) {
		config.default_cfg = allocFile(default_path);
	} else {
		config.default_cfg = NULL;
	}

	char path[MAX_PATH];
	config.loaded = CONFIG_NONE;
	int override = 0;
	Config_getPath(path, CONFIG_WRITE_GAME);
	if (exists(path))
		override = 1;
	if (!override)
		Config_getPath(path, CONFIG_WRITE_ALL);

	config.user_cfg = allocFile(path);
	if (!config.user_cfg)
		return;

	LOG_info("using user config: %s\n", path);
	config.loaded = override ? CONFIG_GAME : CONFIG_CONSOLE;
}

void Config_free(void)
{
	if (config.system_cfg)
		free(config.system_cfg);
	if (config.default_cfg)
		free(config.default_cfg);
	if (config.user_cfg)
		free(config.user_cfg);
}

void Config_readOptions(void)
{
	Config_readOptionsString(config.system_cfg);
	Config_readOptionsString(config.default_cfg);
	Config_readOptionsString(config.user_cfg);
}

void Config_readControls(void)
{
	Config_readControlsString(config.default_cfg);
	Config_readControlsString(config.user_cfg);
}

void Config_write(int override)
{
	char path[MAX_PATH];

	Config_getPath(path, CONFIG_WRITE_GAME);
	if (!override) {
		if (config.loaded == CONFIG_GAME)
			unlink(path);
		Config_getPath(path, CONFIG_WRITE_ALL);
	}
	config.loaded = override ? CONFIG_GAME : CONFIG_CONSOLE;

	FILE *file = fopen(path, "wb");
	if (!file)
		return;

	for (int i = 0; config.frontend.options[i].key; i++) {
		Option *option = &config.frontend.options[i];
		fprintf(file, "%s = %s\n", option->key,
			option->values[option->value]);
	}
	for (int i = 0; config.core.options[i].key; i++) {
		Option *option = &config.core.options[i];
		fprintf(file, "%s = %s\n", option->key,
			option->values[option->value]);
	}

	if (has_custom_controllers)
		fprintf(file, "%s = %i\n", "minarch_gamepad_type",
			gamepad_type);

	for (int i = 0; config.controls[i].name; i++) {
		ButtonMapping *mapping = &config.controls[i];
		int j = mapping->local + 1;
		if (mapping->mod)
			j += LOCAL_BUTTON_COUNT;
		fprintf(file, "bind %s = %s\n", mapping->name, button_labels[j]);
	}
	for (int i = 0; config.shortcuts[i].name; i++) {
		ButtonMapping *mapping = &config.shortcuts[i];
		int j = mapping->local + 1;
		if (mapping->mod)
			j += LOCAL_BUTTON_COUNT;
		fprintf(file, "bind %s = %s\n", mapping->name, button_labels[j]);
	}

	fclose(file);
	sync();
}

void Config_restore(void)
{
	char path[MAX_PATH];

	if (config.loaded == CONFIG_GAME) {
		if (config.device_tag)
			sprintf(path, "%s/%s-%s.cfg", core.config_dir, game.name,
				config.device_tag);
		else
			sprintf(path, "%s/%s.cfg", core.config_dir, game.name);
		unlink(path);
		LOG_info("deleted game config: %s\n", path);
	} else if (config.loaded == CONFIG_CONSOLE) {
		if (config.device_tag)
			sprintf(path, "%s/minarch-%s.cfg", core.config_dir,
				config.device_tag);
		else
			sprintf(path, "%s/minarch.cfg", core.config_dir);
		unlink(path);
		LOG_info("deleted console config: %s\n", path);
	}
	config.loaded = CONFIG_NONE;

	for (int i = 0; config.frontend.options[i].key; i++) {
		Option *option = &config.frontend.options[i];
		option->value = option->default_value;
		Config_syncFrontend(option->key, option->value);
	}
	for (int i = 0; config.core.options[i].key; i++) {
		Option *option = &config.core.options[i];
		option->value = option->default_value;
	}
	config.core.changed = 1;

	if (has_custom_controllers) {
		gamepad_type = 0;
		core.set_controller_port_device(0, RETRO_DEVICE_JOYPAD);
	}

	for (int i = 0; config.controls[i].name; i++) {
		ButtonMapping *mapping = &config.controls[i];
		mapping->local = mapping->default_;
		mapping->mod = 0;
	}
	for (int i = 0; config.shortcuts[i].name; i++) {
		ButtonMapping *mapping = &config.shortcuts[i];
		mapping->local = BTN_ID_NONE;
		mapping->mod = 0;
	}

	Config_load();
	Config_readOptions();
	Config_readControls();
	Config_free();

	renderer.dst_p = 0;
}
