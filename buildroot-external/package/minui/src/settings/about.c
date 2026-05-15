#include "settings.h"

///////////////////////////////////////
int SETTINGS_ABOUT_buildMenu(struct settings_screen *screen,
	const struct settings_snapshot *snapshot,
	struct settings_item *items, int max_items)
{
	struct settings_item *item;
	int count = 0;

	if (!screen || !snapshot || !items || max_items < 3)
		return 0;

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_READONLY, SETTINGS_ACTION_NONE,
		"Release", "");
	SETTINGS_copyText(item->badge.text, sizeof(item->badge.text),
		snapshot->release);

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_READONLY, SETTINGS_ACTION_NONE,
		"Commit", "");
	SETTINGS_copyText(item->badge.text, sizeof(item->badge.text),
		snapshot->commit);

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_READONLY, SETTINGS_ACTION_NONE,
		"Model", "");
	SETTINGS_copyText(item->badge.text, sizeof(item->badge.text),
		snapshot->model);

	return count;
}

int SETTINGS_ABOUT_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction)
{
	(void)screen;
	(void)item;
	(void)direction;
	return 0;
}
