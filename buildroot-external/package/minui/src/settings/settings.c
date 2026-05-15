#include <stdio.h>
#include <string.h>

#include <msettings.h>

#include "settings.h"
#include "jobs.h"
#include "menu.h"

///////////////////////////////////////
static struct settings_screen settings;

///////////////////////////////////////
static void settings_closeDialog(void)
{
	UI_DIALOG_close(&settings.dialog);
}

static void settings_syncPromptDialog(void)
{
	const struct settings_prompt *prompt = &settings.snapshot.prompt;

	if (prompt->type == SETTINGS_PROMPT_NONE) {
		if (settings.dialog.type == UI_DIALOG_PROGRESS ||
				settings.dialog.type == UI_DIALOG_CONFIRM ||
				settings.dialog.type == UI_DIALOG_ERROR)
			settings_closeDialog();
		return;
	}

	if (prompt->type == SETTINGS_PROMPT_BT_PROGRESS) {
		if (settings.dialog.type == UI_DIALOG_PROGRESS &&
				!strcmp(settings.dialog.title, prompt->title) &&
				!strcmp(settings.dialog.detail, prompt->detail))
			return;
		UI_DIALOG_openProgress(&settings.dialog, prompt->title,
			prompt->message, prompt->detail, prompt->arg);
		return;
	}

	if (prompt->type == SETTINGS_PROMPT_BT_CONFIRM) {
		if (settings.dialog.type == UI_DIALOG_CONFIRM &&
				!strcmp(settings.dialog.detail, prompt->detail) &&
				!strcmp(settings.dialog.arg, prompt->arg))
			return;
		UI_DIALOG_openConfirm(&settings.dialog, prompt->title,
			prompt->message, prompt->detail, prompt->arg);
		return;
	}

	if (prompt->type == SETTINGS_PROMPT_ERROR) {
		if (settings.dialog.type == UI_DIALOG_ERROR &&
				!strcmp(settings.dialog.message, prompt->message) &&
				!strcmp(settings.dialog.detail, prompt->detail))
			return;
		UI_DIALOG_openError(&settings.dialog, prompt->title,
			prompt->message, prompt->detail);
	}
}

static int settings_handleDialogResult(int result)
{
	int rc;

	if (result == UI_DIALOG_RESULT_NONE || !UI_DIALOG_isOpen(&settings.dialog))
		return 0;
	if (result == UI_DIALOG_RESULT_UPDATE)
		return 1;

	switch (settings.dialog.type) {
	case UI_DIALOG_TEXT_ENTRY:
		settings_closeDialog();
		return 1;
	case UI_DIALOG_WIFI_PASSPHRASE:
		if (result == UI_DIALOG_RESULT_CONFIRM) {
			if (!settings.dialog.allow_empty &&
					!settings.dialog.keyboard.text[0]) {
				SETTINGS_setNotice(&settings, "Passphrase required");
				return 1;
			}
			rc = SETTINGS_JOBS_enqueueWifiConnect(settings.dialog.arg,
				settings.dialog.keyboard.text);
			if (rc != 0)
				SETTINGS_setNotice(&settings,
					"Wi-Fi connect queued failed");
		}
		settings_closeDialog();
		return 1;
	case UI_DIALOG_CONFIRM:
		if (result == UI_DIALOG_RESULT_CONFIRM) {
			rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_BT_DEVICE_CONFIRM, 0,
				settings.dialog.arg);
			if (rc != 0)
				SETTINGS_setNotice(&settings,
					"Bluetooth confirm queued failed");
		}
		SETTINGS_JOBS_clearPrompt();
		settings_closeDialog();
		return 1;
	case UI_DIALOG_PROGRESS:
		if (result == UI_DIALOG_RESULT_CLOSE) {
			settings_closeDialog();
			return 1;
		}
		return 0;
	case UI_DIALOG_ERROR:
		SETTINGS_JOBS_clearPrompt();
		settings_closeDialog();
		return 1;
	default:
		settings_closeDialog();
		return 1;
	}
}

///////////////////////////////////////
static int settings_currentMenuId(void)
{
	if (settings.depth < 0 || settings.depth >= SETTINGS_MAX_DEPTH)
		return SETTINGS_MENU_ROOT;
	return settings.menu_stack[settings.depth];
}

static const struct settings_menu *settings_currentMenu(void)
{
	return SETTINGS_MENU_get(settings_currentMenuId());
}

static int settings_menuIsCustom(const struct settings_menu *menu)
{
	return menu && menu->draw != NULL;
}

static void settings_leaveCurrentMenu(void)
{
	const struct settings_menu *menu = settings_currentMenu();

	if (!menu || !menu->leave)
		return;
	menu->leave(&settings);
}

static void settings_enterCurrentMenu(void)
{
	const struct settings_menu *menu = settings_currentMenu();

	if (!menu || !menu->enter)
		return;
	menu->enter(&settings, &settings.snapshot);
}

static void settings_clampSelection(void)
{
	int depth = settings.depth;
	int max_start;

	if (depth < 0 || depth >= SETTINGS_MAX_DEPTH)
		return;
	if (settings.item_count <= 0) {
		settings.selected[depth] = 0;
		settings.start[depth] = 0;
		return;
	}
	if (settings.selected[depth] >= settings.item_count)
		settings.selected[depth] = settings.item_count - 1;
	if (settings.selected[depth] < 0)
		settings.selected[depth] = 0;
	if (settings.start[depth] > settings.selected[depth])
		settings.start[depth] = settings.selected[depth];
	if (settings.selected[depth] >=
			settings.start[depth] + SETTINGS_ROW_COUNT)
		settings.start[depth] = settings.selected[depth] -
			SETTINGS_ROW_COUNT + 1;
	max_start = settings.item_count - SETTINGS_ROW_COUNT;
	if (max_start < 0)
		max_start = 0;
	if (settings.start[depth] > max_start)
		settings.start[depth] = max_start;
	if (settings.start[depth] < 0)
		settings.start[depth] = 0;
}

static void settings_rebuild(void)
{
	const struct settings_menu *menu = settings_currentMenu();

	settings.item_count = 0;
	if (!menu) {
		SETTINGS_JOBS_setActiveMenu(SETTINGS_MENU_NONE);
		return;
	}

	if (!settings_menuIsCustom(menu) && menu->build)
		settings.item_count = menu->build(&settings, &settings.snapshot,
			settings.items, SETTINGS_MAX_ITEMS);
	settings_clampSelection();
	SETTINGS_JOBS_setActiveMenu(menu->id);
}

static void settings_openMenu(int depth, int menu_id, int reset_selection)
{
	settings_leaveCurrentMenu();
	settings.depth = depth;
	settings.menu_stack[depth] = menu_id;
	if (reset_selection) {
		settings.selected[depth] = 0;
		settings.start[depth] = 0;
	}
	settings_enterCurrentMenu();
	settings_rebuild();
}

static void settings_selectDelta(int delta)
{
	int depth = settings.depth;
	int next;

	if (settings.item_count <= 0)
		return;

	next = settings.selected[depth] + delta;
	if (next < 0)
		next = settings.item_count - 1;
	else if (next >= settings.item_count)
		next = 0;
	settings.selected[depth] = next;
	settings_clampSelection();
}

static int settings_activateItem(int direction)
{
	struct settings_item *item;

	if (settings.item_count <= 0)
		return 0;

	item = &settings.items[settings.selected[settings.depth]];
	if (!item->enabled || item->type == SETTINGS_ITEM_READONLY)
		return 0;
	if (item->type == SETTINGS_ITEM_SUBMENU) {
		if (settings.depth + 1 >= SETTINGS_MAX_DEPTH)
			return 0;
		settings.selected[settings.depth + 1] = 0;
		settings.start[settings.depth + 1] = 0;
		settings_openMenu(settings.depth + 1, item->submenu_id, 0);
		return 1;
	}
	return SETTINGS_MENU_activate(&settings, item, direction);
}

///////////////////////////////////////
static void settings_back(void)
{
	if (settings.depth <= 0)
		return;
	settings_openMenu(settings.depth - 1,
		settings.menu_stack[settings.depth - 1], 0);
}

static int settings_hintIsSet(const struct ui_hint *hint)
{
	return hint &&
		((hint->text && hint->text[0]) ||
		hint->label != UI_HINT_LABEL_NONE);
}

static void settings_setHint(struct ui_hint *hint, int button, int label,
	const char *text)
{
	if (!hint)
		return;
	memset(hint, 0, sizeof(*hint));
	hint->button = button;
	hint->label = label;
	hint->text = text;
}

static void settings_primaryHint(const struct settings_item *item,
	struct ui_hint *hint)
{
	if (!hint)
		return;
	memset(hint, 0, sizeof(*hint));
	if (!item)
		return;

	SETTINGS_MENU_getHint(&settings, item, hint);
	if (settings_hintIsSet(hint))
		return;

	switch (item->type) {
	case SETTINGS_ITEM_SUBMENU:
		hint->label = UI_HINT_LABEL_OPEN;
		return;
	case SETTINGS_ITEM_ACTION:
		hint->label = UI_HINT_LABEL_SELECT;
		return;
	case SETTINGS_ITEM_TOGGLE:
		hint->label = UI_HINT_LABEL_TOGGLE;
		return;
	case SETTINGS_ITEM_ENUM:
		hint->label = UI_HINT_LABEL_CHANGE;
		return;
	case SETTINGS_ITEM_READONLY:
	default:
		return;
	}
}

static void settings_secondaryHint(const struct settings_item *item,
	struct ui_hint *hint)
{
	if (!hint)
		return;
	memset(hint, 0, sizeof(*hint));
	if (!item)
		return;
	SETTINGS_MENU_getAuxHint(&settings, item, hint);
}

static const char *settings_noticeMessage(void)
{
	int menu_id = settings_currentMenuId();

	if ((menu_id == SETTINGS_MENU_WIFI || menu_id == SETTINGS_MENU_BT) &&
			(!settings.notice[0] ||
			SDL_GetTicks() >= settings.notice_until))
		return "";

	if (settings.notice[0] && SDL_GetTicks() < settings.notice_until)
		return settings.notice;
	return "";
}

static void settings_buildRows(struct ui_list_view *view)
{
	int i;

	if (!view)
		return;

	for (i = 0; i < settings.item_count; i++) {
		struct ui_row *row = &settings.view_items[i];
		const struct settings_item *item = &settings.items[i];

		UI_LIST_initRow(row, UI_ROW_KIND_SETTINGS, item->label, "");
		row->enabled = item->enabled;
		row->badge = item->badge;
	}
}

static void settings_buildHints(struct ui_list_view *view)
{
	const struct settings_item *item = NULL;
	struct ui_hint action_hint;
	struct ui_hint aux_hint;

	if (!view)
		return;
	memset(&action_hint, 0, sizeof(action_hint));
	memset(&aux_hint, 0, sizeof(aux_hint));
	if (settings.item_count > 0)
		item = &settings.items[settings.selected[settings.depth]];
	if (item) {
		settings_primaryHint(item, &action_hint);
		settings_secondaryHint(item, &aux_hint);
	}

	memset(&view->top_group, 0, sizeof(view->top_group));
	memset(&view->bottom_group, 0, sizeof(view->bottom_group));

	if (settings.depth > 0 && settings_hintIsSet(&aux_hint)) {
		view->top_group.count = 1;
		view->top_group.primary = 0;
		view->top_group.align_right = 0;
		view->top_group.hints[0] = aux_hint;
		view->top_group.hints[0].button = UI_BUTTON_ID_X;
	}

	view->bottom_group.primary = 1;
	view->bottom_group.align_right = 1;
	view->bottom_group.count = 1;
	settings_setHint(&view->bottom_group.hints[0], UI_BUTTON_ID_B,
		UI_HINT_LABEL_BACK, NULL);
	if (settings_hintIsSet(&action_hint) &&
			view->bottom_group.count < UI_LIST_MAX_HINTS) {
		view->bottom_group.hints[1] = action_hint;
		view->bottom_group.hints[1].button = UI_BUTTON_ID_A;
		view->bottom_group.count = 2;
	}
}

static void settings_drawCustomHints(SDL_Surface *screen, int show_setting)
{
	const struct settings_menu *menu = settings_currentMenu();
	struct ui_hint_group top;
	struct ui_hint_group bottom;

	memset(&top, 0, sizeof(top));
	memset(&bottom, 0, sizeof(bottom));
	if (menu && menu->build_hints)
		menu->build_hints(&settings, &top, &bottom);
	if (!bottom.count) {
		bottom.count = 1;
		bottom.primary = 0;
		bottom.align_right = 1;
		settings_setHint(&bottom.hints[0], UI_BUTTON_ID_B,
			UI_HINT_LABEL_BACK, NULL);
	}

	if (show_setting && !GetHDMI())
		GFX_blitHardwareHints(screen, show_setting);
	else
		UI_LIST_drawHintGroup(&top, screen);
	UI_LIST_drawHintGroup(&bottom, screen);
}

///////////////////////////////////////
void SETTINGS_init(void)
{
	memset(&settings, 0, sizeof(settings));
	settings.menu_stack[0] = SETTINGS_MENU_ROOT;
	UI_DIALOG_init(&settings.dialog);
	SETTINGS_JOBS_init();
	SETTINGS_JOBS_copySnapshot(&settings.snapshot);
	settings.seen_generation = settings.snapshot.generation;
	settings.last_clock_tick = time(NULL);
}

void SETTINGS_quit(void)
{
	SETTINGS_JOBS_quit();
}

void SETTINGS_open(void)
{
	settings.open = 1;
	settings.depth = 0;
	settings.menu_stack[0] = SETTINGS_MENU_ROOT;
	settings.selected[0] = 0;
	settings.start[0] = 0;
	settings.notice[0] = '\0';
	settings_closeDialog();
	SETTINGS_JOBS_copySnapshot(&settings.snapshot);
	settings.seen_generation = settings.snapshot.generation;
	settings.last_clock_tick = time(NULL);
	settings_syncPromptDialog();
	settings_enterCurrentMenu();
	settings_rebuild();
}

void SETTINGS_close(void)
{
	if (settings.open)
		settings_leaveCurrentMenu();
	settings.open = 0;
	settings_closeDialog();
	SETTINGS_JOBS_setActiveMenu(SETTINGS_MENU_NONE);
}

int SETTINGS_isOpen(void)
{
	return settings.open;
}

int SETTINGS_handleMenuToggle(int *dirty)
{
	if (!settings.open || !UI_DIALOG_isOpen(&settings.dialog))
		return 0;

	if (settings.dialog.type == UI_DIALOG_CONFIRM ||
			settings.dialog.type == UI_DIALOG_ERROR)
		SETTINGS_JOBS_clearPrompt();
	settings_closeDialog();
	if (dirty)
		*dirty = 1;
	return 1;
}

void SETTINGS_update(int *dirty)
{
	const struct settings_menu *menu;
	time_t now;
	uint32_t generation;

	if (!settings.open)
		return;

	generation = SETTINGS_JOBS_copySnapshot(&settings.snapshot);
	if (generation != settings.seen_generation) {
		settings.seen_generation = generation;
		settings_syncPromptDialog();
		menu = settings_currentMenu();
		if (!settings_menuIsCustom(menu))
			settings_rebuild();
		if (dirty)
			*dirty = 1;
	}

	menu = settings_currentMenu();
	if (menu && menu->update)
		menu->update(&settings, &settings.snapshot, dirty);

	now = time(NULL);
	if (now != settings.last_clock_tick) {
		settings.last_clock_tick = now;
		if (!settings_menuIsCustom(menu) &&
				settings_currentMenuId() == SETTINGS_MENU_TIME) {
			settings_rebuild();
			if (dirty)
				*dirty = 1;
		}
	}
}

void SETTINGS_handleInput(int *dirty, int *quit)
{
	const struct settings_menu *menu;
	int changed = 0;
	int input_result = SETTINGS_CUSTOM_INPUT_NONE;

	(void)quit;

	if (!settings.open)
		return;

	if (UI_DIALOG_isOpen(&settings.dialog)) {
		changed = settings_handleDialogResult(
			UI_DIALOG_handleInput(&settings.dialog));
		if (changed) {
			settings_rebuild();
			if (dirty)
				*dirty = 1;
		}
		return;
	}

	menu = settings_currentMenu();
	if (settings_menuIsCustom(menu)) {
		if (menu && menu->handle_input)
			input_result = menu->handle_input(&settings);

		if (input_result == SETTINGS_CUSTOM_INPUT_BACK) {
			settings_back();
			changed = 1;
		} else if (input_result == SETTINGS_CUSTOM_INPUT_DIRTY) {
			changed = 1;
		} else if (input_result == SETTINGS_CUSTOM_INPUT_NONE &&
				PAD_justPressed(BTN_B)) {
			settings_back();
			changed = 1;
		}
		if (changed && dirty)
			*dirty = 1;
		return;
	}

	if (PAD_justPressed(BTN_B)) {
		if (settings.depth > 0) {
			settings_back();
			changed = 1;
		} else {
			SETTINGS_close();
			changed = 1;
		}
	} else if (PAD_justRepeated(BTN_UP)) {
		settings_selectDelta(-1);
		changed = 1;
	} else if (PAD_justRepeated(BTN_DOWN)) {
		settings_selectDelta(1);
		changed = 1;
	} else if (PAD_justRepeated(BTN_LEFT)) {
		changed = settings_activateItem(-1);
	} else if (PAD_justRepeated(BTN_RIGHT)) {
		changed = settings_activateItem(1);
	} else if (PAD_justPressed(BTN_X)) {
		if (settings.item_count > 0)
			changed = SETTINGS_MENU_activateAux(&settings,
				&settings.items[settings.selected[settings.depth]]);
	} else if (PAD_justPressed(BTN_A)) {
		changed = settings_activateItem(0);
	}

	if (changed) {
		if (settings.open && !settings_menuIsCustom(settings_currentMenu()))
			settings_rebuild();
		if (dirty)
			*dirty = 1;
	}
}

void SETTINGS_buildView(struct ui_list_view *view)
{
	if (!view || !settings.open)
		return;
	UI_LIST_initView(view);
	settings_buildRows(view);
	view->rows = settings.view_items;
	view->count = settings.item_count;
	view->selected = settings.selected[settings.depth];
	view->start = settings.start[settings.depth];
	view->visible_rows = SETTINGS_ROW_COUNT;
	view->message = settings_noticeMessage();
	view->empty_message = "";
	settings_buildHints(view);
}

void SETTINGS_draw(SDL_Surface *screen, int show_setting)
{
	const struct settings_menu *menu;
	struct ui_list_view view;
	const struct ui_dialog *dialog;

	if (!settings.open || !screen)
		return;

	menu = settings_currentMenu();
	if (settings_menuIsCustom(menu)) {
		menu->draw(&settings, screen);
		dialog = SETTINGS_getDialog();
		if (dialog && UI_DIALOG_isOpen(dialog))
			UI_DIALOG_draw(dialog, screen);
		else
			settings_drawCustomHints(screen, show_setting);
		return;
	}

	UI_LIST_initView(&view);
	SETTINGS_buildView(&view);
	UI_LIST_drawRows(&view, screen);
	UI_LIST_drawMessage(&view, screen);
	dialog = SETTINGS_getDialog();
	if (dialog && UI_DIALOG_isOpen(dialog))
		UI_DIALOG_draw(dialog, screen);
	else
		UI_LIST_drawHints(&view, screen, show_setting);
}

const struct ui_dialog *SETTINGS_getDialog(void)
{
	return &settings.dialog;
}
