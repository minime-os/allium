#include <stdio.h>
#include <string.h>

#include "defines.h"
#include "dialog.h"

///////////////////////////////////////
static void dialog_prepare(struct ui_dialog *dialog, int type,
	const char *title, const char *message, const char *detail,
	const char *arg)
{
	if (!dialog)
		return;

	memset(dialog, 0, sizeof(*dialog));
	dialog->open = 1;
	dialog->type = type;
	UI_KEYBOARD_init(&dialog->keyboard);
	snprintf(dialog->title, sizeof(dialog->title), "%s",
		title ? title : "");
	snprintf(dialog->message, sizeof(dialog->message), "%s",
		message ? message : "");
	snprintf(dialog->detail, sizeof(dialog->detail), "%s",
		detail ? detail : "");
	snprintf(dialog->arg, sizeof(dialog->arg), "%s", arg ? arg : "");
}

///////////////////////////////////////
static void dialog_drawPanel(SDL_Surface *screen, SDL_Rect *panel)
{
	SDL_Rect shadow;

	shadow = *panel;
	shadow.x += SCALE1(4);
	shadow.y += SCALE1(4);
	GFX_blitPill(ASSET_DARK_GRAY_PILL, screen, &shadow);
	GFX_blitPill(ASSET_WHITE_PILL, screen, panel);
}

static void dialog_drawHints(const struct ui_dialog *dialog,
	SDL_Surface *screen)
{
	struct ui_hint_group top_group;
	struct ui_hint_group bottom_group;

	if (!dialog || !screen)
		return;

	memset(&top_group, 0, sizeof(top_group));
	memset(&bottom_group, 0, sizeof(bottom_group));

	switch (dialog->type) {
	case UI_DIALOG_TEXT_ENTRY:
	case UI_DIALOG_WIFI_PASSPHRASE:
		top_group.count = 1;
		top_group.primary = 0;
		top_group.align_right = 0;
		top_group.hints[0] = (struct ui_hint){
			.button = UI_BUTTON_ID_START,
			.text = "DONE",
		};
		bottom_group.count = 2;
		bottom_group.primary = 1;
		bottom_group.align_right = 1;
		bottom_group.hints[0] = (struct ui_hint){
			.button = UI_BUTTON_ID_B,
			.label = UI_HINT_LABEL_CANCEL,
		};
		bottom_group.hints[1] = (struct ui_hint){
			.button = UI_BUTTON_ID_A,
			.label = UI_HINT_LABEL_SELECT,
		};
		UI_LIST_drawHintGroup(&top_group, screen);
		UI_LIST_drawHintGroup(&bottom_group, screen);
		break;
	case UI_DIALOG_CONFIRM:
		top_group.count = 2;
		top_group.primary = 0;
		top_group.align_right = 0;
		top_group.hints[0] = (struct ui_hint){
			.button = UI_BUTTON_ID_MENU,
			.label = UI_HINT_LABEL_CANCEL,
		};
		top_group.hints[1] = (struct ui_hint){
			.button = UI_BUTTON_ID_A,
			.label = UI_HINT_LABEL_SELECT,
		};
		bottom_group.count = 2;
		bottom_group.primary = 1;
		bottom_group.align_right = 1;
		bottom_group.hints[0] = (struct ui_hint){
			.button = UI_BUTTON_ID_B,
			.label = UI_HINT_LABEL_BACK,
		};
		bottom_group.hints[1] = (struct ui_hint){
			.button = UI_BUTTON_ID_START,
			.label = UI_HINT_LABEL_OKAY,
		};
		UI_LIST_drawHintGroup(&top_group, screen);
		UI_LIST_drawHintGroup(&bottom_group, screen);
		break;
	case UI_DIALOG_PROGRESS:
		top_group.count = 1;
		top_group.primary = 0;
		top_group.align_right = 0;
		top_group.hints[0] = (struct ui_hint){
			.button = UI_BUTTON_ID_MENU,
			.text = "HIDE",
		};
		UI_LIST_drawHintGroup(&top_group, screen);
		break;
	case UI_DIALOG_ERROR:
	default:
		top_group.count = 2;
		top_group.primary = 0;
		top_group.align_right = 0;
		top_group.hints[0] = (struct ui_hint){
			.button = UI_BUTTON_ID_MENU,
			.text = "CLOSE",
		};
		top_group.hints[1] = (struct ui_hint){
			.button = UI_BUTTON_ID_A,
			.label = UI_HINT_LABEL_OKAY,
		};
		UI_LIST_drawHintGroup(&top_group, screen);
		break;
	}
}

static void dialog_drawText(const char *text, TTF_Font *font_ptr,
	SDL_Color color, SDL_Surface *screen, SDL_Rect *rect, int leading)
{
	if (!text || !text[0] || !font_ptr || !screen || !rect)
		return;
	GFX_blitText(font_ptr, (char *)text, leading, color, screen, rect);
}

static void dialog_drawPromptText(SDL_Surface *screen, TTF_Font *face,
	const char *text, int x, int y, SDL_Color color)
{
	SDL_Surface *surface;

	if (!screen || !face || !text || !text[0])
		return;

	surface = TTF_RenderUTF8_Blended(face, text, color);
	if (!surface)
		return;

	SDL_BlitSurface(surface, NULL, screen, &(SDL_Rect){ x, y });
	SDL_FreeSurface(surface);
}

static void dialog_drawButtons(const struct ui_dialog *dialog,
	SDL_Surface *screen, const SDL_Rect *panel)
{
	SDL_Surface *text;
	SDL_Color left_color;
	SDL_Color right_color;
	SDL_Rect left_rect;
	SDL_Rect right_rect;

	if (!dialog || !screen || !panel || dialog->type != UI_DIALOG_CONFIRM)
		return;

	left_rect.x = panel->x + SCALE1(PADDING);
	left_rect.y = panel->y + panel->h - SCALE1(PILL_SIZE + PADDING);
	left_rect.w = (panel->w - SCALE1(PADDING * 3)) / 2;
	left_rect.h = SCALE1(PILL_SIZE);
	right_rect = left_rect;
	right_rect.x += left_rect.w + SCALE1(PADDING);

	if (dialog->choice == 0) {
		GFX_blitPill(ASSET_DARK_GRAY_PILL, screen, &left_rect);
		GFX_blitPill(ASSET_WHITE_PILL, screen, &right_rect);
		left_color = COLOR_WHITE;
		right_color = COLOR_BLACK;
	} else {
		GFX_blitPill(ASSET_WHITE_PILL, screen, &left_rect);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, screen, &right_rect);
		left_color = COLOR_BLACK;
		right_color = COLOR_WHITE;
	}

	text = TTF_RenderUTF8_Blended(font.large, "Cancel", left_color);
	SDL_BlitSurface(text, NULL, screen, &(SDL_Rect){
		left_rect.x + ((left_rect.w - text->w) / 2),
		left_rect.y + ((left_rect.h - text->h) / 2),
	});
	SDL_FreeSurface(text);

	text = TTF_RenderUTF8_Blended(font.large, "Confirm", right_color);
	SDL_BlitSurface(text, NULL, screen, &(SDL_Rect){
		right_rect.x + ((right_rect.w - text->w) / 2),
		right_rect.y + ((right_rect.h - text->h) / 2),
	});
	SDL_FreeSurface(text);
}

///////////////////////////////////////
void UI_DIALOG_init(struct ui_dialog *dialog)
{
	if (!dialog)
		return;
	memset(dialog, 0, sizeof(*dialog));
}

void UI_DIALOG_close(struct ui_dialog *dialog)
{
	if (!dialog)
		return;
	memset(dialog, 0, sizeof(*dialog));
}

int UI_DIALOG_isOpen(const struct ui_dialog *dialog)
{
	return dialog && dialog->open;
}

void UI_DIALOG_openWifiPassphrase(struct ui_dialog *dialog, const char *ssid)
{
	dialog_prepare(dialog, UI_DIALOG_WIFI_PASSPHRASE,
		"Enter Wi-Fi password", "", ssid, ssid);
	dialog->allow_empty = 0;
}

void UI_DIALOG_openTextEntry(struct ui_dialog *dialog, const char *title,
	const char *detail, int allow_empty)
{
	dialog_prepare(dialog, UI_DIALOG_TEXT_ENTRY, title, "", detail, "");
	dialog->allow_empty = allow_empty;
}

void UI_DIALOG_openProgress(struct ui_dialog *dialog, const char *title,
	const char *message, const char *detail, const char *arg)
{
	dialog_prepare(dialog, UI_DIALOG_PROGRESS, title, message, detail, arg);
}

void UI_DIALOG_openConfirm(struct ui_dialog *dialog, const char *title,
	const char *message, const char *detail, const char *arg)
{
	dialog_prepare(dialog, UI_DIALOG_CONFIRM, title, message, detail, arg);
	dialog->choice = 1;
}

void UI_DIALOG_openError(struct ui_dialog *dialog, const char *title,
	const char *message, const char *detail)
{
	dialog_prepare(dialog, UI_DIALOG_ERROR, title, message, detail, "");
}

int UI_DIALOG_handleInput(struct ui_dialog *dialog)
{
	if (!dialog || !dialog->open)
		return UI_DIALOG_RESULT_NONE;

	switch (dialog->type) {
	case UI_DIALOG_TEXT_ENTRY:
	case UI_DIALOG_WIFI_PASSPHRASE:
		if (PAD_tappedMenu(SDL_GetTicks()))
			return UI_DIALOG_RESULT_CANCEL;
		if (PAD_justPressed(BTN_B))
			return UI_DIALOG_RESULT_CANCEL;
		if (PAD_justPressed(BTN_SELECT)) {
			UI_KEYBOARD_toggleShift(&dialog->keyboard);
			return UI_DIALOG_RESULT_UPDATE;
		}
		if (PAD_justRepeated(BTN_LEFT)) {
			UI_KEYBOARD_move(&dialog->keyboard, -1, 0);
			return UI_DIALOG_RESULT_UPDATE;
		}
		else if (PAD_justRepeated(BTN_RIGHT)) {
			UI_KEYBOARD_move(&dialog->keyboard, 1, 0);
			return UI_DIALOG_RESULT_UPDATE;
		}
		else if (PAD_justRepeated(BTN_UP)) {
			UI_KEYBOARD_move(&dialog->keyboard, 0, -1);
			return UI_DIALOG_RESULT_UPDATE;
		}
		else if (PAD_justRepeated(BTN_DOWN)) {
			UI_KEYBOARD_move(&dialog->keyboard, 0, 1);
			return UI_DIALOG_RESULT_UPDATE;
		}
		else if (PAD_justPressed(BTN_A)) {
			const char *key = UI_KEYBOARD_getSelectedKey(
				&dialog->keyboard);

			if (!strcmp(key, "SHIFT"))
				return UI_KEYBOARD_toggleShift(&dialog->keyboard) ?
					UI_DIALOG_RESULT_UPDATE :
					UI_DIALOG_RESULT_NONE;
			else if (!strcmp(key, "DEL"))
				return UI_KEYBOARD_backspace(&dialog->keyboard) ?
					UI_DIALOG_RESULT_UPDATE :
					UI_DIALOG_RESULT_NONE;
			else if (!strcmp(key, "DONE")) {
				if (dialog->allow_empty ||
						dialog->keyboard.text[0])
					return UI_DIALOG_RESULT_CONFIRM;
			} else {
				return UI_KEYBOARD_insertSelected(&dialog->keyboard) ?
					UI_DIALOG_RESULT_UPDATE :
					UI_DIALOG_RESULT_NONE;
			}
		} else if (PAD_justPressed(BTN_START)) {
			if (dialog->allow_empty || dialog->keyboard.text[0])
				return UI_DIALOG_RESULT_CONFIRM;
		}
		return UI_DIALOG_RESULT_NONE;
	case UI_DIALOG_CONFIRM:
		if (PAD_tappedMenu(SDL_GetTicks()) || PAD_justPressed(BTN_B))
			return UI_DIALOG_RESULT_CANCEL;
		if (PAD_justRepeated(BTN_LEFT) || PAD_justRepeated(BTN_RIGHT) ||
				PAD_justRepeated(BTN_UP) ||
				PAD_justRepeated(BTN_DOWN)) {
			dialog->choice = !dialog->choice;
			return UI_DIALOG_RESULT_UPDATE;
		}
		if (PAD_justPressed(BTN_A) || PAD_justPressed(BTN_START))
			return dialog->choice ? UI_DIALOG_RESULT_CONFIRM :
				UI_DIALOG_RESULT_CANCEL;
		return UI_DIALOG_RESULT_NONE;
	case UI_DIALOG_PROGRESS:
		if (PAD_tappedMenu(SDL_GetTicks()) || PAD_justPressed(BTN_B))
			return UI_DIALOG_RESULT_CLOSE;
		return UI_DIALOG_RESULT_NONE;
	case UI_DIALOG_ERROR:
	default:
		if (PAD_tappedMenu(SDL_GetTicks()) || PAD_justPressed(BTN_A) ||
				PAD_justPressed(BTN_B) ||
				PAD_justPressed(BTN_START))
			return UI_DIALOG_RESULT_CLOSE;
		return UI_DIALOG_RESULT_NONE;
	}
}

void UI_DIALOG_draw(const struct ui_dialog *dialog, SDL_Surface *screen)
{
	SDL_Rect panel;
	SDL_Rect rect;
	char buffer[UI_KEYBOARD_TEXT_MAX];

	if (!dialog || !dialog->open || !screen)
		return;

	if (dialog->type == UI_DIALOG_TEXT_ENTRY ||
			dialog->type == UI_DIALOG_WIFI_PASSPHRASE) {
		SDL_Rect input_rect;
		SDL_Rect keyboard_rect;
		int margin = SCALE1(PADDING);

		GFX_clear(screen);
		dialog_drawPromptText(screen, font.large, dialog->title, margin,
			SCALE1(PADDING), COLOR_WHITE);

		input_rect.x = margin;
		input_rect.y = SCALE1(PADDING + 24);
		input_rect.w = screen->w - (margin * 2);
		input_rect.h = SCALE1(PILL_SIZE);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, screen, &input_rect);
		UI_KEYBOARD_copyDisplay(&dialog->keyboard, buffer, sizeof(buffer));
		dialog_drawPromptText(screen, font.small, buffer,
			input_rect.x + SCALE1(6), input_rect.y + SCALE1(8),
			COLOR_WHITE);

		if (dialog->detail[0]) {
			dialog_drawPromptText(screen, font.tiny, dialog->detail,
				margin, input_rect.y + input_rect.h + SCALE1(8),
				COLOR_LIGHT_TEXT);
		}

		keyboard_rect.x = 0;
		keyboard_rect.y = input_rect.y + SCALE1(30);
		keyboard_rect.w = screen->w;
		keyboard_rect.h = screen->h - keyboard_rect.y -
			SCALE1(PADDING + PILL_SIZE + 18);
		UI_KEYBOARD_draw(&dialog->keyboard, screen, &keyboard_rect);
		dialog_drawHints(dialog, screen);
		return;
	}

	panel.x = SCALE1(PADDING * 2);
	panel.y = SCALE1(PADDING * 3);
	panel.w = screen->w - SCALE1(PADDING * 4);
	panel.h = screen->h - SCALE1(PADDING * 8);

	dialog_drawPanel(screen, &panel);

	rect.x = panel.x + SCALE1(PADDING);
	rect.y = panel.y + SCALE1(PADDING);
	rect.w = panel.w - SCALE1(PADDING * 2);
	rect.h = SCALE1(PILL_SIZE);
	dialog_drawText(dialog->title, font.large, COLOR_BLACK, screen, &rect,
		SCALE1(20));

	rect.y += SCALE1(PILL_SIZE + PADDING);
	rect.h = SCALE1(PILL_SIZE);
	dialog_drawText(dialog->message, font.small, COLOR_DARK_TEXT, screen,
		&rect, SCALE1(16));

	if (dialog->type != UI_DIALOG_TEXT_ENTRY &&
			dialog->type != UI_DIALOG_WIFI_PASSPHRASE) {
		rect.y = panel.y + SCALE1(PILL_SIZE * 4);
		rect.h = panel.h - SCALE1(PILL_SIZE * 6);
		dialog_drawText(dialog->detail, font.small, COLOR_BLACK, screen,
			&rect, SCALE1(18));
		dialog_drawButtons(dialog, screen, &panel);
	}

	dialog_drawHints(dialog, screen);
}
