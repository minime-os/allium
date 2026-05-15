#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include "settings.h"
#include "jobs.h"
#include "timezone.h"
#include "utils.h"

#define SHOW_24HOUR_FILE USERDATA_PATH "/show_24hour"

#define DIGIT_WIDTH 10
#define DIGIT_HEIGHT 16
#define CHAR_SLASH 10
#define CHAR_COLON 11

///////////////////////////////////////
static int time_editLocked(const struct settings_snapshot *snapshot)
{
	return snapshot && snapshot->ntp_running && PLAT_isOnline();
}

static int time_selectedFieldCount(const struct settings_time_state *state)
{
	if (!state)
		return 0;
	return state->show_24hour ?
		SETTINGS_TIME_FIELD_SECOND - SETTINGS_TIME_FIELD_YEAR + 1 :
		SETTINGS_TIME_FIELD_AMPM - SETTINGS_TIME_FIELD_YEAR + 1;
}

static int time_daysInMonth(int year, int month)
{
	int february_days = 28;

	if (((year % 4 == 0) && (year % 100 != 0)) || (year % 400 == 0))
		february_days = 29;

	switch (month) {
	case 2:
		return february_days;
	case 4:
	case 6:
	case 9:
	case 11:
		return 30;
	default:
		return 31;
	}
}

static void time_validate(struct settings_time_state *state)
{
	int days;

	if (!state)
		return;

	if (state->month > 12)
		state->month -= 12;
	else if (state->month < 1)
		state->month += 12;

	if (state->year > 2100)
		state->year = 2100;
	else if (state->year < 1970)
		state->year = 1970;

	days = time_daysInMonth(state->year, state->month);
	if (state->day > days)
		state->day -= days;
	else if (state->day < 1)
		state->day += days;

	if (state->hour > 23)
		state->hour -= 24;
	else if (state->hour < 0)
		state->hour += 24;
	if (state->minute > 59)
		state->minute -= 60;
	else if (state->minute < 0)
		state->minute += 60;
	if (state->second > 59)
		state->second -= 60;
	else if (state->second < 0)
		state->second += 60;
}

static void time_loadFromEpoch(struct settings_screen *screen, time_t now)
{
	struct settings_time_state *state;
	struct tm tm_now;
	time_t adjusted;
	int offset_minutes;

	if (!screen)
		return;

	state = &screen->time;
	offset_minutes = SETTINGS_TIMEZONE_offsetAt(state->timezone_index);
	adjusted = now + (offset_minutes * 60);
	gmtime_r(&adjusted, &tm_now);

	state->year = tm_now.tm_year + 1900;
	state->month = tm_now.tm_mon + 1;
	state->day = tm_now.tm_mday;
	state->hour = tm_now.tm_hour;
	state->minute = tm_now.tm_min;
	state->second = tm_now.tm_sec;
	state->last_seen_second = now;
	time_validate(state);
}

static void time_syncTimezone(struct settings_screen *screen,
	const struct settings_snapshot *snapshot)
{
	struct settings_time_state *state;

	if (!screen || !snapshot)
		return;

	state = &screen->time;
	state->timezone_index = SETTINGS_TIMEZONE_findIndex(
		snapshot->timezone_offset_minutes);
}

static void time_reset(struct settings_screen *screen,
	const struct settings_snapshot *snapshot)
{
	struct settings_time_state *state;

	if (!screen || !snapshot)
		return;

	state = &screen->time;
	memset(state, 0, sizeof(*state));
	state->timezone_index = SETTINGS_TIMEZONE_findIndex(
		snapshot->timezone_offset_minutes);
	state->show_24hour = exists((char *)SHOW_24HOUR_FILE);
	state->selected_row = SETTINGS_TIME_ROW_CURRENT_TIME;
	state->selected_field = SETTINGS_TIME_FIELD_YEAR;
	time_loadFromEpoch(screen, time(NULL));
}

static int time_ensureDigits(struct settings_screen *screen)
{
	struct settings_time_state *state;
	SDL_Surface *digit;
	SDL_Surface *digits;
	const char *chars[] = { "0", "1", "2", "3", "4", "5", "6",
		"7", "8", "9", "/", ":", NULL };
	int i;

	if (!screen)
		return -EINVAL;

	state = &screen->time;
	if (state->digits)
		return 0;

	digits = SDL_CreateRGBSurface(SDL_SWSURFACE, SCALE2(120, 16),
		FIXED_DEPTH, RGBA_MASK_AUTO);
	if (!digits)
		return -ENOMEM;
	SDL_FillRect(digits, NULL, RGB_BLACK);

	for (i = 0; chars[i]; i++) {
		int y = i == CHAR_COLON ? SCALE1(-1.5) : 0;

		digit = TTF_RenderUTF8_Blended(font.large, chars[i], COLOR_WHITE);
		if (!digit)
			continue;
		SDL_BlitSurface(digit, NULL, digits, &(SDL_Rect){
			(i * SCALE1(DIGIT_WIDTH)) +
				(SCALE1(DIGIT_WIDTH) - digit->w) / 2,
			y + (SCALE1(DIGIT_HEIGHT) - digit->h) / 2,
		});
		SDL_FreeSurface(digit);
	}

	state->digits = digits;
	return 0;
}

static int time_blitDigit(struct settings_time_state *state, SDL_Surface *screen,
	int digit_index, int x, int y)
{
	SDL_BlitSurface(state->digits, &(SDL_Rect){
		digit_index * SCALE1(DIGIT_WIDTH),
		0,
		SCALE2(DIGIT_WIDTH, DIGIT_HEIGHT)
	}, screen, &(SDL_Rect){ x, y });
	return x + SCALE1(DIGIT_WIDTH);
}

static int time_blitNumber(struct settings_time_state *state, SDL_Surface *screen,
	int value, int x, int y)
{
	int digit;

	if (value > 999) {
		digit = value / 1000;
		value -= digit * 1000;
		x = time_blitDigit(state, screen, digit, x, y);
		digit = value / 100;
		value -= digit * 100;
		x = time_blitDigit(state, screen, digit, x, y);
	}

	digit = value / 10;
	value -= digit * 10;
	x = time_blitDigit(state, screen, digit, x, y);
	return time_blitDigit(state, screen, value, x, y);
}

static void time_drawUnderline(SDL_Surface *screen, int x, int y, int width)
{
	GFX_blitPill(ASSET_UNDERLINE, screen, &(SDL_Rect){ x, y, width });
}

static void time_formatSummary(const struct settings_time_state *state,
	char *dst, size_t dst_size)
{
	int hour;

	if (!state || !dst || !dst_size)
		return;

	if (state->show_24hour) {
		snprintf(dst, dst_size, "%04d/%02d/%02d %02d:%02d:%02d",
			state->year, state->month, state->day, state->hour,
			state->minute, state->second);
		return;
	}

	hour = state->hour;
	if (hour == 0)
		hour = 12;
	else if (hour > 12)
		hour -= 12;
	snprintf(dst, dst_size, "%04d/%02d/%02d %02d:%02d:%02d %s",
		state->year, state->month, state->day, hour, state->minute,
		state->second, state->hour < 12 ? "AM" : "PM");
}

static const char *time_ntpState(const struct settings_snapshot *snapshot)
{
	if (!snapshot)
		return "Stopped";
	if (snapshot->ntp_syncing)
		return "Syncing";
	if (snapshot->ntp_running)
		return "Running";
	return "Stopped";
}

static const char *time_syncState(const struct settings_snapshot *snapshot)
{
	if (!snapshot)
		return "Offline";
	return PLAT_isOnline() ? "Press X" : "Offline";
}

static void time_drawStatusRow(const char *label, const char *value, int y,
	int selected, SDL_Surface *screen)
{
	SDL_Color text_color = COLOR_WHITE;
	SDL_Surface *label_text;
	SDL_Surface *value_text;
	SDL_Rect row_rect;
	SDL_Rect value_rect;
	char label_display[32];
	char value_display[48];
	int value_width;

	if (!label || !value || !screen)
		return;

	row_rect.x = SCALE1(PADDING);
	row_rect.y = y;
	row_rect.w = screen->w - SCALE1(PADDING * 2);
	row_rect.h = SCALE1(PILL_SIZE);
	if (selected) {
		GFX_blitPill(ASSET_WHITE_PILL, screen, &row_rect);
		text_color = COLOR_BLACK;
	}

	value_width = row_rect.w / 2;
	GFX_truncateText(font.small, (char *)label, label_display,
		row_rect.w / 2, 0);
	GFX_truncateText(font.small, (char *)value, value_display, value_width, 0);

	label_text = TTF_RenderUTF8_Blended(font.small, label_display, text_color);
	value_text = TTF_RenderUTF8_Blended(font.small, value_display, text_color);
	if (label_text) {
		SDL_BlitSurface(label_text, NULL, screen, &(SDL_Rect){
			row_rect.x + SCALE1(BUTTON_PADDING),
			row_rect.y + (row_rect.h - label_text->h) / 2,
		});
		SDL_FreeSurface(label_text);
	}
	if (!value_text)
		return;

	value_rect.x = row_rect.x + row_rect.w - SCALE1(BUTTON_PADDING) -
		value_text->w;
	value_rect.y = row_rect.y + (row_rect.h - value_text->h) / 2;
	SDL_BlitSurface(value_text, NULL, screen, &value_rect);
	SDL_FreeSurface(value_text);
}

static void time_drawClock(struct settings_screen *screen, SDL_Surface *dst)
{
	struct settings_time_state *state = &screen->time;
	SDL_Surface *ampm_text = NULL;
	int hour;
	int ampm_width = SCALE1(20);
	int option_x;
	int x;
	int y;

	if (!dst || time_ensureDigits(screen) != 0)
		return;

	option_x = state->show_24hour ? SCALE1(188) : SCALE1(223);
	x = (dst->w - option_x) / 2;
	y = SCALE1(SETTINGS_SIZE + PILL_SIZE * SETTINGS_TIME_STATUS_ROWS + 26);
	option_x = x;

	x = time_blitNumber(state, dst, state->year, x, y);
	x = time_blitDigit(state, dst, CHAR_SLASH, x, y);
	x = time_blitNumber(state, dst, state->month, x, y);
	x = time_blitDigit(state, dst, CHAR_SLASH, x, y);
	x = time_blitNumber(state, dst, state->day, x, y);
	x += SCALE1(10);

	hour = state->hour;
	if (!state->show_24hour) {
		if (hour == 0)
			hour = 12;
		else if (hour > 12)
			hour -= 12;
	}
	x = time_blitNumber(state, dst, hour, x, y);
	x = time_blitDigit(state, dst, CHAR_COLON, x, y);
	x = time_blitNumber(state, dst, state->minute, x, y);
	x = time_blitDigit(state, dst, CHAR_COLON, x, y);
	x = time_blitNumber(state, dst, state->second, x, y);

	if (!state->show_24hour) {
		x += SCALE1(10);
		ampm_text = TTF_RenderUTF8_Blended(font.large,
			state->hour < 12 ? "AM" : "PM", COLOR_WHITE);
		if (ampm_text) {
			ampm_width = ampm_text->w + SCALE1(2);
			SDL_BlitSurface(ampm_text, NULL, dst, &(SDL_Rect){
				x,
				y - SCALE1(3),
			});
			SDL_FreeSurface(ampm_text);
		}
	}

	y += SCALE1(19);
	x = option_x;
	if (state->selected_row != SETTINGS_TIME_ROW_CLOCK)
		return;

	switch (state->selected_field) {
	case SETTINGS_TIME_FIELD_YEAR:
		time_drawUnderline(dst, x, y, SCALE1(40));
		return;
	case SETTINGS_TIME_FIELD_MONTH:
		x += SCALE1(50);
		time_drawUnderline(dst, x, y, SCALE1(20));
		return;
	case SETTINGS_TIME_FIELD_DAY:
		x += SCALE1(80);
		time_drawUnderline(dst, x, y, SCALE1(20));
		return;
	case SETTINGS_TIME_FIELD_HOUR:
		x += SCALE1(110);
		time_drawUnderline(dst, x, y, SCALE1(20));
		return;
	case SETTINGS_TIME_FIELD_MINUTE:
		x += SCALE1(140);
		time_drawUnderline(dst, x, y, SCALE1(20));
		return;
	case SETTINGS_TIME_FIELD_SECOND:
		x += SCALE1(170);
		time_drawUnderline(dst, x, y, SCALE1(20));
		return;
	case SETTINGS_TIME_FIELD_AMPM:
		x += SCALE1(200);
		time_drawUnderline(dst, x, y, ampm_width);
		return;
	default:
		return;
	}
}

static void time_drawNotice(struct settings_screen *screen, SDL_Surface *dst)
{
	static char msg[] = "Manual time edit disabled while NTP is running.";
	SDL_Rect rect;

	if (!screen || !dst || !time_editLocked(&screen->snapshot))
		return;

	rect.x = SCALE1(PADDING);
	rect.y = dst->h - SCALE1(PILL_SIZE * 2);
	rect.w = dst->w - SCALE1(PADDING * 2);
	rect.h = SCALE1(PILL_SIZE);
	GFX_blitText(font.tiny, msg, SCALE1(12), COLOR_LIGHT_TEXT, dst, &rect);
}

static int time_set24Hour(struct settings_time_state *state, int show_24hour)
{
	if (!state)
		return 0;
	if (!!state->show_24hour == !!show_24hour)
		return 0;

	state->show_24hour = !!show_24hour;
	if (state->show_24hour)
		touch((char *)SHOW_24HOUR_FILE);
	else
		unlink(SHOW_24HOUR_FILE);
	if (state->selected_field >= time_selectedFieldCount(state))
		state->selected_field = SETTINGS_TIME_FIELD_SECOND;
	return 1;
}

static int time_moveRow(struct settings_time_state *state, int delta)
{
	int next;

	if (!state)
		return 0;

	next = state->selected_row + delta;
	while (next < 0)
		next += SETTINGS_TIME_ROW_COUNT;
	while (next >= SETTINGS_TIME_ROW_COUNT)
		next -= SETTINGS_TIME_ROW_COUNT;
	state->selected_row = next;
	state->editing_clock = 0;
	return 1;
}

static int time_moveField(struct settings_time_state *state, int delta)
{
	int count;
	int next;

	if (!state)
		return 0;

	count = time_selectedFieldCount(state);
	next = state->selected_field + delta;
	while (next < 0)
		next += count;
	while (next >= count)
		next -= count;
	state->selected_field = next;
	return 1;
}

static int time_adjustTimezone(struct settings_screen *screen, int delta)
{
	struct settings_time_state *state;
	int count;
	int next;
	int rc;

	if (!screen)
		return 0;

	state = &screen->time;
	count = SETTINGS_TIMEZONE_count();
	next = state->timezone_index + delta;

	while (next < 0)
		next += count;
	while (next >= count)
		next -= count;
	if (next == state->timezone_index)
		return 0;
	state->timezone_index = next;
	time_loadFromEpoch(screen, time(NULL));
	state->dirty = 0;
	state->refresh_after = SDL_GetTicks() + 500;
	rc = SETTINGS_JOBS_enqueueTimezoneSet(
		SETTINGS_TIMEZONE_offsetAt(state->timezone_index));
	if (rc != 0)
		SETTINGS_setNotice(screen, "Timezone update queued failed");
	return 1;
}

static int time_adjustField(struct settings_screen *screen, int delta)
{
	struct settings_time_state *state;

	if (!screen)
		return 0;

	state = &screen->time;
	if (time_editLocked(&screen->snapshot))
		return 0;

	switch (state->selected_field) {
	case SETTINGS_TIME_FIELD_YEAR:
		state->year += delta;
		break;
	case SETTINGS_TIME_FIELD_MONTH:
		state->month += delta;
		break;
	case SETTINGS_TIME_FIELD_DAY:
		state->day += delta;
		break;
	case SETTINGS_TIME_FIELD_HOUR:
		state->hour += delta;
		break;
	case SETTINGS_TIME_FIELD_MINUTE:
		state->minute += delta;
		break;
	case SETTINGS_TIME_FIELD_SECOND:
		state->second += delta;
		break;
	case SETTINGS_TIME_FIELD_AMPM:
		state->hour += delta > 0 ? 12 : -12;
		break;
	default:
		return 0;
	}

	time_validate(state);
	state->dirty = 1;
	return 1;
}

static int time_triggerSync(struct settings_screen *screen);
static int time_setClock(struct settings_screen *screen);

static int time_activateRow(struct settings_screen *screen)
{
	struct settings_time_state *state;

	if (!screen)
		return 0;

	state = &screen->time;
	switch (state->selected_row) {
	case SETTINGS_TIME_ROW_SYNC:
		return time_triggerSync(screen);
	case SETTINGS_TIME_ROW_CLOCK:
		if (time_editLocked(&screen->snapshot))
			return SETTINGS_CUSTOM_INPUT_CONSUME;
		if (!state->editing_clock) {
			state->editing_clock = 1;
			return SETTINGS_CUSTOM_INPUT_DIRTY;
		}
		return time_setClock(screen) ? SETTINGS_CUSTOM_INPUT_DIRTY :
			SETTINGS_CUSTOM_INPUT_NONE;
	default:
		return SETTINGS_CUSTOM_INPUT_NONE;
	}
}

static int time_triggerSync(struct settings_screen *screen)
{
	int rc;

	if (!screen)
		return 0;
	if (!PLAT_isOnline()) {
		SETTINGS_setNotice(screen, "Network required for time sync");
		return 1;
	}

	rc = SETTINGS_JOBS_enqueue(SETTINGS_JOB_TIME_SYNC, 0, NULL);
	if (rc != 0) {
		SETTINGS_setNotice(screen, "Clock sync queued failed");
		return 1;
	}

	screen->time.sync_requested = 1;
	screen->time.sync_seen_active = 0;
	return 1;
}

static int time_setClock(struct settings_screen *screen)
{
	struct settings_time_state *state;
	int rc;

	if (!screen)
		return 0;
	if (time_editLocked(&screen->snapshot))
		return 0;

	state = &screen->time;
	rc = SETTINGS_JOBS_enqueueTimeSet(state->year, state->month, state->day,
		state->hour, state->minute, state->second);
	if (rc != 0) {
		SETTINGS_setNotice(screen, "Time update queued failed");
		return 1;
	}

	state->refresh_after = SDL_GetTicks() + 1500;
	return 1;
}

///////////////////////////////////////
void SETTINGS_TIME_enter(struct settings_screen *screen,
	const struct settings_snapshot *snapshot)
{
	time_reset(screen, snapshot);
}

void SETTINGS_TIME_update(struct settings_screen *screen,
	const struct settings_snapshot *snapshot, int *dirty)
{
	struct settings_time_state *state;
	time_t now;

	if (!screen || !snapshot)
		return;

	state = &screen->time;
	now = time(NULL);

	if (state->sync_requested) {
		if (snapshot->ntp_syncing)
			state->sync_seen_active = 1;
		else if (state->sync_seen_active) {
			time_syncTimezone(screen, snapshot);
			time_loadFromEpoch(screen, now);
			state->dirty = 0;
			state->sync_requested = 0;
			state->sync_seen_active = 0;
			if (dirty)
				*dirty = 1;
			return;
		}
	}

	if (state->refresh_after &&
			(int32_t)(SDL_GetTicks() - state->refresh_after) >= 0) {
		time_syncTimezone(screen, snapshot);
		time_loadFromEpoch(screen, now);
		state->dirty = 0;
		state->refresh_after = 0;
		if (dirty)
			*dirty = 1;
		return;
	}

	if (state->dirty)
		return;
	if (state->timezone_index != SETTINGS_TIMEZONE_findIndex(
			snapshot->timezone_offset_minutes)) {
		time_syncTimezone(screen, snapshot);
		time_loadFromEpoch(screen, now);
		if (dirty)
			*dirty = 1;
		return;
	}
	if (now != state->last_seen_second) {
		time_loadFromEpoch(screen, now);
		if (dirty)
			*dirty = 1;
	}
}

int SETTINGS_TIME_handleInput(struct settings_screen *screen)
{
	struct settings_time_state *state;

	if (!screen)
		return 0;

	state = &screen->time;
	if (state->editing_clock && PAD_justPressed(BTN_B)) {
		state->editing_clock = 0;
		return SETTINGS_CUSTOM_INPUT_DIRTY;
	}
	if (PAD_justRepeated(BTN_UP)) {
		if (state->editing_clock)
			return time_adjustField(screen, 1) ?
				SETTINGS_CUSTOM_INPUT_DIRTY :
				SETTINGS_CUSTOM_INPUT_NONE;
		return time_moveRow(state, -1) ? SETTINGS_CUSTOM_INPUT_DIRTY :
			SETTINGS_CUSTOM_INPUT_NONE;
	}
	if (PAD_justRepeated(BTN_DOWN)) {
		if (state->editing_clock)
			return time_adjustField(screen, -1) ?
				SETTINGS_CUSTOM_INPUT_DIRTY :
				SETTINGS_CUSTOM_INPUT_NONE;
		return time_moveRow(state, 1) ? SETTINGS_CUSTOM_INPUT_DIRTY :
			SETTINGS_CUSTOM_INPUT_NONE;
	}
	if (PAD_justRepeated(BTN_LEFT)) {
		if (state->selected_row == SETTINGS_TIME_ROW_TIMEZONE)
			return time_adjustTimezone(screen, -1) ?
				SETTINGS_CUSTOM_INPUT_DIRTY :
				SETTINGS_CUSTOM_INPUT_NONE;
		if (state->selected_row == SETTINGS_TIME_ROW_CLOCK &&
				state->editing_clock)
			return time_moveField(state, -1) ?
				SETTINGS_CUSTOM_INPUT_DIRTY :
				SETTINGS_CUSTOM_INPUT_NONE;
		return SETTINGS_CUSTOM_INPUT_NONE;
	}
	if (PAD_justRepeated(BTN_RIGHT)) {
		if (state->selected_row == SETTINGS_TIME_ROW_TIMEZONE)
			return time_adjustTimezone(screen, 1) ?
				SETTINGS_CUSTOM_INPUT_DIRTY :
				SETTINGS_CUSTOM_INPUT_NONE;
		if (state->selected_row == SETTINGS_TIME_ROW_CLOCK &&
				state->editing_clock)
			return time_moveField(state, 1) ?
				SETTINGS_CUSTOM_INPUT_DIRTY :
				SETTINGS_CUSTOM_INPUT_NONE;
		return SETTINGS_CUSTOM_INPUT_NONE;
	}
	if (PAD_justPressed(BTN_SELECT))
		return time_set24Hour(state, !state->show_24hour) ?
			SETTINGS_CUSTOM_INPUT_DIRTY :
			SETTINGS_CUSTOM_INPUT_NONE;
	if (PAD_justPressed(BTN_X))
		return time_triggerSync(screen) ? SETTINGS_CUSTOM_INPUT_DIRTY :
			SETTINGS_CUSTOM_INPUT_NONE;
	if (PAD_justPressed(BTN_A))
		return time_activateRow(screen);
	return SETTINGS_CUSTOM_INPUT_NONE;
}

void SETTINGS_TIME_draw(struct settings_screen *screen, SDL_Surface *dst)
{
	struct settings_time_state *state;
	char time_text[40];
	char timezone_text[SETTINGS_TIMEZONE_TEXT_SIZE];
	int y;

	if (!screen || !dst)
		return;

	state = &screen->time;
	time_formatSummary(state, time_text, sizeof(time_text));
	SETTINGS_TIMEZONE_format(SETTINGS_TIMEZONE_offsetAt(state->timezone_index),
		timezone_text, sizeof(timezone_text));

	y = SCALE1(PADDING);
	time_drawStatusRow("Current Time:", time_text, y,
		state->selected_row == SETTINGS_TIME_ROW_CURRENT_TIME, dst);
	y += SCALE1(PILL_SIZE);
	time_drawStatusRow("Current Timezone:", timezone_text, y,
		state->selected_row == SETTINGS_TIME_ROW_TIMEZONE, dst);
	y += SCALE1(PILL_SIZE);
	time_drawStatusRow("NTP:", time_ntpState(&screen->snapshot), y,
		state->selected_row == SETTINGS_TIME_ROW_NTP, dst);
	y += SCALE1(PILL_SIZE);
	time_drawStatusRow("Sync Clock Now", time_syncState(&screen->snapshot), y,
		state->selected_row == SETTINGS_TIME_ROW_SYNC, dst);

	time_drawClock(screen, dst);
	time_drawNotice(screen, dst);
}

void SETTINGS_TIME_buildHints(struct settings_screen *screen,
	struct ui_hint_group *top, struct ui_hint_group *bottom)
{
	struct settings_time_state *state;

	if (!screen || !top || !bottom)
		return;

	state = &screen->time;
	memset(top, 0, sizeof(*top));
	memset(bottom, 0, sizeof(*bottom));

	top->count = 2;
	top->primary = 0;
	top->align_right = 0;
	top->hints[0].button = UI_BUTTON_ID_SELECT;
	top->hints[0].text = state->show_24hour ? "12 HOUR" : "24 HOUR";
	top->hints[1].button = UI_BUTTON_ID_X;
	top->hints[1].text = "SYNC";

	bottom->count = 1;
	bottom->primary = 0;
	bottom->align_right = 1;
	bottom->hints[0].button = UI_BUTTON_ID_B;
	bottom->hints[0].label = UI_HINT_LABEL_BACK;
	if (state->selected_row == SETTINGS_TIME_ROW_SYNC) {
		bottom->count = 2;
		bottom->hints[1].button = UI_BUTTON_ID_A;
		bottom->hints[1].text = "SYNC";
	} else if (state->selected_row == SETTINGS_TIME_ROW_CLOCK &&
			!time_editLocked(&screen->snapshot)) {
		bottom->count = 2;
		bottom->primary = 1;
		bottom->hints[1].button = UI_BUTTON_ID_A;
		bottom->hints[1].text = state->editing_clock ? "SET" : "EDIT";
	}
}
