#ifndef __UI_DIALOG_H__
#define __UI_DIALOG_H__

#include "list.h"
#include "keyboard.h"

enum ui_dialog_type {
	UI_DIALOG_NONE,
	UI_DIALOG_TEXT_ENTRY,
	UI_DIALOG_WIFI_PASSPHRASE,
	UI_DIALOG_PROGRESS,
	UI_DIALOG_CONFIRM,
	UI_DIALOG_ERROR,
};

enum ui_dialog_result {
	UI_DIALOG_RESULT_NONE,
	UI_DIALOG_RESULT_UPDATE,
	UI_DIALOG_RESULT_CONFIRM,
	UI_DIALOG_RESULT_CANCEL,
	UI_DIALOG_RESULT_CLOSE,
};

///////////////////////////////////////
struct ui_dialog {
	int open;
	int type;
	int choice;
	char title[64];
	char message[128];
	char detail[64];
	char arg[64];
	int allow_empty;
	struct ui_keyboard keyboard;
};

///////////////////////////////////////
void UI_DIALOG_init(struct ui_dialog *dialog);
void UI_DIALOG_close(struct ui_dialog *dialog);
int UI_DIALOG_isOpen(const struct ui_dialog *dialog);
void UI_DIALOG_openTextEntry(struct ui_dialog *dialog, const char *title,
	const char *detail, int allow_empty);
void UI_DIALOG_openWifiPassphrase(struct ui_dialog *dialog, const char *ssid);
void UI_DIALOG_openProgress(struct ui_dialog *dialog, const char *title,
	const char *message, const char *detail, const char *arg);
void UI_DIALOG_openConfirm(struct ui_dialog *dialog, const char *title,
	const char *message, const char *detail, const char *arg);
void UI_DIALOG_openError(struct ui_dialog *dialog, const char *title,
	const char *message, const char *detail);
int UI_DIALOG_handleInput(struct ui_dialog *dialog);
void UI_DIALOG_draw(const struct ui_dialog *dialog, SDL_Surface *screen);

#endif
