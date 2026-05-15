#ifndef __SETTINGS_BT_BACKEND_H__
#define __SETTINGS_BT_BACKEND_H__

#include "settings.h"

///////////////////////////////////////
int SETTINGS_BT_BACKEND_init(void);
void SETTINGS_BT_BACKEND_quit(void);
int SETTINGS_BT_BACKEND_refresh(struct settings_snapshot *snapshot);
int SETTINGS_BT_BACKEND_set_enabled(int enabled);
int SETTINGS_BT_BACKEND_set_scanning(int enabled);
int SETTINGS_BT_BACKEND_toggle_device(const char *addr);
int SETTINGS_BT_BACKEND_forget_device(const char *addr);
int SETTINGS_BT_BACKEND_confirm_device(const char *addr, int accept);

#endif
