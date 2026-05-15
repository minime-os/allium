#include <stdio.h>
#include <string.h>

#include "settings.h"
#include "jobs.h"

enum bt_row_kind {
	BT_ROW_TOGGLE = 1,
	BT_ROW_DEVICE,
};

///////////////////////////////////////
static int isSupportedKind(int kind)
{
	return kind == SETTINGS_BT_DEVICE_AUDIO ||
		kind == SETTINGS_BT_DEVICE_GAMEPAD;
}

static const char *bt_kindName(int kind)
{
	if (kind == SETTINGS_BT_DEVICE_AUDIO)
		return "audio";
	if (kind == SETTINGS_BT_DEVICE_GAMEPAD)
		return "gamepad";
	return "device";
}

static void bt_setHint(struct ui_hint *hint, int label, const char *text)
{
	if (!hint)
		return;
	memset(hint, 0, sizeof(*hint));
	hint->label = label;
	hint->text = text;
}

static void setDeviceBadge(struct settings_item *item,
	const struct settings_bt_device *device)
{
	if (!item || !device)
		return;

	snprintf(item->badge.text, sizeof(item->badge.text), "%s | %s",
		bt_kindName(device->kind), device->state[0] ? device->state : "new");
}

static int addDevices(struct settings_item *items, int max_items,
	int *count, const struct settings_snapshot *snapshot, int group)
{
	int i;

	for (i = 0; i < snapshot->bt_device_count; i++) {
		const struct settings_bt_device *device;
		struct settings_item *item;
		int matches = 0;

		device = &snapshot->bt_devices[i];
		if (!isSupportedKind(device->kind))
			continue;

		if (group == 0)
			matches = device->paired && device->connected;
		else if (group == 1)
			matches = device->paired && !device->connected;
		else
			matches = !device->paired;
		if (!matches)
			continue;
		if (*count >= max_items)
			return 0;
		item = &items[(*count)++];
		SETTINGS_initItem(item, SETTINGS_ITEM_ACTION,
			SETTINGS_ACTION_BT_DEVICE_TOGGLE,
			device->name[0] ? device->name : device->addr, "");
		item->data = BT_ROW_DEVICE;
		item->enabled = snapshot->bt_enabled &&
			!snapshot->bt_busy;
		item->index = i;
		setDeviceBadge(item, device);
		if (device->connected)
			item->extra |= SETTINGS_FLAG_BT_CONNECTED;
		if (device->paired)
			item->extra |= SETTINGS_FLAG_BT_PAIRED;
		SETTINGS_copyText(item->arg, sizeof(item->arg), device->addr);
	}
	return 1;
}

///////////////////////////////////////
int SETTINGS_BT_buildMenu(struct settings_screen *screen,
	const struct settings_snapshot *snapshot,
	struct settings_item *items, int max_items)
{
	const char *toggle_label;
	struct settings_item *item;
	int count = 0;

	(void)screen;

	if (!snapshot || !items || max_items < 2)
		return 0;

	if (snapshot->bt_busy)
		toggle_label = snapshot->bt_toggle_target ?
			"Enabling Bluetooth" : "Disabling Bluetooth";
	else
		toggle_label = snapshot->bt_enabled ?
			"Disable Bluetooth" : "Enable Bluetooth";

	if (count >= max_items)
		return 0;
	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_ACTION,
		SETTINGS_ACTION_BT_TOGGLE, toggle_label, "");
	item->data = BT_ROW_TOGGLE;
	item->enabled = !snapshot->bt_busy;
	item->busy = snapshot->bt_busy;
	item->data = snapshot->bt_enabled;

	if (snapshot->bt_enabled) {
		if (!addDevices(items, max_items, &count, snapshot, 0))
			return count;
		if (!addDevices(items, max_items, &count, snapshot, 1))
			return count;
		if (!addDevices(items, max_items, &count, snapshot, 2))
			return count;
	}

	return count;
}

int SETTINGS_BT_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction)
{
	int rc;
	int target;

	(void)direction;

	if (!screen || !item)
		return 0;

	switch (item->action) {
	case SETTINGS_ACTION_BT_TOGGLE:
		target = item->data ? 0 : 1;
		rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_BT_TOGGLE, target,
			NULL);
		if (rc != 0)
			SETTINGS_setNotice(screen,
				"Bluetooth change queued failed");
		return 1;
	case SETTINGS_ACTION_BT_DEVICE_TOGGLE:
		if (item->index < 0 ||
				item->index >= screen->snapshot.bt_device_count)
			return 0;
		rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_BT_DEVICE_TOGGLE,
			item->extra, item->arg);
		if (rc != 0) {
			SETTINGS_setNotice(screen,
				"Bluetooth device queued failed");
		}
		return 1;
	default:
		return 0;
	}
}

int SETTINGS_BT_activateAux(struct settings_screen *screen,
	const struct settings_item *item)
{
	int rc;

	if (!screen || !item)
		return 0;
	if (item->action != SETTINGS_ACTION_BT_DEVICE_TOGGLE)
		return 0;
	if (!(item->extra & SETTINGS_FLAG_BT_PAIRED))
		return 0;

	rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_BT_DEVICE_FORGET, 0, item->arg);
	if (rc != 0)
		SETTINGS_setNotice(screen, "Bluetooth forget queued failed");
	return 1;
}

void SETTINGS_BT_getHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint)
{
	const struct settings_bt_device *device;

	if (!screen || !item || !hint || !item->enabled)
		return;

	switch (item->action) {
	case SETTINGS_ACTION_BT_TOGGLE:
		bt_setHint(hint, UI_HINT_LABEL_NONE,
			item->data ? "DISABLE" : "ENABLE");
		return;
	case SETTINGS_ACTION_BT_DEVICE_TOGGLE:
		if (item->index < 0 ||
				item->index >= screen->snapshot.bt_device_count)
			return;
		device = &screen->snapshot.bt_devices[item->index];
		if (device->connected) {
			bt_setHint(hint, UI_HINT_LABEL_NONE, "DISCONNECT");
			return;
		}
		if (device->paired) {
			bt_setHint(hint, UI_HINT_LABEL_NONE, "CONNECT");
			return;
		}
		bt_setHint(hint, UI_HINT_LABEL_NONE, "PAIR");
		return;
	default:
		return;
	}
}

void SETTINGS_BT_getAuxHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint)
{
	(void)screen;

	if (!item || !hint || !item->enabled)
		return;
	if (item->action == SETTINGS_ACTION_BT_DEVICE_TOGGLE &&
			(item->extra & SETTINGS_FLAG_BT_PAIRED))
		bt_setHint(hint, UI_HINT_LABEL_NONE, "FORGET");
}
