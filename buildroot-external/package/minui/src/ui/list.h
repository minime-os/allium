#ifndef __UI_LIST_H__
#define __UI_LIST_H__

#include "defines.h"
#include "api.h"

#define UI_LIST_MAX_HINTS 2
#define UI_BADGE_TEXT_SIZE 32

///////////////////////////////////////
enum ui_row_kind {
	UI_ROW_KIND_SETTINGS,
	UI_ROW_KIND_BROWSER,
};

enum ui_hint_label {
	UI_HINT_LABEL_NONE,
	UI_HINT_LABEL_BACK,
	UI_HINT_LABEL_OKAY,
	UI_HINT_LABEL_CANCEL,
	UI_HINT_LABEL_OPEN,
	UI_HINT_LABEL_SELECT,
	UI_HINT_LABEL_CHANGE,
	UI_HINT_LABEL_TOGGLE,
	UI_HINT_LABEL_SETTINGS,
};

enum ui_button_id {
	UI_BUTTON_ID_NONE,
	UI_BUTTON_ID_A,
	UI_BUTTON_ID_B,
	UI_BUTTON_ID_X,
	UI_BUTTON_ID_MENU,
	UI_BUTTON_ID_POWER,
	UI_BUTTON_ID_SELECT,
	UI_BUTTON_ID_START,
};

///////////////////////////////////////
struct ui_badge {
	unsigned int flags;
	char text[UI_BADGE_TEXT_SIZE];
};

struct ui_row {
	int kind;
	int enabled;
	int clip_right;
	char label[64];
	char subtitle[64];
	struct ui_badge badge;
};

struct ui_hint {
	int button;
	int label;
	const char *text;
};

struct ui_hint_group {
	int count;
	int primary;
	int align_right;
	struct ui_hint hints[UI_LIST_MAX_HINTS];
};

struct ui_list_view {
	struct ui_row *rows;
	int count;
	int selected;
	int start;
	int visible_rows;
	int header_width;
	const char *empty_message;
	const char *message;
	struct ui_hint_group top_group;
	struct ui_hint_group bottom_group;
};

///////////////////////////////////////
void UI_LIST_initView(struct ui_list_view *view);
void UI_LIST_initRow(struct ui_row *row, int kind, const char *label,
	const char *subtitle);
void UI_LIST_drawHintGroup(const struct ui_hint_group *group,
	SDL_Surface *screen);
void UI_LIST_drawRows(const struct ui_list_view *view, SDL_Surface *screen);
void UI_LIST_drawMessage(const struct ui_list_view *view, SDL_Surface *screen);
void UI_LIST_drawHints(const struct ui_list_view *view, SDL_Surface *screen,
	int show_setting);

#endif
