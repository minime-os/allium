#ifndef __SETTINGS_WIFI_BACKEND_H__
#define __SETTINGS_WIFI_BACKEND_H__

#include "settings.h"

///////////////////////////////////////
int SETTINGS_WIFI_BACKEND_init(void);
void SETTINGS_WIFI_BACKEND_quit(void);
int SETTINGS_WIFI_BACKEND_refresh(struct settings_snapshot *snapshot);
int SETTINGS_WIFI_BACKEND_set_enabled(int enabled);
int SETTINGS_WIFI_BACKEND_set_scanning(int enabled);
int SETTINGS_WIFI_BACKEND_connect(const char *ssid, const char *passphrase,
	int hidden);
int SETTINGS_WIFI_BACKEND_disconnect(void);
int SETTINGS_WIFI_BACKEND_forget(const char *ssid);

#endif
