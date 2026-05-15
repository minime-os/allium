#include <stdio.h>

#include "timezone.h"

///////////////////////////////////////
static const int timezone_offsets[] = {
	-720,
	-660,
	-600,
	-570,
	-540,
	-480,
	-420,
	-360,
	-300,
	-240,
	-210,
	-180,
	-150,
	-120,
	-60,
	0,
	60,
	120,
	180,
	210,
	240,
	270,
	300,
	330,
	345,
	360,
	390,
	420,
	480,
	525,
	540,
	570,
	600,
	630,
	660,
	720,
	765,
	780,
	825,
	840,
};

///////////////////////////////////////
static void timezone_formatOffset(int offset_minutes, char *dst, size_t dst_size)
{
	char sign;
	int hours;
	int minutes;

	if (!dst || !dst_size)
		return;

	sign = offset_minutes >= 0 ? '+' : '-';
	if (offset_minutes < 0)
		offset_minutes = -offset_minutes;
	hours = offset_minutes / 60;
	minutes = offset_minutes % 60;
	snprintf(dst, dst_size, "%c%02d:%02d", sign, hours, minutes);
}

///////////////////////////////////////
int SETTINGS_TIMEZONE_count(void)
{
	return sizeof(timezone_offsets) / sizeof(timezone_offsets[0]);
}

int SETTINGS_TIMEZONE_offsetAt(int index)
{
	if (index < 0 || index >= SETTINGS_TIMEZONE_count())
		return 0;
	return timezone_offsets[index];
}

int SETTINGS_TIMEZONE_findIndex(int offset_minutes)
{
	int i;

	for (i = 0; i < SETTINGS_TIMEZONE_count(); i++) {
		if (timezone_offsets[i] == offset_minutes)
			return i;
	}
	for (i = 0; i < SETTINGS_TIMEZONE_count(); i++) {
		if (timezone_offsets[i] == 0)
			return i;
	}
	return 0;
}

int SETTINGS_TIMEZONE_isValidOffset(int offset_minutes)
{
	int i;

	for (i = 0; i < SETTINGS_TIMEZONE_count(); i++) {
		if (timezone_offsets[i] == offset_minutes)
			return 1;
	}
	return 0;
}

void SETTINGS_TIMEZONE_format(int offset_minutes, char *dst, size_t dst_size)
{
	char offset[8];

	if (!dst || !dst_size)
		return;

	timezone_formatOffset(offset_minutes, offset, sizeof(offset));
	snprintf(dst, dst_size, "GMT %s", offset);
}

void SETTINGS_TIMEZONE_formatPosix(int offset_minutes, char *dst,
	size_t dst_size)
{
	char label[8];
	char posix_offset[8];

	if (!dst || !dst_size)
		return;

	timezone_formatOffset(offset_minutes, label, sizeof(label));
	timezone_formatOffset(-offset_minutes, posix_offset,
		sizeof(posix_offset));
	snprintf(dst, dst_size, "<GMT%s>%s", label, posix_offset);
}

void SETTINGS_TIMEZONE_formatZoneId(int offset_minutes, char *dst,
	size_t dst_size)
{
	char sign;
	int hours;
	int minutes;

	if (!dst || !dst_size)
		return;
	if (!SETTINGS_TIMEZONE_isValidOffset(offset_minutes))
		offset_minutes = SETTINGS_TIMEZONE_DEFAULT_OFFSET_MINUTES;

	sign = offset_minutes >= 0 ? '+' : '-';
	if (offset_minutes < 0)
		offset_minutes = -offset_minutes;
	hours = offset_minutes / 60;
	minutes = offset_minutes % 60;
	snprintf(dst, dst_size, "minui/GMT%c%02d_%02d", sign, hours, minutes);
}
