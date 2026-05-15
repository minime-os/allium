#ifndef __SETTINGS_JOBS_H__
#define __SETTINGS_JOBS_H__

#include "settings.h"

///////////////////////////////////////
void SETTINGS_JOBS_init(void);
void SETTINGS_JOBS_quit(void);
void SETTINGS_JOBS_setActiveMenu(int menu_id);
uint32_t SETTINGS_JOBS_copySnapshot(struct settings_snapshot *snapshot);
int SETTINGS_JOBS_enqueue(int job_type, int value, const char *arg);
int SETTINGS_JOBS_enqueueWifiConnect(const char *ssid,
	const char *passphrase);
int SETTINGS_JOBS_enqueueTimeSet(int year, int month, int day, int hour,
	int minute, int second);
int SETTINGS_JOBS_enqueueTimezoneSet(int timezone_offset_minutes);
void SETTINGS_JOBS_clearPrompt(void);

#endif
