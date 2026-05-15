#ifndef __UI_BADGE_H__
#define __UI_BADGE_H__

#include "list.h"

///////////////////////////////////////
int UI_BADGE_hasText(const struct ui_badge *badge);
int UI_BADGE_measure(const struct ui_badge *badge);
void UI_BADGE_draw(const struct ui_badge *badge, int enabled, int selected,
	SDL_Surface *screen, const SDL_Rect *row_rect);

#endif
