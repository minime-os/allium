#ifndef __SETTINGS_H__
#define __SETTINGS_H__

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

#include "defines.h"
#include "api.h"
#include "dialog.h"
#include "list.h"

#define SETTINGS_MAX_ITEMS 48
#define SETTINGS_MAX_DEPTH 8
#define SETTINGS_MAX_NETWORKS 16
#define SETTINGS_MAX_BT_DEVICES 16
#define SETTINGS_MAX_JOBS 16
#define SETTINGS_ROW_COUNT 6
#define SETTINGS_TIME_STATUS_ROWS 4

#define SETTINGS_FLAG_WIFI_SECURE 0x01
#define SETTINGS_FLAG_WIFI_KNOWN 0x02
#define SETTINGS_FLAG_BT_CONNECTED 0x01
#define SETTINGS_FLAG_BT_PAIRED 0x02
#define SETTINGS_FLAG_BT_AUTOCONFIRM 0x04

///////////////////////////////////////
enum settings_menu_id {
	SETTINGS_MENU_NONE = -1,
	SETTINGS_MENU_ROOT,
	SETTINGS_MENU_WIFI,
	SETTINGS_MENU_BT,
	SETTINGS_MENU_POWER,
	SETTINGS_MENU_TIME,
	SETTINGS_MENU_CONTROLS,
	SETTINGS_MENU_ABOUT,
};

enum settings_item_type {
	SETTINGS_ITEM_SUBMENU,
	SETTINGS_ITEM_ACTION,
	SETTINGS_ITEM_TOGGLE,
	SETTINGS_ITEM_ENUM,
	SETTINGS_ITEM_READONLY,
};

enum settings_action_id {
	SETTINGS_ACTION_NONE,
	SETTINGS_ACTION_WIFI_TOGGLE,
	SETTINGS_ACTION_WIFI_CONNECT,
	SETTINGS_ACTION_WIFI_DISCONNECT,
	SETTINGS_ACTION_BT_TOGGLE,
	SETTINGS_ACTION_BT_DEVICE_TOGGLE,
	SETTINGS_ACTION_POWER_SLEEP_TIMEOUT,
	SETTINGS_ACTION_POWER_AUTO_SHUTDOWN_TIMEOUT,
	SETTINGS_ACTION_POWER_LID_BEHAVIOR,
	SETTINGS_ACTION_POWER_BUTTON_BEHAVIOR,
	SETTINGS_ACTION_POWER_BRIGHTNESS,
	SETTINGS_ACTION_POWER_VOLUME,
	SETTINGS_ACTION_POWER_MUTE,
	SETTINGS_ACTION_POWER_OFF,
	SETTINGS_ACTION_TIME_SYNC,
	SETTINGS_ACTION_TIME_SET,
	SETTINGS_ACTION_TIMEZONE_SET,
};

enum settings_job_type {
	SETTINGS_JOB_NONE,
	SETTINGS_JOB_WIFI_TOGGLE,
	SETTINGS_JOB_WIFI_CONNECT,
	SETTINGS_JOB_WIFI_DISCONNECT,
	SETTINGS_JOB_WIFI_FORGET,
	SETTINGS_JOB_BT_TOGGLE,
	SETTINGS_JOB_BT_DEVICE_TOGGLE,
	SETTINGS_JOB_BT_DEVICE_CONFIRM,
	SETTINGS_JOB_BT_DEVICE_FORGET,
	SETTINGS_JOB_POWER_SLEEP_TIMEOUT,
	SETTINGS_JOB_POWER_AUTO_SHUTDOWN_TIMEOUT,
	SETTINGS_JOB_POWER_LID_BEHAVIOR,
	SETTINGS_JOB_POWER_BUTTON_BEHAVIOR,
	SETTINGS_JOB_POWER_BRIGHTNESS,
	SETTINGS_JOB_POWER_VOLUME,
	SETTINGS_JOB_POWER_MUTE,
	SETTINGS_JOB_TIME_SYNC,
	SETTINGS_JOB_TIME_SET,
	SETTINGS_JOB_TIMEZONE_SET,
};

enum settings_bt_device_kind {
	SETTINGS_BT_DEVICE_UNKNOWN,
	SETTINGS_BT_DEVICE_AUDIO,
	SETTINGS_BT_DEVICE_GAMEPAD,
};

enum settings_prompt_type {
	SETTINGS_PROMPT_NONE,
	SETTINGS_PROMPT_BT_PROGRESS,
	SETTINGS_PROMPT_BT_CONFIRM,
	SETTINGS_PROMPT_ERROR,
};

enum settings_custom_input_result {
	SETTINGS_CUSTOM_INPUT_NONE,
	SETTINGS_CUSTOM_INPUT_DIRTY,
	SETTINGS_CUSTOM_INPUT_BACK,
	SETTINGS_CUSTOM_INPUT_CONSUME,
};

///////////////////////////////////////
struct settings_wifi_network {
	char ssid[64];
	char security[24];
	char state[24];
	int connected;
	int secure;
	int known;
	int signal;
};

struct settings_bt_device {
	char addr[18];
	char name[64];
	char state[24];
	int paired;
	int connected;
	int kind;
};

struct settings_prompt {
	int type;
	char title[64];
	char message[128];
	char detail[64];
	char arg[64];
};

struct settings_snapshot {
	uint32_t generation;
	int wifi_enabled;
	int wifi_busy;
	int wifi_scanning;
	int wifi_network_count;
	char wifi_connected_ssid[64];
	int wifi_toggle_target;
	struct settings_wifi_network wifi_networks[SETTINGS_MAX_NETWORKS];

	int bt_enabled;
	int bt_busy;
	int bt_scanning;
	int bt_device_count;
	char bt_connected_name[64];
	int bt_toggle_target;
	struct settings_bt_device bt_devices[SETTINGS_MAX_BT_DEVICES];
	int power_sleep_timeout_ms;
	int power_auto_shutdown_timeout_ms;
	int power_lid_behavior;
	int power_button_behavior;
	struct settings_prompt prompt;

	int ntp_running;
	int ntp_syncing;
	int timezone_offset_minutes;
	char timezone_text[24];
	char release[64];
	char commit[64];
	char model[64];
};

struct settings_item {
	int type;
	int action;
	int enabled;
	int busy;
	int submenu_id;
	int data;
	int extra;
	int index;
	char label[64];
	char desc[128];
	char arg[64];
	struct ui_badge badge;
};

struct settings_job {
	int type;
	int value;
	int timezone_offset_minutes;
	int year;
	int month;
	int day;
	int hour;
	int minute;
	int second;
	char arg[64];
	char secret[128];
};

struct settings_screen;

enum settings_time_row {
	SETTINGS_TIME_ROW_CURRENT_TIME,
	SETTINGS_TIME_ROW_TIMEZONE,
	SETTINGS_TIME_ROW_NTP,
	SETTINGS_TIME_ROW_SYNC,
	SETTINGS_TIME_ROW_CLOCK,
	SETTINGS_TIME_ROW_COUNT,
};

enum settings_time_field {
	SETTINGS_TIME_FIELD_YEAR,
	SETTINGS_TIME_FIELD_MONTH,
	SETTINGS_TIME_FIELD_DAY,
	SETTINGS_TIME_FIELD_HOUR,
	SETTINGS_TIME_FIELD_MINUTE,
	SETTINGS_TIME_FIELD_SECOND,
	SETTINGS_TIME_FIELD_AMPM,
};

struct settings_time_state {
	int timezone_index;
	int show_24hour;
	int selected_row;
	int selected_field;
	int editing_clock;
	int year;
	int month;
	int day;
	int hour;
	int minute;
	int second;
	int dirty;
	int sync_requested;
	int sync_seen_active;
	uint32_t refresh_after;
	time_t last_seen_second;
	SDL_Surface *digits;
};

struct settings_controls_state {
	uint32_t last_pressed;
};

struct settings_menu {
	int id;
	const char *title;
	int (*build)(struct settings_screen *screen,
		const struct settings_snapshot *snapshot,
		struct settings_item *items, int max_items);
	int (*activate)(struct settings_screen *screen,
		const struct settings_item *item, int direction);
	int (*activate_aux)(struct settings_screen *screen,
		const struct settings_item *item);
	void (*get_hint)(struct settings_screen *screen,
		const struct settings_item *item, struct ui_hint *hint);
	void (*get_aux_hint)(struct settings_screen *screen,
		const struct settings_item *item, struct ui_hint *hint);
	void (*enter)(struct settings_screen *screen,
		const struct settings_snapshot *snapshot);
	void (*leave)(struct settings_screen *screen);
	void (*update)(struct settings_screen *screen,
		const struct settings_snapshot *snapshot, int *dirty);
	int (*handle_input)(struct settings_screen *screen);
	void (*draw)(struct settings_screen *screen, SDL_Surface *surface);
	void (*build_hints)(struct settings_screen *screen,
		struct ui_hint_group *top, struct ui_hint_group *bottom);
};

struct settings_screen {
	int open;
	int depth;
	int menu_stack[SETTINGS_MAX_DEPTH];
	int selected[SETTINGS_MAX_DEPTH];
	int start[SETTINGS_MAX_DEPTH];
	int item_count;
	struct settings_item items[SETTINGS_MAX_ITEMS];
	struct settings_snapshot snapshot;
	uint32_t seen_generation;
	time_t last_clock_tick;
	char notice[128];
	uint32_t notice_until;
	struct ui_row view_items[SETTINGS_MAX_ITEMS];
	struct ui_dialog dialog;
	struct settings_time_state time;
	struct settings_controls_state controls;
};

///////////////////////////////////////
static inline void SETTINGS_copyText(char *dst, size_t size,
	const char *src)
{
	if (!dst || !size)
		return;
	if (!src) {
		dst[0] = '\0';
		return;
	}
	snprintf(dst, size, "%s", src);
}

static inline void SETTINGS_initItem(struct settings_item *item, int type,
	int action, const char *label, const char *desc)
{
	memset(item, 0, sizeof(*item));
	item->type = type;
	item->action = action;
	item->enabled = 1;
	item->index = -1;
	SETTINGS_copyText(item->label, sizeof(item->label), label);
	SETTINGS_copyText(item->desc, sizeof(item->desc), desc);
}

static inline void SETTINGS_setNotice(struct settings_screen *screen,
	const char *msg)
{
	if (!screen)
		return;
	SETTINGS_copyText(screen->notice, sizeof(screen->notice), msg);
	screen->notice_until = SDL_GetTicks() + 2500;
}

///////////////////////////////////////
void SETTINGS_init(void);
void SETTINGS_quit(void);
void SETTINGS_open(void);
void SETTINGS_close(void);
int SETTINGS_isOpen(void);
int SETTINGS_handleMenuToggle(int *dirty);
void SETTINGS_update(int *dirty);
void SETTINGS_handleInput(int *dirty, int *quit);
void SETTINGS_buildView(struct ui_list_view *view);
void SETTINGS_draw(SDL_Surface *screen, int show_setting);
const struct ui_dialog *SETTINGS_getDialog(void);

#endif
