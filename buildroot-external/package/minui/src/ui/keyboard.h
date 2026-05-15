#ifndef __UI_KEYBOARD_H__
#define __UI_KEYBOARD_H__

#include <stddef.h>

#include "api.h"

#define UI_KEYBOARD_ROWS 5
#define UI_KEYBOARD_TEXT_MAX 128

///////////////////////////////////////
struct ui_keyboard {
	int row;
	int col;
	int shifted;
	char text[UI_KEYBOARD_TEXT_MAX];
};

///////////////////////////////////////
void UI_KEYBOARD_init(struct ui_keyboard *keyboard);
void UI_KEYBOARD_reset(struct ui_keyboard *keyboard);
int UI_KEYBOARD_move(struct ui_keyboard *keyboard, int dx, int dy);
int UI_KEYBOARD_insertSelected(struct ui_keyboard *keyboard);
int UI_KEYBOARD_backspace(struct ui_keyboard *keyboard);
int UI_KEYBOARD_toggleShift(struct ui_keyboard *keyboard);
void UI_KEYBOARD_copyDisplay(const struct ui_keyboard *keyboard, char *dst,
	size_t dst_size);
const char *UI_KEYBOARD_getSelectedKey(const struct ui_keyboard *keyboard);
void UI_KEYBOARD_draw(const struct ui_keyboard *keyboard, SDL_Surface *screen,
	const SDL_Rect *rect);

#endif
