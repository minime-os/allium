#include <stdio.h>
#include <string.h>

#include "settings.h"
#include "jobs.h"

enum wifi_row_kind {
	WIFI_ROW_TOGGLE = 1,
	WIFI_ROW_NETWORK,
};

///////////////////////////////////////
static void setNetworkBadge(struct settings_item *item,
	const struct settings_wifi_network *network)
{
	const char *state;

	if (!item || !network)
		return;

	state = network->state;
	if (!state[0]) {
		if (network->secure)
			state = "Secure";
		else
			state = "Open";
	}

	if (network->signal > 0) {
		snprintf(item->badge.text, sizeof(item->badge.text), "%s | %d%%",
			state, network->signal);
	} else {
		SETTINGS_copyText(item->badge.text, sizeof(item->badge.text), state);
	}
}

static void wifi_setHint(struct ui_hint *hint, int label, const char *text)
{
	if (!hint)
		return;
	memset(hint, 0, sizeof(*hint));
	hint->label = label;
	hint->text = text;
}

static int wifi_networkIsConnected(const struct settings_screen *screen,
	const struct settings_item *item)
{
	if (!screen || !item)
		return 0;
	if (item->index < 0 || item->index >= screen->snapshot.wifi_network_count)
		return 0;
	return screen->snapshot.wifi_networks[item->index].connected;
}

static void wifi_fillPrimaryHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint)
{
	if (!item || !hint || !item->enabled)
		return;

	switch (item->action) {
	case SETTINGS_ACTION_WIFI_TOGGLE:
		wifi_setHint(hint, UI_HINT_LABEL_NONE,
			item->data ? "DISABLE" : "ENABLE");
		return;
	case SETTINGS_ACTION_WIFI_CONNECT:
		if (wifi_networkIsConnected(screen, item)) {
			wifi_setHint(hint, UI_HINT_LABEL_NONE, "DISCONNECT");
			return;
		}
		if ((item->extra & SETTINGS_FLAG_WIFI_SECURE) &&
				!(item->extra & SETTINGS_FLAG_WIFI_KNOWN)) {
			wifi_setHint(hint, UI_HINT_LABEL_NONE, "PASSWORD");
			return;
		}
		wifi_setHint(hint, UI_HINT_LABEL_NONE, "CONNECT");
		return;
	default:
		return;
	}
}

static void wifi_fillAuxHint(const struct settings_item *item,
	struct ui_hint *hint)
{
	if (!item || !hint || !item->enabled)
		return;
	if (item->action != SETTINGS_ACTION_WIFI_CONNECT)
		return;
	if (!(item->extra & SETTINGS_FLAG_WIFI_KNOWN))
		return;
	wifi_setHint(hint, UI_HINT_LABEL_NONE, "FORGET");
}

///////////////////////////////////////
static int hasNetworkRow(const struct settings_item *items, int count,
	const char *ssid)
{
	int i;

	for (i = 0; i < count; i++) {
		if (items[i].data != WIFI_ROW_NETWORK)
			continue;
		if (!strcmp(items[i].arg, ssid))
			return 1;
	}
	return 0;
}

static int addNetworks(struct settings_item *items, int max_items,
	int *count, const struct settings_snapshot *snapshot, int group)
{
	int i;

	for (i = 0; i < snapshot->wifi_network_count; i++) {
		const struct settings_wifi_network *network;
		struct settings_item *item;
		int matches = 0;

		network = &snapshot->wifi_networks[i];
		if (hasNetworkRow(items, *count, network->ssid))
			continue;

		if (group == 0)
			matches = network->connected;
		else if (group == 1)
			matches = network->known && !network->connected;
		else
			matches = !network->known && !network->connected;
		if (!matches)
			continue;
		if (*count >= max_items)
			return 0;
		item = &items[(*count)++];
		SETTINGS_initItem(item, SETTINGS_ITEM_ACTION,
			SETTINGS_ACTION_WIFI_CONNECT, network->ssid, "");
		item->data = WIFI_ROW_NETWORK;
		item->enabled = snapshot->wifi_enabled &&
			!snapshot->wifi_busy;
		item->index = i;
		setNetworkBadge(item, network);
		if (network->secure)
			item->extra |= SETTINGS_FLAG_WIFI_SECURE;
		if (network->known)
			item->extra |= SETTINGS_FLAG_WIFI_KNOWN;
		SETTINGS_copyText(item->arg, sizeof(item->arg), network->ssid);
	}
	return 1;
}

///////////////////////////////////////
int SETTINGS_WIFI_buildMenu(struct settings_screen *screen,
	const struct settings_snapshot *snapshot,
	struct settings_item *items, int max_items)
{
	const char *toggle_label;
	struct settings_item *item;
	int count = 0;

	(void)screen;

	if (!snapshot || !items || max_items < 2)
		return 0;

	if (snapshot->wifi_busy)
		toggle_label = snapshot->wifi_toggle_target ?
			"Enabling Wi-Fi" : "Disabling Wi-Fi";
	else
		toggle_label = snapshot->wifi_enabled ?
			"Disable Wi-Fi" : "Enable Wi-Fi";

	if (count >= max_items)
		return 0;
	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_ACTION,
		SETTINGS_ACTION_WIFI_TOGGLE, toggle_label, "");
	item->data = WIFI_ROW_TOGGLE;
	item->enabled = !snapshot->wifi_busy;
	item->busy = snapshot->wifi_busy;
	item->data = snapshot->wifi_enabled;

	if (snapshot->wifi_enabled) {
		if (!addNetworks(items, max_items, &count, snapshot, 0))
			return count;
		if (!addNetworks(items, max_items, &count, snapshot, 1))
			return count;
		if (!addNetworks(items, max_items, &count, snapshot, 2))
			return count;
	}

	return count;
}

int SETTINGS_WIFI_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction)
{
	int rc;
	int target;

	(void)direction;

	if (!screen || !item)
		return 0;

	switch (item->action) {
	case SETTINGS_ACTION_WIFI_TOGGLE:
		target = item->data ? 0 : 1;
		rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_WIFI_TOGGLE, target,
			NULL);
		if (rc != 0)
			SETTINGS_setNotice(screen, "Wi-Fi change queued failed");
		return 1;
	case SETTINGS_ACTION_WIFI_CONNECT:
		if (item->index < 0 ||
				item->index >= screen->snapshot.wifi_network_count)
			return 0;
		if (screen->snapshot.wifi_networks[item->index].connected) {
			rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_WIFI_DISCONNECT,
				0, item->arg);
			if (rc != 0)
				SETTINGS_setNotice(screen,
					"Wi-Fi disconnect queued failed");
			return 1;
		}
		if ((item->extra & SETTINGS_FLAG_WIFI_SECURE) &&
				!(item->extra & SETTINGS_FLAG_WIFI_KNOWN)) {
			UI_DIALOG_openWifiPassphrase(&screen->dialog, item->arg);
			SETTINGS_setNotice(screen, "Enter Wi-Fi password");
			return 1;
		}
		rc = SETTINGS_JOBS_enqueueWifiConnect(item->arg, NULL);
		if (rc != 0)
			SETTINGS_setNotice(screen, "Wi-Fi connect queued failed");
		return 1;
	default:
		return 0;
	}
}

int SETTINGS_WIFI_activateAux(struct settings_screen *screen,
	const struct settings_item *item)
{
	int rc;

	if (!screen || !item)
		return 0;
	if (item->action != SETTINGS_ACTION_WIFI_CONNECT)
		return 0;
	if (!(item->extra & SETTINGS_FLAG_WIFI_KNOWN))
		return 0;

	rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_WIFI_FORGET, 0, item->arg);
	if (rc != 0)
		SETTINGS_setNotice(screen, "Wi-Fi forget queued failed");
	return 1;
}

void SETTINGS_WIFI_getHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint)
{
	wifi_fillPrimaryHint(screen, item, hint);
}

void SETTINGS_WIFI_getAuxHint(struct settings_screen *screen,
	const struct settings_item *item, struct ui_hint *hint)
{
	(void)screen;
	wifi_fillAuxHint(item, hint);
}
