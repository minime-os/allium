#include "minarch.h"

static uint32_t buttons = 0;
static int ignore_menu = 0;

ButtonMapping default_button_mapping[] = {
	{"Up", RETRO_DEVICE_ID_JOYPAD_UP, BTN_ID_DPAD_UP},
	{"Down", RETRO_DEVICE_ID_JOYPAD_DOWN, BTN_ID_DPAD_DOWN},
	{"Left", RETRO_DEVICE_ID_JOYPAD_LEFT, BTN_ID_DPAD_LEFT},
	{"Right", RETRO_DEVICE_ID_JOYPAD_RIGHT, BTN_ID_DPAD_RIGHT},
	{"A Button", RETRO_DEVICE_ID_JOYPAD_A, BTN_ID_A},
	{"B Button", RETRO_DEVICE_ID_JOYPAD_B, BTN_ID_B},
	{"X Button", RETRO_DEVICE_ID_JOYPAD_X, BTN_ID_X},
	{"Y Button", RETRO_DEVICE_ID_JOYPAD_Y, BTN_ID_Y},
	{"Start", RETRO_DEVICE_ID_JOYPAD_START, BTN_ID_START},
	{"Select", RETRO_DEVICE_ID_JOYPAD_SELECT, BTN_ID_SELECT},
	{"L1 Button", RETRO_DEVICE_ID_JOYPAD_L, BTN_ID_L1},
	{"R1 Button", RETRO_DEVICE_ID_JOYPAD_R, BTN_ID_R1},
	{"L2 Button", RETRO_DEVICE_ID_JOYPAD_L2, BTN_ID_L2},
	{"R2 Button", RETRO_DEVICE_ID_JOYPAD_R2, BTN_ID_R2},
	{"L3 Button", RETRO_DEVICE_ID_JOYPAD_L3, BTN_ID_L3},
	{"R3 Button", RETRO_DEVICE_ID_JOYPAD_R3, BTN_ID_R3},
	{NULL, 0, 0}
};

ButtonMapping button_label_mapping[] = {
	{"NONE", -1, BTN_ID_NONE},
	{"UP", RETRO_DEVICE_ID_JOYPAD_UP, BTN_ID_DPAD_UP},
	{"DOWN", RETRO_DEVICE_ID_JOYPAD_DOWN, BTN_ID_DPAD_DOWN},
	{"LEFT", RETRO_DEVICE_ID_JOYPAD_LEFT, BTN_ID_DPAD_LEFT},
	{"RIGHT", RETRO_DEVICE_ID_JOYPAD_RIGHT, BTN_ID_DPAD_RIGHT},
	{"A", RETRO_DEVICE_ID_JOYPAD_A, BTN_ID_A},
	{"B", RETRO_DEVICE_ID_JOYPAD_B, BTN_ID_B},
	{"X", RETRO_DEVICE_ID_JOYPAD_X, BTN_ID_X},
	{"Y", RETRO_DEVICE_ID_JOYPAD_Y, BTN_ID_Y},
	{"START", RETRO_DEVICE_ID_JOYPAD_START, BTN_ID_START},
	{"SELECT", RETRO_DEVICE_ID_JOYPAD_SELECT, BTN_ID_SELECT},
	{"L1", RETRO_DEVICE_ID_JOYPAD_L, BTN_ID_L1},
	{"R1", RETRO_DEVICE_ID_JOYPAD_R, BTN_ID_R1},
	{"L2", RETRO_DEVICE_ID_JOYPAD_L2, BTN_ID_L2},
	{"R2", RETRO_DEVICE_ID_JOYPAD_R2, BTN_ID_R2},
	{"L3", RETRO_DEVICE_ID_JOYPAD_L3, BTN_ID_L3},
	{"R3", RETRO_DEVICE_ID_JOYPAD_R3, BTN_ID_R3},
	{NULL, 0, 0}
};

ButtonMapping core_button_mapping[RETRO_BUTTON_COUNT + 1] = {0};

const char *device_button_names[LOCAL_BUTTON_COUNT] = {
	[BTN_ID_DPAD_UP] = "UP",
	[BTN_ID_DPAD_DOWN] = "DOWN",
	[BTN_ID_DPAD_LEFT] = "LEFT",
	[BTN_ID_DPAD_RIGHT] = "RIGHT",
	[BTN_ID_SELECT] = "SELECT",
	[BTN_ID_START] = "START",
	[BTN_ID_Y] = "Y",
	[BTN_ID_X] = "X",
	[BTN_ID_B] = "B",
	[BTN_ID_A] = "A",
	[BTN_ID_L1] = "L1",
	[BTN_ID_R1] = "R1",
	[BTN_ID_L2] = "L2",
	[BTN_ID_R2] = "R2",
	[BTN_ID_L3] = "L3",
	[BTN_ID_R3] = "R3",
};

char *button_labels[] = {
	"NONE",
	"UP",
	"DOWN",
	"LEFT",
	"RIGHT",
	"A",
	"B",
	"X",
	"Y",
	"START",
	"SELECT",
	"L1",
	"R1",
	"L2",
	"R2",
	"L3",
	"R3",
	"MENU+UP",
	"MENU+DOWN",
	"MENU+LEFT",
	"MENU+RIGHT",
	"MENU+A",
	"MENU+B",
	"MENU+X",
	"MENU+Y",
	"MENU+START",
	"MENU+SELECT",
	"MENU+L1",
	"MENU+R1",
	"MENU+L2",
	"MENU+R2",
	"MENU+L3",
	"MENU+R3",
	NULL,
};

char *gamepad_labels[] = {
	"Standard",
	"DualShock",
	NULL,
};

char *gamepad_values[] = {
	"1",
	"517",
	NULL,
};

int setFastForward(int enable)
{
	if (enable) {
		rewind_toggle = 0;
		rewind_pressed = 0;
	}
	if (!fast_forward && enable && thread_video) {
		was_threaded = 1;
		toggle_thread = 1;
	} else if (fast_forward && !enable && !thread_video && was_threaded) {
		was_threaded = 0;
		toggle_thread = 1;
	}
	fast_forward = enable;
	return enable;
}

void input_poll_callback(void)
{
	uint32_t consumed_buttons = 0;

	PAD_poll();

	int show_setting = 0;
	PWR_update(NULL, &show_setting, Menu_beforeSleep, Menu_afterSleep);

	if (PAD_justPressed(BTN_MENU))
		ignore_menu = 0;
	if (PAD_isPressed(BTN_MENU) &&
		(PAD_isPressed(BTN_PLUS) || PAD_isPressed(BTN_MINUS))) {
		ignore_menu = 1;
	}

	if (PAD_justPressed(BTN_POWER)) {
		if (thread_video) {
			was_threaded = 1;
			toggle_thread = 1;
		}
	} else if (PAD_justReleased(BTN_POWER)) {
		if (!thread_video && was_threaded) {
			was_threaded = 0;
			toggle_thread = 1;
		}
	}

	static int toggled_ff_on = 0;
	for (int i = 0; i < SHORTCUT_COUNT; i++) {
		ButtonMapping *mapping = &config.shortcuts[i];
		int btn = mapping->local == BTN_ID_NONE ?
			BTN_NONE : 1 << mapping->local;
		int mod_active = !mapping->mod || PAD_isPressed(BTN_MENU);
		int mod_released = mapping->mod && PAD_justReleased(BTN_MENU);
		int shortcut_released = PAD_justReleased(btn) || mod_released;
		int hold_active = PAD_isPressed(btn) && mod_active;

		if (btn == BTN_NONE)
			continue;
		if (mapping->mod && PAD_isPressed(btn) &&
			(mod_active || mod_released)) {
			consumed_buttons |= (uint32_t)btn;
		}
		if (i == SHORTCUT_HOLD_REWIND) {
			if (hold_active) {
				toggled_ff_on = 0;
				setFastForward(0);
			}
			setRewindPressed(hold_active);
			if (mapping->mod && hold_active)
				ignore_menu = 1;
			continue;
		}
		if (mod_active || mod_released) {
			if (i == SHORTCUT_TOGGLE_FF) {
				if (PAD_justPressed(btn)) {
					setRewindToggle(0);
					setRewindPressed(0);
					toggled_ff_on = setFastForward(!fast_forward);
					if (mapping->mod)
						ignore_menu = 1;
					break;
				} else if (PAD_justReleased(btn)) {
					if (mapping->mod)
						ignore_menu = 1;
					break;
				}
			} else if (i == SHORTCUT_HOLD_FF) {
				if (PAD_justPressed(btn) ||
					(!toggled_ff_on && shortcut_released)) {
					if (PAD_isPressed(btn)) {
						setRewindToggle(0);
						setRewindPressed(0);
					}
					fast_forward = setFastForward(PAD_isPressed(btn) &&
						mod_active);
					if (mapping->mod)
						ignore_menu = 1;
				}
			} else if (i == SHORTCUT_TOGGLE_REWIND) {
				if (PAD_justPressed(btn)) {
					toggled_ff_on = 0;
					setFastForward(0);
					setRewindToggle(!rewind_toggle);
					if (mapping->mod)
						ignore_menu = 1;
					break;
				} else if (PAD_justReleased(btn)) {
					if (mapping->mod)
						ignore_menu = 1;
					break;
				}
			} else if (PAD_justPressed(btn)) {
				switch (i) {
				case SHORTCUT_SAVE_STATE:
					Menu_saveState();
					break;
				case SHORTCUT_LOAD_STATE:
					Menu_loadState();
					break;
				case SHORTCUT_RESET_GAME:
					Core_reset();
					break;
				case SHORTCUT_SAVE_QUIT:
					Menu_saveState();
					quit = 1;
					break;
				case SHORTCUT_CYCLE_SCALE:
					screen_scaling += 1;
					{
						int count =
							config.frontend.options[FE_OPT_SCALING].count;
						if (screen_scaling >= count)
							screen_scaling -= count;
					}
					Config_syncFrontend(
						config.frontend.options[FE_OPT_SCALING].key,
						screen_scaling);
					break;
				case SHORTCUT_CYCLE_EFFECT:
					screen_effect += 1;
					if (screen_effect >= EFFECT_COUNT)
						screen_effect -= EFFECT_COUNT;
					Config_syncFrontend(
						config.frontend.options[FE_OPT_EFFECT].key,
						screen_effect);
					break;
				default:
					break;
				}

				if (mapping->mod)
					ignore_menu = 1;
			}
		}
	}

	if (!ignore_menu && PAD_justReleased(BTN_MENU)) {
		show_menu = 1;
		if (thread_video) {
			pthread_mutex_lock(&core_mx);
			should_run_core = 0;
			pthread_mutex_unlock(&core_mx);
		}
	}

	buttons = 0;
	for (int i = 0; config.controls[i].name; i++) {
		ButtonMapping *mapping = &config.controls[i];
		int raw_btn = mapping->local == BTN_ID_NONE ?
			BTN_NONE : 1 << mapping->local;
		int btn = raw_btn;

		if (btn == BTN_NONE)
			continue;
		if (consumed_buttons & (uint32_t)raw_btn)
			continue;
		if (gamepad_type == 0) {
			switch (btn) {
			case BTN_DPAD_UP: btn = BTN_UP; break;
			case BTN_DPAD_DOWN: btn = BTN_DOWN; break;
			case BTN_DPAD_LEFT: btn = BTN_LEFT; break;
			case BTN_DPAD_RIGHT: btn = BTN_RIGHT; break;
			}
		}
		if (PAD_isPressed(btn) &&
			(!mapping->mod || PAD_isPressed(BTN_MENU))) {
			buttons |= 1 << mapping->retro;
			if (mapping->mod)
				ignore_menu = 1;
		}
	}
}

int16_t input_state_callback(unsigned port, unsigned device, unsigned index,
	unsigned id)
{
	if (port == 0 && device == RETRO_DEVICE_JOYPAD && index == 0) {
		if (id == RETRO_DEVICE_ID_JOYPAD_MASK)
			return buttons;
		return (buttons >> id) & 1;
	} else if (port == 0 && device == RETRO_DEVICE_ANALOG) {
		if (index == RETRO_DEVICE_INDEX_ANALOG_LEFT) {
			if (id == RETRO_DEVICE_ID_ANALOG_X)
				return pad.laxis.x;
			else if (id == RETRO_DEVICE_ID_ANALOG_Y)
				return pad.laxis.y;
		} else if (index == RETRO_DEVICE_INDEX_ANALOG_RIGHT) {
			if (id == RETRO_DEVICE_ID_ANALOG_X)
				return pad.raxis.x;
			else if (id == RETRO_DEVICE_ID_ANALOG_Y)
				return pad.raxis.y;
		}
	}
	return 0;
}

void Input_init(const struct retro_input_descriptor *vars)
{
	static int input_initialized = 0;

	if (input_initialized)
		return;

	LOG_info("Input_init\n");

	config.controls = core_button_mapping[0].name ?
		core_button_mapping : default_button_mapping;

	puts("---------------------------------");

	const char *core_button_names[RETRO_BUTTON_COUNT] = {0};
	int present[RETRO_BUTTON_COUNT] = {0};
	int core_mapped = 0;

	if (vars) {
		core_mapped = 1;
		for (int i = 0; vars[i].description; i++) {
			const struct retro_input_descriptor *var = &vars[i];
			if (var->port != 0 || var->device != RETRO_DEVICE_JOYPAD ||
				var->index != 0) {
				continue;
			}

			if (var->id >= RETRO_BUTTON_COUNT) {
				printf("UNAVAILABLE: %s\n", var->description);
				fflush(stdout);
				continue;
			}

			printf("PRESENT    : %s\n", var->description);
			fflush(stdout);
			present[var->id] = 1;
			core_button_names[var->id] = var->description;
		}
	}

	puts("---------------------------------");

	for (int i = 0; default_button_mapping[i].name; i++) {
		ButtonMapping *mapping = &default_button_mapping[i];
		LOG_info("DEFAULT %s (%s): <%s>\n",
			core_button_names[mapping->retro], mapping->name,
			(mapping->local == BTN_ID_NONE ? "NONE" :
				device_button_names[mapping->local]));
		if (core_button_names[mapping->retro])
			mapping->name = (char *)core_button_names[mapping->retro];
	}

	puts("---------------------------------");

	for (int i = 0; config.controls[i].name; i++) {
		ButtonMapping *mapping = &config.controls[i];
		mapping->default_ = mapping->local;

		if (core_mapped && !present[mapping->retro]) {
			mapping->ignore = 1;
			continue;
		}
		LOG_info("%s: <%s> (%i:%i)\n", mapping->name,
			(mapping->local == BTN_ID_NONE ? "NONE" :
				device_button_names[mapping->local]),
			mapping->local, mapping->retro);
	}

	puts("---------------------------------");
	input_initialized = 1;
}
