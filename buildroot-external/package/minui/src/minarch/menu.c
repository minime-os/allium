#include "minarch.h"

#define MENU_ITEM_COUNT 5
#define MENU_SLOT_COUNT 8
#define OPTION_PADDING 8

enum {
	ITEM_CONT,
	ITEM_SAVE,
	ITEM_LOAD,
	ITEM_OPTS,
	ITEM_QUIT,
};

enum {
	STATUS_CONT = 0,
	STATUS_SAVE = 1,
	STATUS_LOAD = 11,
	STATUS_OPTS = 23,
	STATUS_DISC = 24,
	STATUS_QUIT = 30,
	STATUS_RESET = 31,
};

typedef struct MenuList MenuList;
typedef struct MenuItem MenuItem;

enum {
	MENU_CALLBACK_NOP,
	MENU_CALLBACK_EXIT,
	MENU_CALLBACK_NEXT_ITEM,
};

typedef int (*MenuList_callback_t)(MenuList *list, int i);

struct MenuItem {
	char *name;
	char *desc;
	char **values;
	char *key;
	int id;
	int value;
	MenuList *submenu;
	MenuList_callback_t on_confirm;
	MenuList_callback_t on_change;
};

enum {
	MENU_LIST,
	MENU_VAR,
	MENU_FIXED,
	MENU_INPUT,
};

struct MenuList {
	int type;
	int max_width;
	char *desc;
	MenuItem *items;
	MenuList_callback_t on_confirm;
	MenuList_callback_t on_change;
};

static struct {
	SDL_Surface *bitmap;
	SDL_Surface *overlay;
	char *items[MENU_ITEM_COUNT];
	char *disc_paths[9];
	char minui_dir[256];
	char slot_path[256];
	char base_path[256];
	char bmp_path[256];
	char txt_path[256];
	int disc;
	int total_discs;
	int slot;
	int save_exists;
	int preview_exists;
} menu = {
	.bitmap = NULL,
	.disc = -1,
	.total_discs = 0,
	.save_exists = 0,
	.preview_exists = 0,
	.items = {
		[ITEM_CONT] = "Continue",
		[ITEM_SAVE] = "Save",
		[ITEM_LOAD] = "Load",
		[ITEM_OPTS] = "Options",
		[ITEM_QUIT] = "Quit",
	}
};

static int Menu_message(char *message, char **pairs);
static int Menu_options(MenuList *list);
static void Menu_updateState(void);
static void Menu_scale(SDL_Surface *src, SDL_Surface *dst);
static char *getSaveDesc(void);
static void OptionSaveChanges_updateDesc(void);

void Menu_init(void)
{
	menu.overlay = SDL_CreateRGBSurface(SDL_SWSURFACE, DEVICE_WIDTH,
		DEVICE_HEIGHT, FIXED_DEPTH, RGBA_MASK_AUTO);
	SDLX_SetAlpha(menu.overlay, SDL_SRCALPHA, 0x80);
	SDL_FillRect(menu.overlay, NULL, 0);

	char core_registry_name[256];
	getEmuName(game.path, core_registry_name);
	snprintf(menu.minui_dir, sizeof(menu.minui_dir), "%s", core.states_dir);
	if (ensure_dir_recursive(menu.minui_dir, 0755) != 0) {
		LOG_error("Error creating slot dir: %s (%s)\n", menu.minui_dir,
			strerror(errno));
	}
	snprintf(menu.slot_path, sizeof(menu.slot_path), "%s/selected-slot.state",
		menu.minui_dir);

	if (simple_mode)
		menu.items[ITEM_OPTS] = "Reset";

	if (game.m3u_path[0]) {
		char *tmp;
		strcpy(menu.base_path, game.m3u_path);
		tmp = strrchr(menu.base_path, '/') + 1;
		tmp[0] = '\0';

		FILE *file = fopen(game.m3u_path, "r");
		if (file) {
			char line[256];
			while (fgets(line, 256, file) != NULL) {
				normalizeNewline(line);
				trimTrailingNewlines(line);
				if (strlen(line) == 0)
					continue;

				char disc_path[256];
				strcpy(disc_path, menu.base_path);
				tmp = disc_path + strlen(disc_path);
				strcpy(tmp, line);

				if (exists(disc_path)) {
					menu.disc_paths[menu.total_discs] = strdup(disc_path);
					if (exactMatch(disc_path, game.path))
						menu.disc = menu.total_discs;
					menu.total_discs += 1;
				}
			}
			fclose(file);
		}
	}
}

void Menu_quit(void)
{
	SDL_FreeSurface(menu.overlay);
}

void Menu_beforeSleep(void)
{
	SRAM_write();
	RTC_write();
	State_autosave();
	putFile(AUTO_RESUME_PATH, game.path + strlen(SDCARD_PATH));
	PWR_setCPUSpeed(CPU_SPEED_MENU);
}

void Menu_afterSleep(void)
{
	unlink(AUTO_RESUME_PATH);
	setOverclock(overclock);
}

static int Menu_message(char *message, char **pairs)
{
	GFX_setMode(MODE_MAIN);
	int dirty = 1;

	while (1) {
		GFX_startFrame();
		PAD_poll();

		if (PAD_justPressed(BTN_A) || PAD_justPressed(BTN_B))
			break;

		PWR_update(&dirty, NULL, Menu_beforeSleep, Menu_afterSleep);
		if (dirty) {
			GFX_clear(screen);
			GFX_blitMessage(font.medium, message, screen, &(SDL_Rect){
				0, SCALE1(PADDING),
				screen->w,
				screen->h - SCALE1(PILL_SIZE + PADDING)
			});
			GFX_blitButtonGroup(pairs, 0, screen, 1);
			GFX_flip(screen);
			dirty = 0;
		} else {
			GFX_sync();
		}

		hdmimon();
	}

	GFX_setMode(MODE_MENU);
	return MENU_CALLBACK_NOP;
}

static int MenuList_freeItems(MenuList *list, int i)
{
	(void)i;
	if (list->items)
		free(list->items);
	return MENU_CALLBACK_NOP;
}

static int OptionFrontend_optionChanged(MenuList *list, int i)
{
	MenuItem *item = &list->items[i];
	Config_syncFrontend(item->key, item->value);
	return MENU_CALLBACK_NOP;
}

static MenuList OptionFrontend_menu = {
	.type = MENU_VAR,
	.on_change = OptionFrontend_optionChanged,
	.items = NULL,
};

static int OptionFrontend_openMenu(MenuList *list, int i)
{
	(void)list;
	(void)i;
	if (OptionFrontend_menu.items == NULL) {
		if (!config.frontend.enabled_count) {
			int enabled_count = 0;
			for (int j = 0; j < config.frontend.count; j++) {
				if (!config.frontend.options[j].lock)
					enabled_count += 1;
			}
			config.frontend.enabled_count = enabled_count;
			config.frontend.enabled_options =
				calloc(enabled_count + 1, sizeof(Option *));
			int k = 0;
			for (int j = 0; j < config.frontend.count; j++) {
				Option *item = &config.frontend.options[j];
				if (item->lock)
					continue;
				config.frontend.enabled_options[k++] = item;
			}
		}

		OptionFrontend_menu.items =
			calloc(config.frontend.enabled_count + 1, sizeof(MenuItem));
		for (int j = 0; j < config.frontend.enabled_count; j++) {
			Option *option = config.frontend.enabled_options[j];
			MenuItem *item = &OptionFrontend_menu.items[j];
			item->key = option->key;
			item->name = option->name;
			item->desc = option->desc;
			item->value = option->value;
			item->values = option->labels;
		}
	} else {
		for (int j = 0; j < config.frontend.enabled_count; j++) {
			Option *option = config.frontend.enabled_options[j];
			MenuItem *item = &OptionFrontend_menu.items[j];
			item->value = option->value;
		}
	}

	Menu_options(&OptionFrontend_menu);
	return MENU_CALLBACK_NOP;
}

static int OptionEmulator_optionChanged(MenuList *list, int i)
{
	MenuItem *item = &list->items[i];
	Option *option = OptionList_getOption(&config.core, item->key);

	LOG_info("%s (%s) changed from `%s` (%s) to `%s` (%s)\n",
		item->name, item->key,
		item->values[option->value], option->values[option->value],
		item->values[item->value], option->values[item->value]);
	OptionList_setOptionRawValue(&config.core, item->key, item->value);
	return MENU_CALLBACK_NOP;
}

static int OptionEmulator_optionDetail(MenuList *list, int i)
{
	MenuItem *item = &list->items[i];
	Option *option = OptionList_getOption(&config.core, item->key);

	if (option->full)
		return Menu_message(option->full, (char *[]){"B", "BACK", NULL});
	return MENU_CALLBACK_NOP;
}

static MenuList OptionEmulator_menu = {
	.type = MENU_FIXED,
	.on_confirm = OptionEmulator_optionDetail,
	.on_change = OptionEmulator_optionChanged,
	.items = NULL,
};

static int OptionEmulator_openMenu(MenuList *list, int i)
{
	(void)list;
	(void)i;
	if (OptionEmulator_menu.items == NULL) {
		if (!config.core.enabled_count) {
			int enabled_count = 0;
			for (int j = 0; j < config.core.count; j++) {
				if (!config.core.options[j].lock)
					enabled_count += 1;
			}
			config.core.enabled_count = enabled_count;
			config.core.enabled_options =
				calloc(enabled_count + 1, sizeof(Option *));
			int k = 0;
			for (int j = 0; j < config.core.count; j++) {
				Option *item = &config.core.options[j];
				if (item->lock)
					continue;
				config.core.enabled_options[k++] = item;
			}
		}

		OptionEmulator_menu.items =
			calloc(config.core.enabled_count + 1, sizeof(MenuItem));
		for (int j = 0; j < config.core.enabled_count; j++) {
			Option *option = config.core.enabled_options[j];
			MenuItem *item = &OptionEmulator_menu.items[j];
			item->key = option->key;
			item->name = option->name;
			item->desc = option->desc;
			item->value = option->value;
			item->values = option->labels;
		}
	} else {
		for (int j = 0; j < config.core.enabled_count; j++) {
			Option *option = config.core.enabled_options[j];
			MenuItem *item = &OptionEmulator_menu.items[j];
			item->value = option->value;
		}
	}

	if (OptionEmulator_menu.items[0].name)
		Menu_options(&OptionEmulator_menu);
	else
		Menu_message("This core has no options.",
			(char *[]){"B", "BACK", NULL});

	return MENU_CALLBACK_NOP;
}

int OptionControls_bind(MenuList *list, int i)
{
	MenuItem *item = &list->items[i];
	if (item->values != button_labels)
		return MENU_CALLBACK_NOP;

	ButtonMapping *button = &config.controls[item->id];
	int bound = 0;

	while (!bound) {
		GFX_startFrame();
		PAD_poll();
		for (int id = 0; id <= LOCAL_BUTTON_COUNT; id++) {
			if (!PAD_justPressed(1 << (id - 1)))
				continue;
			item->value = id;
			button->local = id - 1;
			if (PAD_isPressed(BTN_MENU)) {
				item->value += LOCAL_BUTTON_COUNT;
				button->mod = 1;
			} else {
				button->mod = 0;
			}
			bound = 1;
			break;
		}
		GFX_sync();
		hdmimon();
	}
	return MENU_CALLBACK_NEXT_ITEM;
}

static int OptionControls_unbind(MenuList *list, int i)
{
	MenuItem *item = &list->items[i];
	if (item->values != button_labels)
		return MENU_CALLBACK_NOP;

	ButtonMapping *button = &config.controls[item->id];
	button->local = -1;
	button->mod = 0;
	return MENU_CALLBACK_NOP;
}

static int OptionControls_optionChanged(MenuList *list, int i)
{
	MenuItem *item = &list->items[i];
	if (item->values != gamepad_labels)
		return MENU_CALLBACK_NOP;

	if (has_custom_controllers) {
		gamepad_type = item->value;
		int device = strtol(gamepad_values[item->value], NULL, 0);
		core.set_controller_port_device(0, device);
	}
	return MENU_CALLBACK_NOP;
}

static MenuList OptionControls_menu = {
	.type = MENU_INPUT,
	.desc = "Press A to set and X to clear.\nSupports single button and "
		"MENU+button.",
	.on_confirm = OptionControls_bind,
	.on_change = OptionControls_unbind,
	.items = NULL
};

static int OptionControls_openMenu(MenuList *list, int i)
{
	(void)list;
	(void)i;
	LOG_info("OptionControls_openMenu\n");

	if (OptionControls_menu.items == NULL) {
		OptionControls_menu.items = calloc(RETRO_BUTTON_COUNT + 1 +
			has_custom_controllers, sizeof(MenuItem));
		int k = 0;

		if (has_custom_controllers) {
			MenuItem *item = &OptionControls_menu.items[k++];
			item->name = "Controller";
			item->desc = "Select the type of controller.";
			item->value = gamepad_type;
			item->values = gamepad_labels;
			item->on_change = OptionControls_optionChanged;
		}

		for (int j = 0; config.controls[j].name; j++) {
			ButtonMapping *button = &config.controls[j];
			if (button->ignore)
				continue;

			LOG_info("\t%s (%i:%i)\n", button->name, button->local,
				button->retro);

			MenuItem *item = &OptionControls_menu.items[k++];
			item->id = j;
			item->name = button->name;
			item->desc = NULL;
			item->value = button->local + 1;
			if (button->mod)
				item->value += LOCAL_BUTTON_COUNT;
			item->values = button_labels;
		}
	} else {
		int k = 0;
		if (has_custom_controllers) {
			MenuItem *item = &OptionControls_menu.items[k++];
			item->value = gamepad_type;
		}

		for (int j = 0; config.controls[j].name; j++) {
			ButtonMapping *button = &config.controls[j];
			if (button->ignore)
				continue;

			MenuItem *item = &OptionControls_menu.items[k++];
			item->value = button->local + 1;
			if (button->mod)
				item->value += LOCAL_BUTTON_COUNT;
		}
	}
	Menu_options(&OptionControls_menu);
	return MENU_CALLBACK_NOP;
}

static int OptionShortcuts_bind(MenuList *list, int i)
{
	MenuItem *item = &list->items[i];
	ButtonMapping *button = &config.shortcuts[item->id];
	int bound = 0;

	while (!bound) {
		GFX_startFrame();
		PAD_poll();
		for (int id = 0; id <= LOCAL_BUTTON_COUNT; id++) {
			if (!PAD_justPressed(1 << (id - 1)))
				continue;
			item->value = id;
			button->local = id - 1;
			if (PAD_isPressed(BTN_MENU)) {
				item->value += LOCAL_BUTTON_COUNT;
				button->mod = 1;
			} else {
				button->mod = 0;
			}
			bound = 1;
			break;
		}
		GFX_sync();
		hdmimon();
	}
	return MENU_CALLBACK_NEXT_ITEM;
}

static int OptionShortcuts_unbind(MenuList *list, int i)
{
	MenuItem *item = &list->items[i];
	ButtonMapping *button = &config.shortcuts[item->id];
	(void)item;
	button->local = -1;
	button->mod = 0;
	return MENU_CALLBACK_NOP;
}

static MenuList OptionShortcuts_menu = {
	.type = MENU_INPUT,
	.desc = "Press A to set and X to clear.\nSupports single button and "
		"MENU+button.",
	.on_confirm = OptionShortcuts_bind,
	.on_change = OptionShortcuts_unbind,
	.items = NULL
};

static char *getSaveDesc(void)
{
	switch (config.loaded) {
	case CONFIG_NONE: return "Using defaults.";
	case CONFIG_CONSOLE: return "Using console config.";
	case CONFIG_GAME: return "Using game config.";
	}
	return NULL;
}

static int OptionShortcuts_openMenu(MenuList *list, int i)
{
	(void)list;
	(void)i;
	if (OptionShortcuts_menu.items == NULL) {
		OptionShortcuts_menu.items =
			calloc(SHORTCUT_COUNT + 1, sizeof(MenuItem));
		for (int j = 0; config.shortcuts[j].name; j++) {
			ButtonMapping *button = &config.shortcuts[j];
			MenuItem *item = &OptionShortcuts_menu.items[j];
			item->id = j;
			item->name = button->name;
			item->desc = NULL;
			item->value = button->local + 1;
			if (button->mod)
				item->value += LOCAL_BUTTON_COUNT;
			item->values = button_labels;
		}
	} else {
		for (int j = 0; config.shortcuts[j].name; j++) {
			ButtonMapping *button = &config.shortcuts[j];
			MenuItem *item = &OptionShortcuts_menu.items[j];
			item->value = button->local + 1;
			if (button->mod)
				item->value += LOCAL_BUTTON_COUNT;
		}
	}
	Menu_options(&OptionShortcuts_menu);
	return MENU_CALLBACK_NOP;
}

static int OptionSaveChanges_onConfirm(MenuList *list, int i)
{
	char *message;
	(void)list;

	switch (i) {
	case 0:
		Config_write(CONFIG_WRITE_ALL);
		message = "Saved for console.";
		break;
	case 1:
		Config_write(CONFIG_WRITE_GAME);
		message = "Saved for game.";
		break;
	default:
		Config_restore();
		if (config.loaded)
			message = "Restored console defaults.";
		else
			message = "Restored defaults.";
		break;
	}
	Menu_message(message, (char *[]){"A", "OKAY", NULL});
	OptionSaveChanges_updateDesc();
	return MENU_CALLBACK_EXIT;
}

static MenuList OptionSaveChanges_menu = {
	.type = MENU_LIST,
	.on_confirm = OptionSaveChanges_onConfirm,
	.items = (MenuItem[]){
		{"Save for console"},
		{"Save for game"},
		{"Restore defaults"},
		{NULL},
	}
};

static int OptionSaveChanges_openMenu(MenuList *list, int i)
{
	(void)list;
	(void)i;
	OptionSaveChanges_updateDesc();
	OptionSaveChanges_menu.desc = getSaveDesc();
	Menu_options(&OptionSaveChanges_menu);
	return MENU_CALLBACK_NOP;
}

static int OptionQuicksave_onConfirm(MenuList *list, int i)
{
	(void)list;
	(void)i;
	Menu_beforeSleep();
	PWR_powerOff();
	return MENU_CALLBACK_EXIT;
}

static MenuList options_menu = {
	.type = MENU_LIST,
	.items = (MenuItem[]) {
		{"Frontend", "MinUI (" BUILD_DATE " " BUILD_HASH ")",
			.on_confirm = OptionFrontend_openMenu},
		{"Emulator", .on_confirm = OptionEmulator_openMenu},
		{"Controls", .on_confirm = OptionControls_openMenu},
		{"Shortcuts", .on_confirm = OptionShortcuts_openMenu},
		{"Save Changes", .on_confirm = OptionSaveChanges_openMenu},
		{NULL},
		{NULL},
		{NULL},
	}
};

void Menu_setCoreVersion(const char *version)
{
	options_menu.items[1].desc = (char *)version;
}

static void OptionSaveChanges_updateDesc(void)
{
	options_menu.items[4].desc = getSaveDesc();
}

static int Menu_options(MenuList *list)
{
	MenuItem *items = list->items;
	int type = list->type;
	int dirty = 1;
	int show_options = 1;
	int show_settings = 0;
	int await_input = 0;
	int max_visible_options = (screen->h -
		((SCALE1(PADDING + PILL_SIZE) * 2) + SCALE1(BUTTON_SIZE))) /
		SCALE1(BUTTON_SIZE);

	int count;
	for (count = 0; items[count].name; count++)
		;
	int selected = 0;
	int start = 0;
	int end = MIN(count, max_visible_options);
	int visible_rows = end;

	OptionSaveChanges_updateDesc();

	int defer_menu = false;
	while (show_options) {
		if (await_input) {
			defer_menu = true;
			list->on_confirm(list, selected);

			selected += 1;
			if (selected >= count) {
				selected = 0;
				start = 0;
				end = visible_rows;
			} else if (selected >= end) {
				start += 1;
				end += 1;
			}
			dirty = 1;
			await_input = false;
		}

		GFX_startFrame();
		PAD_poll();

		if (PAD_justRepeated(BTN_UP)) {
			selected -= 1;
			if (selected < 0) {
				selected = count - 1;
				start = MAX(0, count - max_visible_options);
				end = count;
			} else if (selected < start) {
				start -= 1;
				end -= 1;
			}
			dirty = 1;
		} else if (PAD_justRepeated(BTN_DOWN)) {
			selected += 1;
			if (selected >= count) {
				selected = 0;
				start = 0;
				end = visible_rows;
			} else if (selected >= end) {
				start += 1;
				end += 1;
			}
			dirty = 1;
		} else {
			MenuItem *item = &items[selected];
			if (item->values && item->values != button_labels) {
				if (PAD_justRepeated(BTN_LEFT)) {
					if (item->value > 0) {
						item->value -= 1;
					} else {
						int j;
						for (j = 0; item->values[j]; j++)
							;
						item->value = j - 1;
					}

					if (item->on_change)
						item->on_change(list, selected);
					else if (list->on_change)
						list->on_change(list, selected);
					dirty = 1;
				} else if (PAD_justRepeated(BTN_RIGHT)) {
					if (item->values[item->value + 1])
						item->value += 1;
					else
						item->value = 0;

					if (item->on_change)
						item->on_change(list, selected);
					else if (list->on_change)
						list->on_change(list, selected);
					dirty = 1;
				}
			}
		}

		if (PAD_justPressed(BTN_B)) {
			show_options = 0;
		} else if (PAD_justPressed(BTN_A)) {
			MenuItem *item = &items[selected];
			int result = MENU_CALLBACK_NOP;
			if (item->on_confirm)
				result = item->on_confirm(list, selected);
			else if (item->submenu)
				result = Menu_options(item->submenu);
			else if (list->on_confirm) {
				if (item->values == button_labels)
					await_input = 1;
				else
					result = list->on_confirm(list, selected);
			}

			if (result == MENU_CALLBACK_EXIT) {
				show_options = 0;
			} else {
				if (result == MENU_CALLBACK_NEXT_ITEM) {
					selected += 1;
					if (selected >= count) {
						selected = 0;
						start = 0;
						end = visible_rows;
					} else if (selected >= end) {
						start += 1;
						end += 1;
					}
				}
				dirty = 1;
			}
		} else if (type == MENU_INPUT) {
			if (PAD_justPressed(BTN_X)) {
				MenuItem *item = &items[selected];
				item->value = 0;

				if (item->on_change)
					item->on_change(list, selected);
				else if (list->on_change)
					list->on_change(list, selected);

				selected += 1;
				if (selected >= count) {
					selected = 0;
					start = 0;
					end = visible_rows;
				} else if (selected >= end) {
					start += 1;
					end += 1;
				}
				dirty = 1;
			}
		}

		if (!defer_menu)
			PWR_update(&dirty, &show_settings, Menu_beforeSleep,
				Menu_afterSleep);
		if (defer_menu && PAD_justReleased(BTN_MENU))
			defer_menu = false;

		if (dirty) {
			GFX_clear(screen);
			GFX_blitHardwareGroup(screen, show_settings);

			char *desc = NULL;
			SDL_Surface *text;

			if (type == MENU_LIST) {
				int mw = list->max_width;
				if (!mw) {
					for (int i = 0; i < count; i++) {
						MenuItem *item = &items[i];
						int w = 0;
						TTF_SizeUTF8(font.small, item->name, &w, NULL);
						w += SCALE1(OPTION_PADDING * 2);
						if (w > mw)
							mw = w;
					}
					list->max_width = mw =
						MIN(mw, screen->w - SCALE1(PADDING * 2));
				}

				int ox = (screen->w - mw) / 2;
				int oy = SCALE1(PADDING + PILL_SIZE);
				int selected_row = selected - start;
				for (int i = start, j = 0; i < end; i++, j++) {
					MenuItem *item = &items[i];
					SDL_Color text_color = COLOR_WHITE;

					if (j == selected_row) {
						int w = 0;
						TTF_SizeUTF8(font.small, item->name, &w, NULL);
						w += SCALE1(OPTION_PADDING * 2);
						GFX_blitPill(ASSET_BUTTON, screen, &(SDL_Rect){
							ox,
							oy + SCALE1(j * BUTTON_SIZE),
							w,
							SCALE1(BUTTON_SIZE)
						});
						text_color = COLOR_BLACK;
						if (item->desc)
							desc = item->desc;
					}
					text = TTF_RenderUTF8_Blended(font.small,
						item->name, text_color);
					SDL_BlitSurface(text, NULL, screen, &(SDL_Rect){
						ox + SCALE1(OPTION_PADDING),
						oy + SCALE1((j * BUTTON_SIZE) + 1)
					});
					SDL_FreeSurface(text);
				}
			} else if (type == MENU_FIXED) {
				int mw = screen->w - SCALE1(PADDING * 2);
				int ox = SCALE1(PADDING);
				int oy = SCALE1(PADDING + PILL_SIZE);
				int selected_row = selected - start;
				for (int i = start, j = 0; i < end; i++, j++) {
					MenuItem *item = &items[i];
					SDL_Color text_color = COLOR_WHITE;

					if (j == selected_row) {
						GFX_blitPill(ASSET_OPTION, screen, &(SDL_Rect){
							ox, oy + SCALE1(j * BUTTON_SIZE), mw,
							SCALE1(BUTTON_SIZE)
						});
					}

					if (item->value >= 0) {
						text = TTF_RenderUTF8_Blended(font.tiny,
							item->values[item->value],
							COLOR_WHITE);
						SDL_BlitSurface(text, NULL, screen,
							&(SDL_Rect){
								ox + mw - text->w -
									SCALE1(OPTION_PADDING),
								oy + SCALE1((j * BUTTON_SIZE) + 3)
							});
						SDL_FreeSurface(text);
					}

					if (j == selected_row) {
						int w = 0;
						TTF_SizeUTF8(font.small, item->name, &w, NULL);
						w += SCALE1(OPTION_PADDING * 2);
						GFX_blitPill(ASSET_BUTTON, screen, &(SDL_Rect){
							ox, oy + SCALE1(j * BUTTON_SIZE), w,
							SCALE1(BUTTON_SIZE)
						});
						text_color = COLOR_BLACK;
						if (item->desc)
							desc = item->desc;
					}
					text = TTF_RenderUTF8_Blended(font.small,
						item->name, text_color);
					SDL_BlitSurface(text, NULL, screen, &(SDL_Rect){
						ox + SCALE1(OPTION_PADDING),
						oy + SCALE1((j * BUTTON_SIZE) + 1)
					});
					SDL_FreeSurface(text);
				}
			} else if (type == MENU_VAR || type == MENU_INPUT) {
				int mw = list->max_width;
				if (!mw) {
					int mrw = 0;
					for (int i = 0; i < count; i++) {
						MenuItem *item = &items[i];
						int w = 0;
						int lw = 0;
						int rw = 0;
						TTF_SizeUTF8(font.small, item->name, &lw, NULL);
						if (!mrw || type != MENU_INPUT) {
							for (int j = 0; item->values[j]; j++) {
								TTF_SizeUTF8(font.tiny,
									item->values[j], &rw, NULL);
								if (lw + rw > w)
									w = lw + rw;
								if (rw > mrw)
									mrw = rw;
							}
						} else {
							w = lw + mrw;
						}
						w += SCALE1(OPTION_PADDING * 4);
						if (w > mw)
							mw = w;
					}
					list->max_width = mw =
						MIN(mw, screen->w - SCALE1(PADDING * 2));
				}

				int ox = (screen->w - mw) / 2;
				int oy = SCALE1(PADDING + PILL_SIZE);
				int selected_row = selected - start;
				for (int i = start, j = 0; i < end; i++, j++) {
					MenuItem *item = &items[i];
					SDL_Color text_color = COLOR_WHITE;

					if (j == selected_row) {
						GFX_blitPill(ASSET_OPTION, screen, &(SDL_Rect){
							ox, oy + SCALE1(j * BUTTON_SIZE), mw,
							SCALE1(BUTTON_SIZE)
						});

						int w = 0;
						TTF_SizeUTF8(font.small, item->name, &w, NULL);
						w += SCALE1(OPTION_PADDING * 2);
						GFX_blitPill(ASSET_BUTTON, screen, &(SDL_Rect){
							ox, oy + SCALE1(j * BUTTON_SIZE), w,
							SCALE1(BUTTON_SIZE)
						});
						text_color = COLOR_BLACK;
						if (item->desc)
							desc = item->desc;
					}
					text = TTF_RenderUTF8_Blended(font.small,
						item->name, text_color);
					SDL_BlitSurface(text, NULL, screen, &(SDL_Rect){
						ox + SCALE1(OPTION_PADDING),
						oy + SCALE1((j * BUTTON_SIZE) + 1)
					});
					SDL_FreeSurface(text);

					if (await_input && j == selected_row) {
					} else if (item->value >= 0) {
						text = TTF_RenderUTF8_Blended(font.tiny,
							item->values[item->value],
							COLOR_WHITE);
						SDL_BlitSurface(text, NULL, screen,
							&(SDL_Rect){
								ox + mw - text->w -
									SCALE1(OPTION_PADDING),
								oy + SCALE1((j * BUTTON_SIZE) + 3)
							});
						SDL_FreeSurface(text);
					}
				}
			}

			if (count > max_visible_options) {
#define SCROLL_WIDTH 24
#define SCROLL_HEIGHT 4
				int ox = (screen->w - SCALE1(SCROLL_WIDTH)) / 2;
				int oy = SCALE1((PILL_SIZE - SCROLL_HEIGHT) / 2);
				if (start > 0) {
					GFX_blitAsset(ASSET_SCROLL_UP, NULL, screen,
						&(SDL_Rect){ox, SCALE1(PADDING) + oy});
				}
				if (end < count) {
					GFX_blitAsset(ASSET_SCROLL_DOWN, NULL, screen,
						&(SDL_Rect){
							ox,
							screen->h - SCALE1(PADDING + PILL_SIZE +
								BUTTON_SIZE) + oy
						});
				}
			}

			if (!desc && list->desc)
				desc = list->desc;

			if (desc) {
				int w;
				int h;
				GFX_sizeText(font.tiny, desc, SCALE1(12), &w, &h);
				GFX_blitText(font.tiny, desc, SCALE1(12), COLOR_WHITE,
					screen, &(SDL_Rect){
						(screen->w - w) / 2,
						screen->h - SCALE1(PADDING) - h,
						w, h
					});
			}

			GFX_flip(screen);
			dirty = 0;
		} else {
			GFX_sync();
		}
		hdmimon();
	}

	return 0;
}

static void Menu_scale(SDL_Surface *src, SDL_Surface *dst)
{
	uint16_t *s = src->pixels;
	uint16_t *d = dst->pixels;

	int sw = src->w;
	int sh = src->h;
	int sp = src->pitch / FIXED_BPP;
	int dw = dst->w;
	int dh = dst->h;
	int dp = dst->pitch / FIXED_BPP;
	int rx = 0;
	int ry = 0;
	int rw = dw;
	int rh = dh;

	int scaling = screen_scaling;
	if (scaling == SCALE_CROPPED && DEVICE_WIDTH == HDMI_WIDTH)
		scaling = SCALE_NATIVE;
	if (scaling == SCALE_NATIVE) {
		rx = renderer.dst_x;
		ry = renderer.dst_y;
		rw = renderer.src_w;
		rh = renderer.src_h;
		if (renderer.scale) {
			rw *= renderer.scale;
			rh *= renderer.scale;
		} else {
			rw -= renderer.src_x * 2;
			rh -= renderer.src_y * 2;
			sw = rw;
			sh = rh;
		}

		if (dw == DEVICE_WIDTH / 2) {
			rx /= 2;
			ry /= 2;
			rw /= 2;
			rh /= 2;
		}
	} else if (scaling == SCALE_CROPPED) {
		sw -= renderer.src_x * 2;
		sh -= renderer.src_y * 2;
		rx = renderer.dst_x;
		ry = renderer.dst_y;
		rw = sw * renderer.scale;
		rh = sh * renderer.scale;

		if (dw == DEVICE_WIDTH / 2) {
			rx /= 2;
			ry /= 2;
			rw /= 2;
			rh /= 2;
		}
	}

	if (scaling == SCALE_ASPECT || rw > dw || rh > dh) {
		double fixed_aspect_ratio = ((double)DEVICE_WIDTH) / DEVICE_HEIGHT;
		int core_aspect = core.aspect_ratio * 1000;
		int fixed_aspect = fixed_aspect_ratio * 1000;

		if (core_aspect > fixed_aspect) {
			rw = dw;
			rh = rw / core.aspect_ratio;
			rh += rh % 2;
		} else if (core_aspect < fixed_aspect) {
			rh = dh;
			rw = rh * core.aspect_ratio;
			rw += rw % 2;
			rw = (rw / 8) * 8;
		} else {
			rw = dw;
			rh = dh;
		}

		rx = (dw - rw) / 2;
		ry = (dh - rh) / 2;
	}

	int mx = (sw << 16) / rw;
	int my = (sh << 16) / rh;
	int ox = (renderer.src_x << 16);
	int sx = ox;
	int sy = (renderer.src_y << 16);
	int lr = -1;
	int sr = 0;
	int dr = ry * dp;
	int cp = dp * FIXED_BPP;

	for (int dy = 0; dy < rh; dy++) {
		sx = ox;
		sr = (sy >> 16) * sp;
		if (sr == lr) {
			memcpy(d + dr, d + dr - dp, cp);
		} else {
			for (int dx = 0; dx < rw; dx++) {
				d[dr + rx + dx] = s[sr + (sx >> 16)];
				sx += mx;
			}
		}
		lr = sr;
		sy += my;
		dr += dp;
	}
}

void Menu_initState(void)
{
	if (exists(menu.slot_path))
		menu.slot = getInt(menu.slot_path);
	if (menu.slot == 8)
		menu.slot = 0;

	menu.save_exists = 0;
	menu.preview_exists = 0;
}

static void Menu_updateState(void)
{
	int last_slot = state_slot;
	state_slot = menu.slot;

	char save_path[256];
	State_getPath(save_path);
	state_slot = last_slot;

	{
		char slot_dir[256];
		snprintf(slot_dir, sizeof(slot_dir), "%s/slot %d",
			menu.minui_dir, menu.slot + 1);
		snprintf(menu.bmp_path, sizeof(menu.bmp_path), "%s/preview.bmp",
			slot_dir);
		snprintf(menu.txt_path, sizeof(menu.txt_path), "%s/disc.txt",
			slot_dir);
	}

	menu.save_exists = exists(save_path);
	menu.preview_exists = menu.save_exists && exists(menu.bmp_path);
}

void Menu_saveState(void)
{
	Menu_updateState();

	if (menu.total_discs) {
		char *disc_path = menu.disc_paths[menu.disc];
		putFile(menu.txt_path, disc_path + strlen(menu.base_path));
	}

	SDL_Surface *bitmap = menu.bitmap;
	if (!bitmap) {
		bitmap = SDL_CreateRGBSurfaceFrom(renderer.src, renderer.true_w,
			renderer.true_h, FIXED_DEPTH, renderer.src_p,
			RGBA_MASK_565);
	}
	SDL_RWops *out = SDL_RWFromFile(menu.bmp_path, "wb");
	SDL_SaveBMP_RW(bitmap, out, 1);

	if (bitmap != menu.bitmap)
		SDL_FreeSurface(bitmap);

	state_slot = menu.slot;
	putInt(menu.slot_path, menu.slot);
	State_write();
}

void Menu_loadState(void)
{
	Menu_updateState();

	if (menu.save_exists) {
		if (menu.total_discs) {
			char slot_disc_name[256];
			getFile(menu.txt_path, slot_disc_name, 256);

			char slot_disc_path[256];
			if (slot_disc_name[0] == '/')
				strcpy(slot_disc_path, slot_disc_name);
			else
				sprintf(slot_disc_path, "%s%s", menu.base_path,
					slot_disc_name);

			char *disc_path = menu.disc_paths[menu.disc];
			if (!exactMatch(slot_disc_path, disc_path))
				Game_changeDisc(slot_disc_path);
		}

		state_slot = menu.slot;
		putInt(menu.slot_path, menu.slot);
		State_read();
	}
}

static char *getAlias(char *path, char *alias)
{
	char *tmp;
	char map_path[256];
	strcpy(map_path, path);
	tmp = strrchr(map_path, '/');
	if (tmp) {
		tmp += 1;
		strcpy(tmp, "map.txt");
	}
	char *file_name = strrchr(path, '/');
	if (file_name)
		file_name += 1;

	if (exists(map_path)) {
		FILE *file = fopen(map_path, "r");
		if (file) {
			char line[256];
			while (fgets(line, 256, file) != NULL) {
				normalizeNewline(line);
				trimTrailingNewlines(line);
				if (strlen(line) == 0)
					continue;

				tmp = strchr(line, '\t');
				if (!tmp)
					continue;
				tmp[0] = '\0';
				char *key = line;
				char *value = tmp + 1;
				if (!exactMatch(file_name, key))
					continue;
				strcpy(alias, value);
				break;
			}
			fclose(file);
		}
	}
	return alias;
}

void Menu_loop(void)
{
	menu.bitmap = SDL_CreateRGBSurfaceFrom(renderer.src, renderer.true_w,
		renderer.true_h, FIXED_DEPTH, renderer.src_p, RGBA_MASK_565);

	SDL_Surface *backing = SDL_CreateRGBSurface(SDL_SWSURFACE, DEVICE_WIDTH,
		DEVICE_HEIGHT, FIXED_DEPTH, RGBA_MASK_565);
	Menu_scale(menu.bitmap, backing);

	int restore_w = screen->w;
	int restore_h = screen->h;
	int restore_p = screen->pitch;
	if (restore_w != DEVICE_WIDTH || restore_h != DEVICE_HEIGHT)
		screen = GFX_resize(DEVICE_WIDTH, DEVICE_HEIGHT, DEVICE_PITCH);

	SRAM_write();
	RTC_write();
	PWR_warn(0);
	if (!HAS_POWER_BUTTON)
		PWR_enableSleep();
	PWR_setCPUSpeed(CPU_SPEED_MENU);
	GFX_setVsync(VSYNC_STRICT);
	GFX_setEffect(EFFECT_NONE);

	int rumble_strength = VIB_getStrength();
	VIB_setStrength(0);

	PWR_enableAutosleep();
	PAD_reset();

	char rom_name[256];
	getDisplayName(game.name, rom_name);
	getAlias(game.path, rom_name);

	int rom_disc = -1;
	char disc_name[16];
	if (menu.total_discs) {
		rom_disc = menu.disc;
		sprintf(disc_name, "Disc %i", menu.disc + 1);
	}

	int selected = 0;
	Menu_initState();

	int status = STATUS_CONT;
	int show_setting = 0;
	int dirty = 1;
	SDL_Surface *preview = SDL_CreateRGBSurface(SDL_SWSURFACE, DEVICE_WIDTH / 2,
		DEVICE_HEIGHT / 2, FIXED_DEPTH, RGBA_MASK_565);

	while (show_menu) {
		GFX_startFrame();
		uint32_t now = SDL_GetTicks();
		PAD_poll();

		if (PAD_justPressed(BTN_UP)) {
			selected -= 1;
			if (selected < 0)
				selected += MENU_ITEM_COUNT;
			dirty = 1;
		} else if (PAD_justPressed(BTN_DOWN)) {
			selected += 1;
			if (selected >= MENU_ITEM_COUNT)
				selected -= MENU_ITEM_COUNT;
			dirty = 1;
		} else if (PAD_justPressed(BTN_LEFT)) {
			if (menu.total_discs > 1 && selected == ITEM_CONT) {
				menu.disc -= 1;
				if (menu.disc < 0)
					menu.disc += menu.total_discs;
				dirty = 1;
				sprintf(disc_name, "Disc %i", menu.disc + 1);
			} else if (selected == ITEM_SAVE || selected == ITEM_LOAD) {
				menu.slot -= 1;
				if (menu.slot < 0)
					menu.slot += MENU_SLOT_COUNT;
				dirty = 1;
			}
		} else if (PAD_justPressed(BTN_RIGHT)) {
			if (menu.total_discs > 1 && selected == ITEM_CONT) {
				menu.disc += 1;
				if (menu.disc == menu.total_discs)
					menu.disc -= menu.total_discs;
				dirty = 1;
				sprintf(disc_name, "Disc %i", menu.disc + 1);
			} else if (selected == ITEM_SAVE || selected == ITEM_LOAD) {
				menu.slot += 1;
				if (menu.slot >= MENU_SLOT_COUNT)
					menu.slot -= MENU_SLOT_COUNT;
				dirty = 1;
			}
		}

		if (dirty && (selected == ITEM_SAVE || selected == ITEM_LOAD))
			Menu_updateState();

		if (PAD_justPressed(BTN_B) ||
			(BTN_WAKE != BTN_MENU && PAD_tappedMenu(now))) {
			status = STATUS_CONT;
			show_menu = 0;
		} else if (PAD_justPressed(BTN_A)) {
			switch (selected) {
			case ITEM_CONT:
				if (menu.total_discs && rom_disc != menu.disc) {
					status = STATUS_DISC;
					char *disc_path = menu.disc_paths[menu.disc];
					Game_changeDisc(disc_path);
				} else {
					status = STATUS_CONT;
				}
				show_menu = 0;
				break;
			case ITEM_SAVE:
				Menu_saveState();
				status = STATUS_SAVE;
				show_menu = 0;
				break;
			case ITEM_LOAD:
				Menu_loadState();
				status = STATUS_LOAD;
				show_menu = 0;
				break;
			case ITEM_OPTS:
				if (simple_mode) {
					Core_reset();
					status = STATUS_RESET;
					show_menu = 0;
				} else {
					int old_scaling = screen_scaling;
					Menu_options(&options_menu);
					if (screen_scaling != old_scaling) {
						selectScaler(renderer.true_w, renderer.true_h,
							renderer.src_p);
						restore_w = screen->w;
						restore_h = screen->h;
						restore_p = screen->pitch;
						screen = GFX_resize(DEVICE_WIDTH,
							DEVICE_HEIGHT, DEVICE_PITCH);
						SDL_FillRect(backing, NULL, 0);
						Menu_scale(menu.bitmap, backing);
					}
					dirty = 1;
				}
				break;
			case ITEM_QUIT:
				status = STATUS_QUIT;
				show_menu = 0;
				quit = 1;
				break;
			}
			if (!show_menu)
				break;
		}

		PWR_update(&dirty, &show_setting, Menu_beforeSleep,
			Menu_afterSleep);

		if (dirty) {
			GFX_clear(screen);
			SDL_BlitSurface(backing, NULL, screen, NULL);
			SDL_BlitSurface(menu.overlay, NULL, screen, NULL);

			int ox;
			int oy;
			int ow = GFX_blitHardwareGroup(screen, show_setting);
			int max_width = screen->w - SCALE1(PADDING * 2) - ow;

			char display_name[256];
			int text_width = GFX_truncateText(font.large, rom_name,
				display_name, max_width, SCALE1(BUTTON_PADDING * 2));
			max_width = MIN(max_width, text_width);

			SDL_Surface *text = TTF_RenderUTF8_Blended(font.large,
				display_name, COLOR_WHITE);
			GFX_blitPill(ASSET_BLACK_PILL, screen, &(SDL_Rect){
				SCALE1(PADDING),
				SCALE1(PADDING),
				max_width,
				SCALE1(PILL_SIZE)
			});
			SDL_BlitSurface(text, &(SDL_Rect){
				0, 0,
				max_width - SCALE1(BUTTON_PADDING * 2),
				text->h
			}, screen, &(SDL_Rect){
				SCALE1(PADDING + BUTTON_PADDING),
				SCALE1(PADDING + 4)
			});
			SDL_FreeSurface(text);

			if (show_setting && !GetHDMI()) {
				GFX_blitHardwareHints(screen, show_setting);
			} else {
				GFX_blitButtonGroup((char *[]){
					BTN_SLEEP == BTN_POWER ? "POWER" : "MENU",
					"SLEEP",
					NULL
				}, 0, screen, 0);
			}
			GFX_blitButtonGroup((char *[]){"B", "BACK", "A", "OKAY",
				NULL}, 1, screen, 1);

			oy = (((DEVICE_HEIGHT / FIXED_SCALE) - PADDING * 2) -
				(MENU_ITEM_COUNT * PILL_SIZE)) / 2;
			for (int i = 0; i < MENU_ITEM_COUNT; i++) {
				char *item = menu.items[i];
				SDL_Color text_color = COLOR_WHITE;

				if (i == selected) {
					if (menu.total_discs > 1 && i == ITEM_CONT) {
						GFX_blitPill(ASSET_DARK_GRAY_PILL, screen,
							&(SDL_Rect){
								SCALE1(PADDING),
								SCALE1(oy + PADDING),
								screen->w -
									SCALE1(PADDING * 2),
								SCALE1(PILL_SIZE)
							});
						text = TTF_RenderUTF8_Blended(font.large,
							disc_name, COLOR_WHITE);
						SDL_BlitSurface(text, NULL, screen,
							&(SDL_Rect){
								screen->w -
									SCALE1(PADDING +
										BUTTON_PADDING) -
									text->w,
								SCALE1(oy + PADDING + 4)
							});
						SDL_FreeSurface(text);
					}

					TTF_SizeUTF8(font.large, item, &ow, NULL);
					ow += SCALE1(BUTTON_PADDING * 2);
					GFX_blitPill(ASSET_WHITE_PILL, screen, &(SDL_Rect){
						SCALE1(PADDING),
						SCALE1(oy + PADDING + (i * PILL_SIZE)),
						ow,
						SCALE1(PILL_SIZE)
					});
					text_color = COLOR_BLACK;
				} else {
					text = TTF_RenderUTF8_Blended(font.large, item,
						COLOR_BLACK);
					SDL_BlitSurface(text, NULL, screen, &(SDL_Rect){
						SCALE1(2 + PADDING + BUTTON_PADDING),
						SCALE1(1 + PADDING + oy +
							(i * PILL_SIZE) + 4)
					});
					SDL_FreeSurface(text);
				}

				text = TTF_RenderUTF8_Blended(font.large, item, text_color);
				SDL_BlitSurface(text, NULL, screen, &(SDL_Rect){
					SCALE1(PADDING + BUTTON_PADDING),
					SCALE1(oy + PADDING + (i * PILL_SIZE) + 4)
				});
				SDL_FreeSurface(text);
			}

			if (selected == ITEM_SAVE || selected == ITEM_LOAD) {
#define WINDOW_RADIUS 4
#define PAGINATION_HEIGHT 6
				int hw = DEVICE_WIDTH / 2;
				int hh = DEVICE_HEIGHT / 2;
				int pw = hw + SCALE1(WINDOW_RADIUS * 2);
				int ph = hh + SCALE1(WINDOW_RADIUS * 2 +
					PAGINATION_HEIGHT + WINDOW_RADIUS);
				ox = DEVICE_WIDTH - pw - SCALE1(PADDING);
				oy = (DEVICE_HEIGHT - ph) / 2;

				GFX_blitRect(ASSET_STATE_BG, screen, &(SDL_Rect){
					ox, oy, pw, ph
				});
				ox += SCALE1(WINDOW_RADIUS);
				oy += SCALE1(WINDOW_RADIUS);

				if (menu.preview_exists) {
					SDL_Surface *bmp = IMG_Load(menu.bmp_path);
					SDL_Surface *raw_preview = SDL_ConvertSurface(bmp,
						screen->format, SDL_SWSURFACE);
					SDL_FillRect(preview, NULL, 0);
					Menu_scale(raw_preview, preview);
					SDL_BlitSurface(preview, NULL, screen,
						&(SDL_Rect){ox, oy});
					SDL_FreeSurface(raw_preview);
					SDL_FreeSurface(bmp);
				} else {
					SDL_Rect preview_rect = {ox, oy, hw, hh};
					SDL_FillRect(screen, &preview_rect, 0);
					if (menu.save_exists)
						GFX_blitMessage(font.large, "No Preview",
							screen, &preview_rect);
					else
						GFX_blitMessage(font.large, "Empty Slot",
							screen, &preview_rect);
				}

				ox += (pw - SCALE1(15 * MENU_SLOT_COUNT)) / 2;
				oy += hh + SCALE1(WINDOW_RADIUS);
				for (int i = 0; i < MENU_SLOT_COUNT; i++) {
					if (i == menu.slot) {
						GFX_blitAsset(ASSET_PAGE, NULL, screen,
							&(SDL_Rect){
								ox + SCALE1(i * 15), oy
							});
					} else {
						GFX_blitAsset(ASSET_DOT, NULL, screen,
							&(SDL_Rect){
								ox + SCALE1(i * 15) + 4,
								oy + SCALE1(2)
							});
					}
				}
			}

			GFX_flip(screen);
			dirty = 0;
		} else {
			GFX_sync();
		}
		hdmimon();
	}

	SDL_FreeSurface(preview);
	PAD_reset();

	GFX_clearAll();
	PWR_warn(1);

	if (!quit) {
		if (restore_w != DEVICE_WIDTH || restore_h != DEVICE_HEIGHT)
			screen = GFX_resize(restore_w, restore_h, restore_p);
		GFX_setEffect(screen_effect);
		GFX_clear(screen);
		video_refresh_callback(renderer.src, renderer.true_w, renderer.true_h,
			renderer.src_p);

		setOverclock(overclock);
		if (rumble_strength)
			VIB_setStrength(rumble_strength);

		GFX_setVsync(prevent_tearing);
		if (!HAS_POWER_BUTTON)
			PWR_disableSleep();

		if (thread_video) {
			pthread_mutex_lock(&core_mx);
			should_run_core = 1;
			pthread_mutex_unlock(&core_mx);
		}
	} else if (exists(NOUI_PATH)) {
		PWR_powerOff();
	}

	SDL_FreeSurface(menu.bitmap);
	menu.bitmap = NULL;
	SDL_FreeSurface(backing);
	PWR_disableAutosleep();
	(void)status;
	(void)OptionQuicksave_onConfirm;
	(void)MenuList_freeItems;
}
