#include <stdio.h>
#include <string.h>

#include <msettings.h>

#include "defines.h"
#include "badge.h"
#include "list.h"

///////////////////////////////////////
static const char *ui_buttonName(int button_id)
{
	switch (button_id) {
	case UI_BUTTON_ID_A:
		return "A";
	case UI_BUTTON_ID_B:
		return "B";
	case UI_BUTTON_ID_X:
		return "X";
	case UI_BUTTON_ID_MENU:
		return "MENU";
	case UI_BUTTON_ID_POWER:
		return "POWER";
	case UI_BUTTON_ID_SELECT:
		return "SELECT";
	case UI_BUTTON_ID_START:
		return "START";
	default:
		return "";
	}
}

static const char *ui_hintName(const struct ui_hint *hint)
{
	if (!hint)
		return "";
	if (hint->text && hint->text[0])
		return hint->text;

	switch (hint->label) {
	case UI_HINT_LABEL_BACK:
		return "BACK";
	case UI_HINT_LABEL_OKAY:
		return "OKAY";
	case UI_HINT_LABEL_CANCEL:
		return "CANCEL";
	case UI_HINT_LABEL_OPEN:
		return "OPEN";
	case UI_HINT_LABEL_SELECT:
		return "SELECT";
	case UI_HINT_LABEL_CHANGE:
		return "CHANGE";
	case UI_HINT_LABEL_TOGGLE:
		return "TOGGLE";
	case UI_HINT_LABEL_SETTINGS:
		return "SETTINGS";
	default:
		return "";
	}
}

void UI_LIST_drawHintGroup(const struct ui_hint_group *group,
	SDL_Surface *screen)
{
	char *hints[UI_LIST_MAX_HINTS * 2 + 1];
	int i;
	int n = 0;

	if (!group || !screen || group->count <= 0)
		return;

	for (i = 0; i < group->count && i < UI_LIST_MAX_HINTS; i++) {
		const char *button;
		const char *action;

		button = ui_buttonName(group->hints[i].button);
		action = ui_hintName(&group->hints[i]);
		if (!button[0] || !action[0])
			continue;
		hints[n++] = (char *)button;
		hints[n++] = (char *)action;
	}
	if (!n)
		return;
	hints[n] = NULL;
	GFX_blitButtonGroup(hints, group->primary, screen, group->align_right);
}

///////////////////////////////////////
static void ui_list_drawBrowserRow(const struct ui_row *row,
	const struct ui_list_view *view, SDL_Surface *screen, int row_index,
	int visible_index)
{
	SDL_Color text_color = COLOR_WHITE;
	char label_display[256];
	char subtitle_display[256];
	const char *label;
	const char *subtitle;
	int row_y;
	int right_edge;
	int available_width;
	int text_width;
	int max_width;
	int selected;
	SDL_Surface *text;

	if (!row || !view || !screen)
		return;

	selected = (row_index == view->selected);
	row_y = SCALE1(PADDING + (visible_index * PILL_SIZE));
	right_edge = row->clip_right > 0 ? row->clip_right : screen->w;
	available_width = right_edge - SCALE1(PADDING * 2);
	if (visible_index == 0)
		available_width -= view->header_width;
	if (available_width < SCALE1(BUTTON_SIZE))
		available_width = SCALE1(BUTTON_SIZE);

	label = row->label;
	subtitle = row->subtitle[0] ? row->subtitle : NULL;
	text_width = GFX_truncateText(font.large, subtitle ? subtitle : label,
		label_display, available_width, SCALE1(BUTTON_PADDING * 2));
	max_width = MIN(available_width, text_width);

	if (selected) {
		GFX_blitPill(ASSET_WHITE_PILL, screen, &(SDL_Rect){
			SCALE1(PADDING),
			row_y,
			max_width,
			SCALE1(PILL_SIZE)
		});
		text_color = COLOR_BLACK;
	} else if (subtitle) {
		GFX_truncateText(font.large, subtitle, subtitle_display,
			available_width, SCALE1(BUTTON_PADDING * 2));
		text = TTF_RenderUTF8_Blended(font.large, subtitle_display,
			COLOR_DARK_TEXT);
		if (text) {
			SDL_BlitSurface(text, &(SDL_Rect){
				0,
				0,
				max_width - SCALE1(BUTTON_PADDING * 2),
				text->h
			}, screen, &(SDL_Rect){
				SCALE1(PADDING + BUTTON_PADDING),
				row_y + SCALE1(4)
			});
			SDL_FreeSurface(text);
		}
		GFX_truncateText(font.large, label, label_display, available_width,
			SCALE1(BUTTON_PADDING * 2));
	}

	text = TTF_RenderUTF8_Blended(font.large, label_display, text_color);
	if (!text)
		return;

	SDL_BlitSurface(text, &(SDL_Rect){
		0,
		0,
		max_width - SCALE1(BUTTON_PADDING * 2),
		text->h
	}, screen, &(SDL_Rect){
		SCALE1(PADDING + BUTTON_PADDING),
		row_y + SCALE1(4)
	});
	SDL_FreeSurface(text);
}

static void ui_list_drawSettingsRow(const struct ui_row *row,
	const struct ui_list_view *view, SDL_Surface *screen, int row_index,
	int visible_index)
{
	SDL_Color text_color;
	char label_display[64];
	SDL_Rect row_rect;
	int selected;
	int badge_width;
	int label_width;
	SDL_Surface *label_text;

	if (!row || !screen)
		return;

	selected = (row_index == view->selected);
	row_rect.x = SCALE1(PADDING);
	row_rect.y = SCALE1(PADDING + (visible_index * PILL_SIZE));
	row_rect.w = screen->w - SCALE1(PADDING * 2);
	row_rect.h = SCALE1(PILL_SIZE);
	text_color = row->enabled ? COLOR_WHITE : COLOR_DARK_TEXT;
	if (selected) {
		GFX_blitPill(ASSET_WHITE_PILL, screen, &row_rect);
		text_color = COLOR_BLACK;
	}

	badge_width = UI_BADGE_measure(&row->badge);
	label_width = row_rect.w - SCALE1(BUTTON_PADDING * 2) - badge_width;
	if (label_width < SCALE1(BUTTON_SIZE))
		label_width = SCALE1(BUTTON_SIZE);

	GFX_truncateText(font.large, row->label, label_display, label_width, 0);
	label_text = TTF_RenderUTF8_Blended(font.large, label_display, text_color);
	if (label_text) {
		SDL_BlitSurface(label_text, NULL, screen, &(SDL_Rect){
			row_rect.x + SCALE1(BUTTON_PADDING),
			row_rect.y + SCALE1(4),
		});
		SDL_FreeSurface(label_text);
	}

	UI_BADGE_draw(&row->badge, row->enabled, selected, screen, &row_rect);
}

///////////////////////////////////////
void UI_LIST_initView(struct ui_list_view *view)
{
	if (!view)
		return;
	memset(view, 0, sizeof(*view));
}

void UI_LIST_initRow(struct ui_row *row, int kind, const char *label,
	const char *subtitle)
{
	if (!row)
		return;
	memset(row, 0, sizeof(*row));
	row->kind = kind;
	row->enabled = 1;
	snprintf(row->label, sizeof(row->label), "%s", label ? label : "");
	snprintf(row->subtitle, sizeof(row->subtitle), "%s",
		subtitle ? subtitle : "");
}

void UI_LIST_drawRows(const struct ui_list_view *view, SDL_Surface *screen)
{
	int row_count;
	int visible;

	if (!view || !screen)
		return;
	if (view->count <= 0) {
		if (view->empty_message)
			GFX_blitMessage(font.large, (char *)view->empty_message,
				screen, &(SDL_Rect){ 0, 0, screen->w, screen->h });
		return;
	}

	row_count = view->visible_rows;
	if (row_count <= 0)
		row_count = view->count;

	for (visible = 0; visible < row_count; visible++) {
		int index;
		const struct ui_row *row;

		index = view->start + visible;
		if (index >= view->count)
			break;

		row = &view->rows[index];
		if (row->kind == UI_ROW_KIND_BROWSER)
			ui_list_drawBrowserRow(row, view, screen, index, visible);
		else
			ui_list_drawSettingsRow(row, view, screen, index, visible);
	}
}

void UI_LIST_drawMessage(const struct ui_list_view *view, SDL_Surface *screen)
{
	SDL_Rect rect;

	if (!view || !screen || !view->message || !view->message[0])
		return;

	rect.x = SCALE1(PADDING);
	rect.y = SCALE1(PADDING + (view->visible_rows * PILL_SIZE));
	rect.w = screen->w - SCALE1(PADDING * 2);
	rect.h = SCALE1(PILL_SIZE * 2);
	GFX_blitText(font.tiny, (char *)view->message, SCALE1(12),
		COLOR_LIGHT_TEXT, screen, &rect);
}

void UI_LIST_drawHints(const struct ui_list_view *view, SDL_Surface *screen,
	int show_setting)
{
	if (!view || !screen)
		return;

	if (show_setting && !GetHDMI())
		GFX_blitHardwareHints(screen, show_setting);
	else
		UI_LIST_drawHintGroup(&view->top_group, screen);

	UI_LIST_drawHintGroup(&view->bottom_group, screen);
}
