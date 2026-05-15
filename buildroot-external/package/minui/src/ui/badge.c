#include <stdio.h>
#include <string.h>

#include "defines.h"
#include "badge.h"

///////////////////////////////////////
int UI_BADGE_hasText(const struct ui_badge *badge)
{
	return badge && badge->text[0] != '\0';
}

int UI_BADGE_measure(const struct ui_badge *badge)
{
	int width = 0;

	if (!badge || !badge->text[0])
		return 0;
	TTF_SizeUTF8(font.small, badge->text, &width, NULL);
	return width + SCALE1(BUTTON_PADDING);
}

void UI_BADGE_draw(const struct ui_badge *badge, int enabled, int selected,
	SDL_Surface *screen, const SDL_Rect *row_rect)
{
	SDL_Surface *text;
	SDL_Color color;

	if (!screen || !row_rect || !badge || !badge->text[0])
		return;

	if (!enabled)
		color = COLOR_GRAY;
	else if (selected)
		color = COLOR_BLACK;
	else
		color = COLOR_DARK_TEXT;

	text = TTF_RenderUTF8_Blended(font.small, badge->text, color);
	if (!text)
		return;

	SDL_BlitSurface(text, NULL, screen, &(SDL_Rect){
		row_rect->x + row_rect->w - SCALE1(BUTTON_PADDING) - text->w,
		row_rect->y + SCALE1(7),
	});
	SDL_FreeSurface(text);
}
