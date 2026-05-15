#include <stdio.h>
#include <string.h>

#include "settings.h"

///////////////////////////////////////
static int controls_buttonWidth(const char *label)
{
	SDL_Surface *text;
	int width = 0;

	if (!label)
		return 0;
	if (strlen(label) <= 2)
		return SCALE1(BUTTON_SIZE);

	text = TTF_RenderUTF8_Blended(font.tiny, label, COLOR_BUTTON_TEXT);
	if (!text)
		return SCALE1(BUTTON_SIZE);
	width = SCALE1(BUTTON_SIZE) + text->w;
	SDL_FreeSurface(text);
	return width;
}

static void controls_drawButton(const char *label, SDL_Surface *dst,
	int pressed, int x, int y, int width)
{
	SDL_Rect point = { x, y };
	SDL_Surface *text;
	int len;

	if (!label || !dst)
		return;

	len = strlen(label);
	if (len <= 2) {
		text = TTF_RenderUTF8_Blended(len == 2 ? font.small : font.medium,
			label, COLOR_BUTTON_TEXT);
		GFX_blitAsset(pressed ? ASSET_BUTTON : ASSET_HOLE, NULL, dst,
			&point);
		if (text) {
			SDL_BlitSurface(text, NULL, dst, &(SDL_Rect){
				point.x + (SCALE1(BUTTON_SIZE) - text->w) / 2,
				point.y + (SCALE1(BUTTON_SIZE) - text->h) / 2,
			});
			SDL_FreeSurface(text);
		}
		return;
	}

	text = TTF_RenderUTF8_Blended(font.tiny, label, COLOR_BUTTON_TEXT);
	if (!text)
		return;
	if (!width)
		width = SCALE1(BUTTON_SIZE) / 2 + text->w;
	GFX_blitPill(pressed ? ASSET_BUTTON : ASSET_HOLE, dst, &(SDL_Rect){
		point.x,
		point.y,
		width,
		SCALE1(BUTTON_SIZE)
	});
	SDL_BlitSurface(text, NULL, dst, &(SDL_Rect){
		point.x + (width - text->w) / 2,
		point.y + (SCALE1(BUTTON_SIZE) - text->h) / 2,
	});
	SDL_FreeSurface(text);
}

static int controls_hasButton(int button, int code, int joy)
{
	return button != BUTTON_NA || code != CODE_NA || joy != JOY_NA;
}

///////////////////////////////////////
void SETTINGS_CONTROLS_enter(struct settings_screen *screen,
	const struct settings_snapshot *snapshot)
{
	(void)snapshot;

	if (!screen)
		return;
	memset(&screen->controls, 0, sizeof(screen->controls));
	screen->controls.last_pressed = pad.is_pressed;
}

void SETTINGS_CONTROLS_update(struct settings_screen *screen,
	const struct settings_snapshot *snapshot, int *dirty)
{
	if (!screen)
		return;
	if (screen->controls.last_pressed != (uint32_t)pad.is_pressed ||
			PAD_anyJustPressed() || PAD_anyJustReleased()) {
		screen->controls.last_pressed = pad.is_pressed;
		if (dirty)
			*dirty = 1;
	}
	(void)snapshot;
}

int SETTINGS_CONTROLS_handleInput(struct settings_screen *screen)
{
	(void)screen;

	if (PAD_isPressed(BTN_SELECT) && PAD_isPressed(BTN_START))
		return SETTINGS_CUSTOM_INPUT_BACK;
	if (PAD_justPressed(BTN_B))
		return SETTINGS_CUSTOM_INPUT_CONSUME;
	return SETTINGS_CUSTOM_INPUT_NONE;
}

void SETTINGS_CONTROLS_buildHints(struct settings_screen *screen,
	struct ui_hint_group *top, struct ui_hint_group *bottom)
{
	(void)screen;

	if (!top || !bottom)
		return;

	memset(top, 0, sizeof(*top));
	memset(bottom, 0, sizeof(*bottom));
	bottom->count = 2;
	bottom->primary = 0;
	bottom->align_right = 1;
	bottom->hints[0].button = UI_BUTTON_ID_SELECT;
	bottom->hints[0].text = "+";
	bottom->hints[1].button = UI_BUTTON_ID_START;
	bottom->hints[1].text = "EXIT";
}

void SETTINGS_CONTROLS_draw(struct settings_screen *screen, SDL_Surface *dst)
{
	int has_l2;
	int has_r2;
	int has_l3;
	int has_r3;
	int has_volume;
	int has_power;
	int has_menu;
	int has_both;
	int oy;

	(void)screen;

	if (!dst)
		return;

	has_l2 = controls_hasButton(BUTTON_L2, CODE_L2, JOY_L2) ||
		AXIS_L2 != AXIS_NA;
	has_r2 = controls_hasButton(BUTTON_R2, CODE_R2, JOY_R2) ||
		AXIS_R2 != AXIS_NA;
	has_l3 = controls_hasButton(BUTTON_L3, CODE_L3, JOY_L3);
	has_r3 = controls_hasButton(BUTTON_R3, CODE_R3, JOY_R3);
	has_volume = controls_hasButton(BUTTON_PLUS, CODE_PLUS, JOY_PLUS);
	has_power = HAS_POWER_BUTTON;
	has_menu = HAS_MENU_BUTTON;
	has_both = has_power && has_menu;

	oy = SCALE1(PADDING);
	if (!has_l3 && !has_r3)
		oy += SCALE1(PILL_SIZE);

	{
		int x = SCALE1(BUTTON_MARGIN + PADDING);
		int y = oy;
		int width = controls_buttonWidth("L1") + SCALE1(BUTTON_MARGIN) * 2;
		int offset = width;

		if (has_l2)
			width += controls_buttonWidth("L2") + SCALE1(BUTTON_MARGIN);
		if (!has_l2)
			x += SCALE1(PILL_SIZE);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, width });

		controls_drawButton("L1", dst, PAD_isPressed(BTN_L1),
			x + SCALE1(BUTTON_MARGIN), y + SCALE1(BUTTON_MARGIN), 0);
		if (has_l2)
			controls_drawButton("L2", dst, PAD_isPressed(BTN_L2),
				x + offset, y + SCALE1(BUTTON_MARGIN), 0);
	}

	{
		int x;
		int y = oy;
		int width = controls_buttonWidth("R1") + SCALE1(BUTTON_MARGIN) * 2;
		int offset = width;

		if (has_r2)
			width += controls_buttonWidth("R2") + SCALE1(BUTTON_MARGIN);
		x = FIXED_WIDTH - width - SCALE1(BUTTON_MARGIN + PADDING);
		if (!has_r2)
			x -= SCALE1(PILL_SIZE);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, width });

		controls_drawButton(has_r2 ? "R2" : "R1", dst,
			PAD_isPressed(has_r2 ? BTN_R2 : BTN_R1),
			x + SCALE1(BUTTON_MARGIN), y + SCALE1(BUTTON_MARGIN), 0);
		if (has_r2)
			controls_drawButton("R1", dst, PAD_isPressed(BTN_R1),
				x + offset, y + SCALE1(BUTTON_MARGIN), 0);
	}

	{
		int x = SCALE1(PADDING + PILL_SIZE);
		int y = oy + SCALE1(PILL_SIZE * 2);
		int o = SCALE1(BUTTON_MARGIN);

		SDL_FillRect(dst, &(SDL_Rect){
			x,
			y + SCALE1(PILL_SIZE / 2),
			SCALE1(PILL_SIZE),
			SCALE1(PILL_SIZE * 2)
		}, RGB_DARK_GRAY);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("U", dst, PAD_isPressed(BTN_DPAD_UP), x + o,
			y + o, 0);

		y += SCALE1(PILL_SIZE * 2);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("D", dst, PAD_isPressed(BTN_DPAD_DOWN), x + o,
			y + o, 0);

		x -= SCALE1(PILL_SIZE);
		y -= SCALE1(PILL_SIZE);
		SDL_FillRect(dst, &(SDL_Rect){
			x + SCALE1(PILL_SIZE / 2),
			y,
			SCALE1(PILL_SIZE * 2),
			SCALE1(PILL_SIZE)
		}, RGB_DARK_GRAY);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("L", dst, PAD_isPressed(BTN_DPAD_LEFT), x + o,
			y + o, 0);

		x += SCALE1(PILL_SIZE * 2);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("R", dst, PAD_isPressed(BTN_DPAD_RIGHT), x + o,
			y + o, 0);
	}

	{
		int x = FIXED_WIDTH - SCALE1(PADDING + PILL_SIZE * 3) +
			SCALE1(PILL_SIZE);
		int y = oy + SCALE1(PILL_SIZE * 2);
		int o = SCALE1(BUTTON_MARGIN);

		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("X", dst, PAD_isPressed(BTN_X), x + o, y + o, 0);

		y += SCALE1(PILL_SIZE * 2);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("B", dst, PAD_isPressed(BTN_B), x + o, y + o, 0);

		x -= SCALE1(PILL_SIZE);
		y -= SCALE1(PILL_SIZE);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("Y", dst, PAD_isPressed(BTN_Y), x + o, y + o, 0);

		x += SCALE1(PILL_SIZE * 2);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("A", dst, PAD_isPressed(BTN_A), x + o, y + o, 0);
	}

	if (has_volume) {
		int x = (FIXED_WIDTH - SCALE1(99)) / 2;
		int y = oy + SCALE1(PILL_SIZE);
		int width = SCALE1(42);

		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){
			x,
			y,
			SCALE1(98)
		});
		x += SCALE1(BUTTON_MARGIN);
		y += SCALE1(BUTTON_MARGIN);
		controls_drawButton("VOL. -", dst, PAD_isPressed(BTN_MINUS), x, y,
			width);
		x += width + SCALE1(BUTTON_MARGIN);
		controls_drawButton("VOL. +", dst, PAD_isPressed(BTN_PLUS), x, y,
			width);
	}

	if (has_power || has_menu) {
		int button_width = SCALE1(42);
		int pill_width = has_both ?
			SCALE1(42 * 2 + BUTTON_MARGIN * 3) :
			SCALE1(42 + BUTTON_MARGIN * 2);
		int x = (FIXED_WIDTH - pill_width) / 2;
		int y = oy + SCALE1(PILL_SIZE * 3);

		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){
			x,
			y,
			pill_width
		});
		x += SCALE1(BUTTON_MARGIN);
		y += SCALE1(BUTTON_MARGIN);
		if (has_menu) {
			controls_drawButton("MENU", dst, PAD_isPressed(BTN_MENU), x, y,
				button_width);
			x += button_width + SCALE1(BUTTON_MARGIN);
		}
		if (has_power)
			controls_drawButton("POWER", dst, PAD_isPressed(BTN_POWER), x,
				y, button_width);
	}

	{
		int x = (FIXED_WIDTH - SCALE1(99)) / 2;
		int y = oy + SCALE1(PILL_SIZE * 5);
		int width = SCALE1(42);
		int meta_width = SCALE1(99);

		if (has_l3 || has_r3)
			meta_width = SCALE1(130);
		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){
			x,
			y,
			meta_width
		});
		x += SCALE1(BUTTON_MARGIN);
		y += SCALE1(BUTTON_MARGIN);
		controls_drawButton("SELECT", dst, PAD_isPressed(BTN_SELECT), x, y,
			width);
		x += width + SCALE1(BUTTON_MARGIN);
		controls_drawButton("START", dst, PAD_isPressed(BTN_START), x, y,
			width);
	}

	if (has_l3) {
		int x = SCALE1(PADDING + PILL_SIZE);
		int y = oy + SCALE1(PILL_SIZE * 6);
		int o = SCALE1(BUTTON_MARGIN);

		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("L3", dst, PAD_isPressed(BTN_L3), x + o, y + o,
			0);
	}

	if (has_r3) {
		int x = FIXED_WIDTH - SCALE1(PADDING + PILL_SIZE * 3) +
			SCALE1(PILL_SIZE);
		int y = oy + SCALE1(PILL_SIZE * 6);
		int o = SCALE1(BUTTON_MARGIN);

		GFX_blitPill(ASSET_DARK_GRAY_PILL, dst, &(SDL_Rect){ x, y, 0 });
		controls_drawButton("R3", dst, PAD_isPressed(BTN_R3), x + o, y + o,
			0);
	}
}
