#ifndef __SETTINGS_MENU_H__
#define __SETTINGS_MENU_H__

#include "settings.h"

///////////////////////////////////////
const struct settings_menu *SETTINGS_MENU_get(int menu_id);
int SETTINGS_MENU_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction);
int SETTINGS_MENU_activateAux(struct settings_screen *screen,
	const struct settings_item *item);
void SETTINGS_MENU_getHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint);
void SETTINGS_MENU_getAuxHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint);

#endif
