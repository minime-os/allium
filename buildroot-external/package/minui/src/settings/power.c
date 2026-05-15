#include <stdio.h>

#include "settings.h"
#include "jobs.h"

static const int power_timeout_values[] = {
	PWR_TIMEOUT_OFF,
	PWR_TIMEOUT_1_MIN,
	PWR_TIMEOUT_5_MIN,
	PWR_TIMEOUT_15_MIN,
	PWR_TIMEOUT_30_MIN,
	PWR_TIMEOUT_1_HOUR,
};

///////////////////////////////////////
// enum helpers

static int next_enum_index(const int *values, int count, int current,
	int direction)
{
	int i;

	for (i = 0; i < count; i++) {
		if (values[i] == current)
			break;
	}
	if (i >= count)
		i = 0;
	if (direction < 0)
		return i == 0 ? count - 1 : i - 1;
	return i >= count - 1 ? 0 : i + 1;
}

static int next_timeout_value(int current, int direction)
{
	return power_timeout_values[next_enum_index(power_timeout_values,
		(int)(sizeof(power_timeout_values) / sizeof(power_timeout_values[0])),
		current, direction)];
}

static int next_behavior_value(int current, int direction, int allow_auto)
{
	int values[3];
	int count = 0;

	values[count++] = PWR_BEHAVIOR_SLEEP_ONLY;
	if (allow_auto)
		values[count++] = PWR_BEHAVIOR_AUTO_SHUTDOWN;
	values[count++] = PWR_BEHAVIOR_SHUT_DOWN_NOW;

	return values[next_enum_index(values, count, current, direction)];
}

static const char *power_timeout_label(int value)
{
	switch (value) {
	case PWR_TIMEOUT_OFF:
		return "Off";
	case PWR_TIMEOUT_1_MIN:
		return "1 min";
	case PWR_TIMEOUT_5_MIN:
		return "5 min";
	case PWR_TIMEOUT_15_MIN:
		return "15 min";
	case PWR_TIMEOUT_30_MIN:
		return "30 min";
	case PWR_TIMEOUT_1_HOUR:
		return "1 hr";
	default:
		return "Unknown";
	}
}

static const char *power_behavior_label(int value)
{
	switch (value) {
	case PWR_BEHAVIOR_SLEEP_ONLY:
		return "Sleep Only";
	case PWR_BEHAVIOR_AUTO_SHUTDOWN:
		return "Auto Shutdown";
	case PWR_BEHAVIOR_SHUT_DOWN_NOW:
		return "Shut Down Now";
	default:
		return "Unknown";
	}
}

///////////////////////////////////////
int SETTINGS_POWER_buildMenu(struct settings_screen *screen,
	const struct settings_snapshot *snapshot,
	struct settings_item *items, int max_items)
{
	struct settings_item *item;
	int count = 0;

	(void)screen;

	if (!snapshot || !items || max_items < 4)
		return 0;

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_ENUM,
		SETTINGS_ACTION_POWER_SLEEP_TIMEOUT, "Sleep Timeout", "");
	item->data = snapshot->power_sleep_timeout_ms;
	SETTINGS_copyText(item->badge.text, sizeof(item->badge.text),
		power_timeout_label(item->data));

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_ENUM,
		SETTINGS_ACTION_POWER_AUTO_SHUTDOWN_TIMEOUT,
		"Auto Shutdown Timeout", "");
	item->data = snapshot->power_auto_shutdown_timeout_ms;
	SETTINGS_copyText(item->badge.text, sizeof(item->badge.text),
		power_timeout_label(item->data));

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_ENUM,
		SETTINGS_ACTION_POWER_LID_BEHAVIOR, "Lid Behavior", "");
	item->data = snapshot->power_lid_behavior;
	SETTINGS_copyText(item->badge.text, sizeof(item->badge.text),
		power_behavior_label(item->data));

	item = &items[count++];
	SETTINGS_initItem(item, SETTINGS_ITEM_ENUM,
		SETTINGS_ACTION_POWER_BUTTON_BEHAVIOR,
		"Power Button Behavior", "");
	item->data = snapshot->power_button_behavior;
	SETTINGS_copyText(item->badge.text, sizeof(item->badge.text),
		power_behavior_label(item->data));

	return count;
}

int SETTINGS_POWER_activate(struct settings_screen *screen,
	const struct settings_item *item, int direction)
{
	int target;
	int rc;

	if (!screen || !item)
		return 0;

	switch (item->action) {
	case SETTINGS_ACTION_POWER_SLEEP_TIMEOUT:
		target = next_timeout_value(item->data, direction);
		rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_POWER_SLEEP_TIMEOUT,
			target, NULL);
		if (rc == 0)
			SETTINGS_setNotice(screen, "Updating sleep timeout");
		else
			SETTINGS_setNotice(screen,
				"Sleep timeout update queued failed");
		return 1;
	case SETTINGS_ACTION_POWER_AUTO_SHUTDOWN_TIMEOUT:
		target = next_timeout_value(item->data, direction);
		rc = SETTINGS_JOBS_enqueue(
			SETTINGS_JOB_POWER_AUTO_SHUTDOWN_TIMEOUT, target, NULL);
		if (rc == 0)
			SETTINGS_setNotice(screen,
				"Updating auto shutdown timeout");
		else
			SETTINGS_setNotice(screen,
				"Auto shutdown timeout queued failed");
		return 1;
	case SETTINGS_ACTION_POWER_LID_BEHAVIOR:
		target = next_behavior_value(item->data, direction,
			screen->snapshot.power_auto_shutdown_timeout_ms !=
			PWR_TIMEOUT_OFF);
		rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_POWER_LID_BEHAVIOR,
			target, NULL);
		if (rc == 0)
			SETTINGS_setNotice(screen, "Updating lid behavior");
		else
			SETTINGS_setNotice(screen,
				"Lid behavior update queued failed");
		return 1;
	case SETTINGS_ACTION_POWER_BUTTON_BEHAVIOR:
		target = next_behavior_value(item->data, direction,
			screen->snapshot.power_auto_shutdown_timeout_ms !=
			PWR_TIMEOUT_OFF);
		rc = SETTINGS_JOBS_enqueue(
			SETTINGS_JOB_POWER_BUTTON_BEHAVIOR, target, NULL);
		if (rc == 0)
			SETTINGS_setNotice(screen,
				"Updating power button behavior");
		else
			SETTINGS_setNotice(screen,
				"Power button behavior queued failed");
		return 1;
	default:
		return 0;
	}
}
