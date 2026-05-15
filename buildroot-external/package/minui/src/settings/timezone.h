#ifndef __SETTINGS_TIMEZONE_H__
#define __SETTINGS_TIMEZONE_H__

#include <stddef.h>

#define SETTINGS_TIMEZONE_TEXT_SIZE 16
#define SETTINGS_TIMEZONE_POSIX_SIZE 32
#define SETTINGS_TIMEZONE_ZONE_ID_SIZE 24
#define SETTINGS_TIMEZONE_DEFAULT_OFFSET_MINUTES 180

///////////////////////////////////////
int SETTINGS_TIMEZONE_count(void);
int SETTINGS_TIMEZONE_offsetAt(int index);
int SETTINGS_TIMEZONE_findIndex(int offset_minutes);
int SETTINGS_TIMEZONE_isValidOffset(int offset_minutes);
void SETTINGS_TIMEZONE_format(int offset_minutes, char *dst, size_t dst_size);
void SETTINGS_TIMEZONE_formatPosix(int offset_minutes, char *dst,
	size_t dst_size);
void SETTINGS_TIMEZONE_formatZoneId(int offset_minutes, char *dst,
	size_t dst_size);

#endif
