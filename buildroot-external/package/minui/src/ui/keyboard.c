#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "defines.h"
#include "keyboard.h"

struct keyboard_row {
	const char **keys;
	int count;
};

struct keyboard_layout {
	struct keyboard_row rows[UI_KEYBOARD_ROWS];
};

static const char *keyboard_row0_lower[] = {
	"`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=",
};
static const char *keyboard_row1_lower[] = {
	"q", "w", "e", "r", "t", "y", "u", "i", "o", "p", "[", "]", "\\",
};
static const char *keyboard_row2_lower[] = {
	"a", "s", "d", "f", "g", "h", "j", "k", "l", ";", "'",
};
static const char *keyboard_row3_lower[] = {
	"z", "x", "c", "v", "b", "n", "m", ",", ".", "/",
};
static const char *keyboard_row0_upper[] = {
	"~", "!", "@", "#", "$", "%", "^", "&", "*", "(", ")", "_", "+",
};
static const char *keyboard_row1_upper[] = {
	"Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "{", "}", "|",
};
static const char *keyboard_row2_upper[] = {
	"A", "S", "D", "F", "G", "H", "J", "K", "L", ":", "\"",
};
static const char *keyboard_row3_upper[] = {
	"Z", "X", "C", "V", "B", "N", "M", "<", ">", "?",
};
static const char *keyboard_row4_controls[] = {
	"SHIFT", "SPACE", "DEL", "DONE",
};

static const struct keyboard_layout keyboard_layout_lower = {
	.rows = {
		{ keyboard_row0_lower,
			sizeof(keyboard_row0_lower) / sizeof(keyboard_row0_lower[0]) },
		{ keyboard_row1_lower,
			sizeof(keyboard_row1_lower) / sizeof(keyboard_row1_lower[0]) },
		{ keyboard_row2_lower,
			sizeof(keyboard_row2_lower) / sizeof(keyboard_row2_lower[0]) },
		{ keyboard_row3_lower,
			sizeof(keyboard_row3_lower) / sizeof(keyboard_row3_lower[0]) },
		{ keyboard_row4_controls,
			sizeof(keyboard_row4_controls) / sizeof(keyboard_row4_controls[0]) },
	},
};

static const struct keyboard_layout keyboard_layout_upper = {
	.rows = {
		{ keyboard_row0_upper,
			sizeof(keyboard_row0_upper) / sizeof(keyboard_row0_upper[0]) },
		{ keyboard_row1_upper,
			sizeof(keyboard_row1_upper) / sizeof(keyboard_row1_upper[0]) },
		{ keyboard_row2_upper,
			sizeof(keyboard_row2_upper) / sizeof(keyboard_row2_upper[0]) },
		{ keyboard_row3_upper,
			sizeof(keyboard_row3_upper) / sizeof(keyboard_row3_upper[0]) },
		{ keyboard_row4_controls,
			sizeof(keyboard_row4_controls) / sizeof(keyboard_row4_controls[0]) },
	},
};

///////////////////////////////////////
static const struct keyboard_layout *keyboard_layoutFor(
	const struct ui_keyboard *keyboard)
{
	if (keyboard && keyboard->shifted)
		return &keyboard_layout_upper;
	return &keyboard_layout_lower;
}

static int keyboard_keyWidth(const char *key)
{
	if (!strcmp(key, "SPACE"))
		return SCALE1(92);
	if (!strcmp(key, "SHIFT"))
		return SCALE1(44);
	if (!strcmp(key, "DEL"))
		return SCALE1(38);
	if (!strcmp(key, "DONE"))
		return SCALE1(44);
	return SCALE1(18);
}

static int keyboard_gap(void)
{
	return SCALE1(3);
}

static int keyboard_rowWidth(const struct keyboard_row *row)
{
	int total = 0;
	int i;

	if (!row)
		return 0;

	for (i = 0; i < row->count; i++)
		total += keyboard_keyWidth(row->keys[i]);
	if (row->count > 1)
		total += keyboard_gap() * (row->count - 1);
	return total;
}

static int keyboard_keyCenterX(const struct keyboard_row *row, int col)
{
	int x;
	int i;

	if (!row || row->count <= 0)
		return 0;
	if (col < 0)
		col = 0;
	if (col >= row->count)
		col = row->count - 1;

	x = (FIXED_WIDTH - keyboard_rowWidth(row)) / 2;
	for (i = 0; i < col; i++)
		x += keyboard_keyWidth(row->keys[i]) + keyboard_gap();
	return x + (keyboard_keyWidth(row->keys[col]) / 2);
}

static int keyboard_colForX(const struct keyboard_row *row, int target_x)
{
	int x;
	int i;
	int best_col = 0;
	int best_dist = 0x7fffffff;

	if (!row || row->count <= 0)
		return 0;

	x = (FIXED_WIDTH - keyboard_rowWidth(row)) / 2;
	for (i = 0; i < row->count; i++) {
		int center = x + (keyboard_keyWidth(row->keys[i]) / 2);
		int dist = abs(center - target_x);

		if (dist < best_dist) {
			best_dist = dist;
			best_col = i;
		}
		x += keyboard_keyWidth(row->keys[i]) + keyboard_gap();
	}
	return best_col;
}

static int keyboard_textLength(const struct ui_keyboard *keyboard)
{
	return strlen(keyboard->text);
}

static void keyboard_drawCenteredText(TTF_Font *face, const char *text,
	SDL_Color color, SDL_Surface *screen, int x, int y, int w, int h)
{
	SDL_Surface *surface;

	if (!face || !text || !text[0] || !screen)
		return;

	surface = TTF_RenderUTF8_Blended(face, text, color);
	if (!surface)
		return;

	SDL_BlitSurface(surface, NULL, screen, &(SDL_Rect){
		x + ((w - surface->w) / 2),
		y + ((h - surface->h) / 2),
	});
	SDL_FreeSurface(surface);
}

static int keyboard_insert(struct ui_keyboard *keyboard, const char *text)
{
	size_t len;
	size_t add_len;

	if (!keyboard || !text || !text[0])
		return 0;

	len = keyboard_textLength(keyboard);
	add_len = strlen(text);
	if (len + add_len >= sizeof(keyboard->text))
		return 0;

	snprintf(keyboard->text + len, sizeof(keyboard->text) - len, "%s",
		text);
	return 1;
}

///////////////////////////////////////
void UI_KEYBOARD_init(struct ui_keyboard *keyboard)
{
	UI_KEYBOARD_reset(keyboard);
}

void UI_KEYBOARD_reset(struct ui_keyboard *keyboard)
{
	if (!keyboard)
		return;

	memset(keyboard, 0, sizeof(*keyboard));
	keyboard->row = 1;
	keyboard->col = 0;
}

int UI_KEYBOARD_move(struct ui_keyboard *keyboard, int dx, int dy)
{
	const struct keyboard_layout *layout;
	int target_x;

	if (!keyboard)
		return 0;

	layout = keyboard_layoutFor(keyboard);
	if (dx < 0) {
		keyboard->col = (keyboard->col - 1 +
			layout->rows[keyboard->row].count) %
			layout->rows[keyboard->row].count;
		return 1;
	}
	if (dx > 0) {
		keyboard->col = (keyboard->col + 1) %
			layout->rows[keyboard->row].count;
		return 1;
	}
	if (dy == 0)
		return 0;

	target_x = keyboard_keyCenterX(&layout->rows[keyboard->row],
		keyboard->col);
	keyboard->row = (keyboard->row + dy + UI_KEYBOARD_ROWS) %
		UI_KEYBOARD_ROWS;
	layout = keyboard_layoutFor(keyboard);
	keyboard->col = keyboard_colForX(&layout->rows[keyboard->row],
		target_x);
	return 1;
}

int UI_KEYBOARD_insertSelected(struct ui_keyboard *keyboard)
{
	const char *key;

	key = UI_KEYBOARD_getSelectedKey(keyboard);
	if (!key || !key[0])
		return 0;
	if (!strcmp(key, "SPACE"))
		return keyboard_insert(keyboard, " ");
	if (!strcmp(key, "SHIFT") || !strcmp(key, "DEL") ||
			!strcmp(key, "DONE"))
		return 0;
	return keyboard_insert(keyboard, key);
}

int UI_KEYBOARD_backspace(struct ui_keyboard *keyboard)
{
	int len;

	if (!keyboard)
		return 0;

	len = keyboard_textLength(keyboard);
	if (len <= 0)
		return 0;
	keyboard->text[len - 1] = '\0';
	return 1;
}

int UI_KEYBOARD_toggleShift(struct ui_keyboard *keyboard)
{
	const struct keyboard_layout *layout;

	if (!keyboard)
		return 0;

	keyboard->shifted = !keyboard->shifted;
	layout = keyboard_layoutFor(keyboard);
	if (keyboard->col >= layout->rows[keyboard->row].count)
		keyboard->col = layout->rows[keyboard->row].count - 1;
	return 1;
}

void UI_KEYBOARD_copyDisplay(const struct ui_keyboard *keyboard, char *dst,
	size_t dst_size)
{
	if (!dst || !dst_size)
		return;
	dst[0] = '\0';
	if (!keyboard)
		return;

	snprintf(dst, dst_size, "%s", keyboard->text);
}

const char *UI_KEYBOARD_getSelectedKey(const struct ui_keyboard *keyboard)
{
	const struct keyboard_layout *layout;

	if (!keyboard)
		return "";
	layout = keyboard_layoutFor(keyboard);
	if (keyboard->row < 0 || keyboard->row >= UI_KEYBOARD_ROWS)
		return "";
	if (keyboard->col < 0 ||
			keyboard->col >= layout->rows[keyboard->row].count)
		return "";
	return layout->rows[keyboard->row].keys[keyboard->col];
}

void UI_KEYBOARD_draw(const struct ui_keyboard *keyboard, SDL_Surface *screen,
	const SDL_Rect *rect)
{
	const struct keyboard_layout *layout;
	int row_height;
	int row;

	if (!keyboard || !screen || !rect)
		return;

	layout = keyboard_layoutFor(keyboard);
	row_height = SCALE1(22);

	for (row = 0; row < UI_KEYBOARD_ROWS; row++) {
		const struct keyboard_row *row_def = &layout->rows[row];
		int row_total = keyboard_rowWidth(row_def);
		int x = rect->x + ((rect->w - row_total) / 2);
		int y = rect->y + (row * (row_height + keyboard_gap()));
		int col;

		for (col = 0; col < row_def->count; col++) {
			const char *key = row_def->keys[col];
			int width = keyboard_keyWidth(key);
			int selected = (row == keyboard->row &&
				col == keyboard->col);
			TTF_Font *face = strlen(key) > 1 ? font.tiny : font.small;

			GFX_blitPill(selected ? ASSET_WHITE_PILL :
				ASSET_DARK_GRAY_PILL, screen, &(SDL_Rect){
				x,
				y,
				width,
				row_height,
			});
			keyboard_drawCenteredText(face, key,
				selected ? COLOR_BLACK : COLOR_WHITE, screen,
				x, y, width, row_height);
			x += width + keyboard_gap();
		}
	}
}
