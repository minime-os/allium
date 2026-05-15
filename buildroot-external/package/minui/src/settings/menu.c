#include "settings.h"
#include "menu.h"

int SETTINGS_ABOUT_buildMenu(struct settings_screen *screen,
	const struct settings_snapshot *snapshot,
	struct settings_item *items, int max_items);
int SETTINGS_ABOUT_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction);
int SETTINGS_BT_buildMenu(struct settings_screen *screen,
	const struct settings_snapshot *snapshot,
	struct settings_item *items, int max_items);
int SETTINGS_BT_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction);
int SETTINGS_BT_activateAux(struct settings_screen *screen,
	const struct settings_item *item);
void SETTINGS_BT_getHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint);
void SETTINGS_BT_getAuxHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint);
int SETTINGS_POWER_buildMenu(struct settings_screen *screen,
	const struct settings_snapshot *snapshot,
	struct settings_item *items, int max_items);
int SETTINGS_POWER_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction);
void SETTINGS_TIME_enter(struct settings_screen *screen,
	const struct settings_snapshot *snapshot);
void SETTINGS_TIME_update(struct settings_screen *screen,
	const struct settings_snapshot *snapshot, int *dirty);
int SETTINGS_TIME_handleInput(struct settings_screen *screen);
void SETTINGS_TIME_draw(struct settings_screen *screen, SDL_Surface *surface);
void SETTINGS_TIME_buildHints(struct settings_screen *screen,
	struct ui_hint_group *top, struct ui_hint_group *bottom);
int SETTINGS_WIFI_buildMenu(struct settings_screen *screen,
	const struct settings_snapshot *snapshot,
	struct settings_item *items, int max_items);
int SETTINGS_WIFI_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction);
int SETTINGS_WIFI_activateAux(struct settings_screen *screen,
	const struct settings_item *item);
void SETTINGS_WIFI_getHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint);
void SETTINGS_WIFI_getAuxHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint);
void SETTINGS_CONTROLS_enter(struct settings_screen *screen,
	const struct settings_snapshot *snapshot);
void SETTINGS_CONTROLS_update(struct settings_screen *screen,
	const struct settings_snapshot *snapshot, int *dirty);
int SETTINGS_CONTROLS_handleInput(struct settings_screen *screen);
void SETTINGS_CONTROLS_draw(struct settings_screen *screen,
	SDL_Surface *surface);
void SETTINGS_CONTROLS_buildHints(struct settings_screen *screen,
	struct ui_hint_group *top, struct ui_hint_group *bottom);

///////////////////////////////////////
static int SETTINGS_ROOT_buildMenu(struct settings_screen *screen,
	const struct settings_snapshot *snapshot,
	struct settings_item *items, int max_items)
{
	struct settings_item *item;
	int count = 0;

	(void)screen;
	(void)snapshot;

	if (!items || max_items < 6)
		return 0;

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_SUBMENU, SETTINGS_ACTION_NONE,
		"Wi-Fi", "");
	item->submenu_id = SETTINGS_MENU_WIFI;

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_SUBMENU, SETTINGS_ACTION_NONE,
		"Bluetooth", "");
	item->submenu_id = SETTINGS_MENU_BT;

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_SUBMENU, SETTINGS_ACTION_NONE,
		"Power", "");
	item->submenu_id = SETTINGS_MENU_POWER;

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_SUBMENU, SETTINGS_ACTION_NONE,
		"Time", "");
	item->submenu_id = SETTINGS_MENU_TIME;

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_SUBMENU, SETTINGS_ACTION_NONE,
		"Controls", "");
	item->submenu_id = SETTINGS_MENU_CONTROLS;

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_SUBMENU, SETTINGS_ACTION_NONE,
		"About", "");
	item->submenu_id = SETTINGS_MENU_ABOUT;

	return count;
}

static int SETTINGS_ROOT_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction)
{
	(void)screen;
	(void)item;
	(void)direction;
	return 0;
}

///////////////////////////////////////
static const struct settings_menu settings_menus[] = {
	{
		.id = SETTINGS_MENU_ROOT,
		.title = "Settings",
		.build = SETTINGS_ROOT_buildMenu,
		.activate = SETTINGS_ROOT_activate,
	},
	{
		.id = SETTINGS_MENU_WIFI,
		.title = "Wi-Fi",
		.build = SETTINGS_WIFI_buildMenu,
		.activate = SETTINGS_WIFI_activate,
		.activate_aux = SETTINGS_WIFI_activateAux,
		.get_hint = SETTINGS_WIFI_getHint,
		.get_aux_hint = SETTINGS_WIFI_getAuxHint,
	},
	{
		.id = SETTINGS_MENU_BT,
		.title = "Bluetooth",
		.build = SETTINGS_BT_buildMenu,
		.activate = SETTINGS_BT_activate,
		.activate_aux = SETTINGS_BT_activateAux,
		.get_hint = SETTINGS_BT_getHint,
		.get_aux_hint = SETTINGS_BT_getAuxHint,
	},
	{
		.id = SETTINGS_MENU_POWER,
		.title = "Power",
		.build = SETTINGS_POWER_buildMenu,
		.activate = SETTINGS_POWER_activate,
	},
	{
		.id = SETTINGS_MENU_TIME,
		.title = "Time",
		.enter = SETTINGS_TIME_enter,
		.update = SETTINGS_TIME_update,
		.handle_input = SETTINGS_TIME_handleInput,
		.draw = SETTINGS_TIME_draw,
		.build_hints = SETTINGS_TIME_buildHints,
	},
	{
		.id = SETTINGS_MENU_CONTROLS,
		.title = "Controls",
		.enter = SETTINGS_CONTROLS_enter,
		.update = SETTINGS_CONTROLS_update,
		.handle_input = SETTINGS_CONTROLS_handleInput,
		.draw = SETTINGS_CONTROLS_draw,
		.build_hints = SETTINGS_CONTROLS_buildHints,
	},
	{
		.id = SETTINGS_MENU_ABOUT,
		.title = "About",
		.build = SETTINGS_ABOUT_buildMenu,
		.activate = SETTINGS_ABOUT_activate,
	},
};

///////////////////////////////////////
const struct settings_menu *SETTINGS_MENU_get(int menu_id)
{
	size_t i;

	for (i = 0; i < sizeof(settings_menus) / sizeof(settings_menus[0]); i++) {
		if (settings_menus[i].id == menu_id)
			return &settings_menus[i];
	}
	return NULL;
}

int SETTINGS_MENU_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction)
{
	const struct settings_menu *menu;
	int menu_id;

	if (!screen || !item || screen->depth < 0 ||
			screen->depth >= SETTINGS_MAX_DEPTH)
		return 0;

	menu_id = screen->menu_stack[screen->depth];
	menu = SETTINGS_MENU_get(menu_id);
	if (!menu || !menu->activate)
		return 0;
	return menu->activate(screen, item, direction);
}

int SETTINGS_MENU_activateAux(struct settings_screen *screen,
	const struct settings_item *item)
{
	const struct settings_menu *menu;
	int menu_id;

	if (!screen || !item || screen->depth < 0 ||
			screen->depth >= SETTINGS_MAX_DEPTH)
		return 0;

	menu_id = screen->menu_stack[screen->depth];
	menu = SETTINGS_MENU_get(menu_id);
	if (!menu || !menu->activate_aux)
		return 0;
	return menu->activate_aux(screen, item);
}

void SETTINGS_MENU_getHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint)
{
	const struct settings_menu *menu;
	int menu_id;

	if (hint)
		memset(hint, 0, sizeof(*hint));
	if (!screen || !item || screen->depth < 0 ||
			screen->depth >= SETTINGS_MAX_DEPTH)
		return;

	menu_id = screen->menu_stack[screen->depth];
	menu = SETTINGS_MENU_get(menu_id);
	if (!menu || !menu->get_hint)
		return;
	menu->get_hint(screen, item, hint);
}

void SETTINGS_MENU_getAuxHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint)
{
	const struct settings_menu *menu;
	int menu_id;

	if (hint)
		memset(hint, 0, sizeof(*hint));
	if (!screen || !item || screen->depth < 0 ||
			screen->depth >= SETTINGS_MAX_DEPTH)
		return;

	menu_id = screen->menu_stack[screen->depth];
	menu = SETTINGS_MENU_get(menu_id);
	if (!menu || !menu->get_aux_hint)
		return;
	menu->get_aux_hint(screen, item, hint);
}
