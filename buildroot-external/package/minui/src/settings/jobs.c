#define _GNU_SOURCE

#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#include <msettings.h>

#include "settings.h"
#include "bt_backend.h"
#include "jobs.h"
#include "timezone.h"
#include "wifi_backend.h"
#include "utils.h"

/* Shared service state stays under /system/* because early boot services use it. */
#define WIFI_STATE_FILE SDCARD_PATH "/system/wifi/wifi.conf"
#define BT_STATE_FILE SDCARD_PATH "/system/bluetooth/bluetooth.conf"
#define POWER_POLICY_FILE USERDATA_PATH "/power.conf"
#define TIMEZONE_STATE_FILE SDCARD_PATH "/system/time/timezone.conf"
#define NTP_HELPER "/etc/init.d/S45ntpd"
#define LOCALTIME_PATH "/etc/localtime"
#define LOCALTIME_TMP_PATH "/etc/localtime.tmp"
#define TIMEZONE_NAME_PATH "/etc/timezone"
#define TIMEZONE_NAME_TMP_PATH "/etc/timezone.tmp"

#define SETTINGS_POLL_MS 400
#define SETTINGS_WIFI_SCAN_MS 4000
#define SETTINGS_BT_BOOT_SCAN_MS 8000
#define SETTINGS_PENDING_MIN_MS 1200
#define SETTINGS_WIFI_PENDING_MS 12000
#define SETTINGS_BT_PENDING_MS 30000

///////////////////////////////////////
struct settings_jobs_state {
	pthread_t thread;
	pthread_mutex_t lock;
	pthread_cond_t cond;
	int running;
	int active_menu;
	int head;
	int tail;
	int count;
	struct settings_job queue[SETTINGS_MAX_JOBS];
	struct settings_snapshot snapshot;
	int wifi_boot_scan_done;
	int bt_boot_scan_done;
	uint32_t wifi_scan_after;
	uint32_t bt_boot_scan_until;
	char wifi_pending_ssid[64];
	char wifi_pending_state[24];
	uint32_t wifi_pending_started;
	uint32_t wifi_pending_until;
	char bt_pending_addr[18];
	char bt_pending_state[24];
	uint32_t bt_pending_started;
	uint32_t bt_pending_until;
};

static struct settings_jobs_state jobs;

///////////////////////////////////////
static void jobs_trim(char *str)
{
	size_t len;

	if (!str)
		return;
	while (*str == ' ' || *str == '\t' || *str == '\n')
		memmove(str, str + 1, strlen(str));
	len = strlen(str);
	while (len > 0 && (str[len - 1] == ' ' || str[len - 1] == '\t' ||
			str[len - 1] == '\n')) {
		str[len - 1] = '\0';
		len--;
	}
}

static int jobs_queue_full(void)
{
	return jobs.count >= SETTINGS_MAX_JOBS;
}

static int jobs_queue_pop(struct settings_job *job)
{
	if (!job || jobs.count <= 0)
		return 0;
	*job = jobs.queue[jobs.head];
	memset(&jobs.queue[jobs.head], 0, sizeof(jobs.queue[jobs.head]));
	jobs.head = (jobs.head + 1) % SETTINGS_MAX_JOBS;
	jobs.count--;
	return 1;
}

static void jobs_clear_prompt_locked(void)
{
	memset(&jobs.snapshot.prompt, 0, sizeof(jobs.snapshot.prompt));
}

static void jobs_set_prompt_locked(int type, const char *title,
	const char *message, const char *detail, const char *arg)
{
	struct settings_prompt *prompt = &jobs.snapshot.prompt;

	memset(prompt, 0, sizeof(*prompt));
	prompt->type = type;
	SETTINGS_copyText(prompt->title, sizeof(prompt->title), title);
	SETTINGS_copyText(prompt->message, sizeof(prompt->message), message);
	SETTINGS_copyText(prompt->detail, sizeof(prompt->detail), detail);
	SETTINGS_copyText(prompt->arg, sizeof(prompt->arg), arg);
}

static void jobs_set_error_prompt(const char *title, const char *message,
	const char *detail)
{
	pthread_mutex_lock(&jobs.lock);
	jobs_set_prompt_locked(SETTINGS_PROMPT_ERROR, title, message, detail,
		NULL);
	pthread_mutex_unlock(&jobs.lock);
}

static void jobs_wifi_pending_clear(void)
{
	jobs.wifi_pending_ssid[0] = '\0';
	jobs.wifi_pending_state[0] = '\0';
	jobs.wifi_pending_started = 0;
	jobs.wifi_pending_until = 0;
}

static void jobs_wifi_pending_set(const char *ssid, const char *state)
{
	if (!ssid || !ssid[0] || !state || !state[0])
		return;
	SETTINGS_copyText(jobs.wifi_pending_ssid,
		sizeof(jobs.wifi_pending_ssid), ssid);
	SETTINGS_copyText(jobs.wifi_pending_state,
		sizeof(jobs.wifi_pending_state), state);
	jobs.wifi_pending_started = SDL_GetTicks();
	jobs.wifi_pending_until = SDL_GetTicks() + SETTINGS_WIFI_PENDING_MS;
}

static void jobs_bt_pending_clear(void)
{
	jobs.bt_pending_addr[0] = '\0';
	jobs.bt_pending_state[0] = '\0';
	jobs.bt_pending_started = 0;
	jobs.bt_pending_until = 0;
}

static void jobs_bt_pending_set(const char *addr, const char *state)
{
	if (!addr || !addr[0] || !state || !state[0])
		return;
	SETTINGS_copyText(jobs.bt_pending_addr,
		sizeof(jobs.bt_pending_addr), addr);
	SETTINGS_copyText(jobs.bt_pending_state,
		sizeof(jobs.bt_pending_state), state);
	jobs.bt_pending_started = SDL_GetTicks();
	jobs.bt_pending_until = SDL_GetTicks() + SETTINGS_BT_PENDING_MS;
}

static const char *jobs_wifi_pending_for(const char *ssid)
{
	uint32_t now;

	if (!ssid || !ssid[0] || !jobs.wifi_pending_ssid[0] ||
			strcmp(ssid, jobs.wifi_pending_ssid))
		return NULL;
	now = SDL_GetTicks();
	if (!jobs.wifi_pending_until ||
			(int32_t)(now - jobs.wifi_pending_until) >= 0) {
		jobs_wifi_pending_clear();
		return NULL;
	}
	return jobs.wifi_pending_state;
}

static const char *jobs_bt_pending_for(const char *addr)
{
	uint32_t now;

	if (!addr || !addr[0] || !jobs.bt_pending_addr[0] ||
			strcmp(addr, jobs.bt_pending_addr))
		return NULL;
	now = SDL_GetTicks();
	if (!jobs.bt_pending_until ||
			(int32_t)(now - jobs.bt_pending_until) >= 0) {
		jobs_bt_pending_clear();
		return NULL;
	}
	return jobs.bt_pending_state;
}

static int jobs_pending_min_elapsed(uint32_t started)
{
	uint32_t now;

	if (!started)
		return 1;
	now = SDL_GetTicks();
	return (int32_t)(now - (started + SETTINGS_PENDING_MIN_MS)) >= 0;
}

static int jobs_wifi_should_clear_pending(const struct settings_wifi_network *net)
{
	if (!net || !jobs.wifi_pending_state[0])
		return 0;
	if (!strcmp(jobs.wifi_pending_state, "Connecting"))
		return net->connected && jobs_pending_min_elapsed(
			jobs.wifi_pending_started);
	if (!strcmp(jobs.wifi_pending_state, "Disconnecting"))
		return !net->connected && jobs_pending_min_elapsed(
			jobs.wifi_pending_started);
	return 0;
}

static int jobs_bt_should_clear_pending(const struct settings_bt_device *dev)
{
	if (!dev || !jobs.bt_pending_state[0])
		return 0;
	if (!strcmp(jobs.bt_pending_state, "Connecting"))
		return dev->connected && jobs_pending_min_elapsed(
			jobs.bt_pending_started);
	if (!strcmp(jobs.bt_pending_state, "Pairing"))
		return dev->paired && jobs_pending_min_elapsed(
			jobs.bt_pending_started);
	if (!strcmp(jobs.bt_pending_state, "Disconnecting"))
		return !dev->connected && jobs_pending_min_elapsed(
			jobs.bt_pending_started);
	return 0;
}

static int jobs_find_wifi_network(const struct settings_snapshot *snapshot,
	const char *ssid)
{
	int i;

	if (!snapshot || !ssid || !ssid[0])
		return -1;
	for (i = 0; i < snapshot->wifi_network_count; i++) {
		if (!strcmp(snapshot->wifi_networks[i].ssid, ssid))
			return i;
	}
	return -1;
}

static int jobs_find_bt_device(const struct settings_snapshot *snapshot,
	const char *addr)
{
	int i;

	if (!snapshot || !addr || !addr[0])
		return -1;
	for (i = 0; i < snapshot->bt_device_count; i++) {
		if (!strcmp(snapshot->bt_devices[i].addr, addr))
			return i;
	}
	return -1;
}

static void jobs_merge_cached_wifi(struct settings_snapshot *snapshot,
	const struct settings_snapshot *current)
{
	int i;

	if (!snapshot || !current)
		return;
	for (i = 0; i < current->wifi_network_count &&
			snapshot->wifi_network_count < SETTINGS_MAX_NETWORKS; i++) {
		if (jobs_find_wifi_network(snapshot,
				current->wifi_networks[i].ssid) >= 0)
			continue;
		snapshot->wifi_networks[snapshot->wifi_network_count++] =
			current->wifi_networks[i];
	}
}

static void jobs_merge_cached_bt(struct settings_snapshot *snapshot,
	const struct settings_snapshot *current)
{
	int i;

	if (!snapshot || !current)
		return;
	for (i = 0; i < current->bt_device_count &&
			snapshot->bt_device_count < SETTINGS_MAX_BT_DEVICES; i++) {
		if (jobs_find_bt_device(snapshot, current->bt_devices[i].addr) >= 0)
			continue;
		snapshot->bt_devices[snapshot->bt_device_count++] =
			current->bt_devices[i];
	}
}

static void jobs_publish_wifi_pending(const char *ssid, const char *state)
{
	int i;

	if (!ssid || !ssid[0] || !state || !state[0])
		return;

	pthread_mutex_lock(&jobs.lock);
	for (i = 0; i < jobs.snapshot.wifi_network_count; i++) {
		if (strcmp(jobs.snapshot.wifi_networks[i].ssid, ssid))
			continue;
		SETTINGS_copyText(jobs.snapshot.wifi_networks[i].state,
			sizeof(jobs.snapshot.wifi_networks[i].state), state);
		jobs.snapshot.generation++;
		break;
	}
	pthread_mutex_unlock(&jobs.lock);
}

static void jobs_publish_bt_pending(const char *addr, const char *state)
{
	int i;

	if (!addr || !addr[0] || !state || !state[0])
		return;

	pthread_mutex_lock(&jobs.lock);
	for (i = 0; i < jobs.snapshot.bt_device_count; i++) {
		if (strcmp(jobs.snapshot.bt_devices[i].addr, addr))
			continue;
		SETTINGS_copyText(jobs.snapshot.bt_devices[i].state,
			sizeof(jobs.snapshot.bt_devices[i].state), state);
		jobs.snapshot.generation++;
		break;
	}
	pthread_mutex_unlock(&jobs.lock);
}

static int jobs_ensure_state_dir(const char *path)
{
	if (!path)
		return -EINVAL;
	if (!strncmp(path, SDCARD_PATH "/system/wifi/",
			strlen(SDCARD_PATH "/system/wifi/"))) {
		(void)mkdir(SDCARD_PATH "/system", 0755);
		(void)mkdir(SDCARD_PATH "/system/wifi", 0755);
		return 0;
	}
	if (!strncmp(path, SDCARD_PATH "/system/bluetooth/",
			strlen(SDCARD_PATH "/system/bluetooth/"))) {
		(void)mkdir(SDCARD_PATH "/system", 0755);
		(void)mkdir(SDCARD_PATH "/system/bluetooth", 0755);
		return 0;
	}
	if (!strncmp(path, SDCARD_PATH "/system/time/",
			strlen(SDCARD_PATH "/system/time/"))) {
		(void)mkdir(SDCARD_PATH "/system", 0755);
		(void)mkdir(SDCARD_PATH "/system/time", 0755);
		return 0;
	}
	if (!strncmp(path, SDCARD_PATH "/system/minui/",
			strlen(SDCARD_PATH "/system/minui/"))) {
		(void)mkdir(SDCARD_PATH "/system", 0755);
		(void)mkdir(SDCARD_PATH "/system/minui", 0755);
		return 0;
	}
	return 0;
}

static int jobs_write_state_int(const char *path, const char *key, int value)
{
	char tmp[320];
	FILE *file;
	int fd;

	if (!path || !key)
		return -EINVAL;
	jobs_ensure_state_dir(path);
	snprintf(tmp, sizeof(tmp), "%s.tmp", path);
	file = fopen(tmp, "w");
	if (!file)
		return -errno;
	fprintf(file, "%s=%d\n", key, value ? 1 : 0);
	fflush(file);
	fd = fileno(file);
	if (fd >= 0)
		fsync(fd);
	fclose(file);
	if (rename(tmp, path) != 0) {
		unlink(tmp);
		return -errno;
	}
	return 0;
}

static int jobs_write_state_number(const char *path, const char *key, int value)
{
	char tmp[320];
	FILE *file;
	int fd;

	if (!path || !key)
		return -EINVAL;
	jobs_ensure_state_dir(path);
	snprintf(tmp, sizeof(tmp), "%s.tmp", path);
	file = fopen(tmp, "w");
	if (!file)
		return -errno;
	fprintf(file, "%s=%d\n", key, value);
	fflush(file);
	fd = fileno(file);
	if (fd >= 0)
		fsync(fd);
	fclose(file);
	if (rename(tmp, path) != 0) {
		unlink(tmp);
		return -errno;
	}
	return 0;
}

static int jobs_read_state_number(const char *path, const char *key, int *value)
{
	char buffer[128];
	char *line;
	char *saveptr = NULL;
	size_t key_len;

	if (!path || !key || !value)
		return -EINVAL;

	buffer[0] = '\0';
	getFile((char *)path, buffer, sizeof(buffer));
	if (!buffer[0])
		return -ENOENT;

	key_len = strlen(key);
	line = strtok_r(buffer, "\n", &saveptr);
	while (line) {
		if (!strncmp(line, key, key_len) && line[key_len] == '=') {
			*value = atoi(line + key_len + 1);
			return 0;
		}
		line = strtok_r(NULL, "\n", &saveptr);
	}
	return -ENOENT;
}

static int jobs_read_timezone_offset(void)
{
	int offset = 0;

	if (jobs_read_state_number(TIMEZONE_STATE_FILE,
			"gmt_offset_minutes", &offset) != 0)
		return SETTINGS_TIMEZONE_DEFAULT_OFFSET_MINUTES;
	if (!SETTINGS_TIMEZONE_isValidOffset(offset))
		return SETTINGS_TIMEZONE_DEFAULT_OFFSET_MINUTES;
	return offset;
}

static int jobs_apply_timezone(int offset_minutes)
{
	char zone_id[SETTINGS_TIMEZONE_ZONE_ID_SIZE];
	char target[64];
	char display[SETTINGS_TIMEZONE_TEXT_SIZE];
	FILE *file;
	int fd;

	if (!SETTINGS_TIMEZONE_isValidOffset(offset_minutes))
		offset_minutes = SETTINGS_TIMEZONE_DEFAULT_OFFSET_MINUTES;
	SETTINGS_TIMEZONE_formatZoneId(offset_minutes, zone_id, sizeof(zone_id));
	snprintf(target, sizeof(target), "../usr/share/zoneinfo/%s", zone_id);
	if (access(LOCALTIME_TMP_PATH, F_OK) == 0)
		unlink(LOCALTIME_TMP_PATH);
	if (symlink(target, LOCALTIME_TMP_PATH) != 0)
		return -errno;
	if (rename(LOCALTIME_TMP_PATH, LOCALTIME_PATH) != 0) {
		unlink(LOCALTIME_TMP_PATH);
		return -errno;
	}

	SETTINGS_TIMEZONE_format(offset_minutes, display, sizeof(display));
	file = fopen(TIMEZONE_NAME_TMP_PATH, "w");
	if (!file)
		return -errno;
	fprintf(file, "%s\n", display);
	fflush(file);
	fd = fileno(file);
	if (fd >= 0)
		fsync(fd);
	fclose(file);
	if (rename(TIMEZONE_NAME_TMP_PATH, TIMEZONE_NAME_PATH) != 0) {
		unlink(TIMEZONE_NAME_TMP_PATH);
		return -errno;
	}

	unsetenv("TZ");
	tzset();
	return 0;
}

static int jobs_write_power_policy(void)
{
	char tmp[320];
	FILE *file;
	int fd;

	jobs_ensure_state_dir(POWER_POLICY_FILE);
	snprintf(tmp, sizeof(tmp), "%s.tmp", POWER_POLICY_FILE);
	file = fopen(tmp, "w");
	if (!file)
		return -errno;

	fprintf(file, "sleep_timeout_ms=%d\n", PWR_getSleepTimeoutMs());
	fprintf(file, "auto_shutdown_timeout_ms=%d\n",
		PWR_getAutoShutdownTimeoutMs());
	fprintf(file, "lid_behavior=%d\n", PWR_getLidBehavior());
	fprintf(file, "power_button_behavior=%d\n",
		PWR_getPowerButtonBehavior());
	fflush(file);
	fd = fileno(file);
	if (fd >= 0)
		fsync(fd);
	fclose(file);
	if (rename(tmp, POWER_POLICY_FILE) != 0) {
		unlink(tmp);
		return -errno;
	}
	return 0;
}

static int jobs_pidfile_exists(const char *path)
{
	return path && access(path, F_OK) == 0;
}

static void jobs_parse_release(struct settings_snapshot *snapshot)
{
	char buffer[256];
	char *release;
	char *commit;
	char *nl;

	if (!snapshot)
		return;

	buffer[0] = '\0';
	getFile(ROOT_SYSTEM_PATH "/version.txt", buffer, sizeof(buffer));
	trimTrailingNewlines(buffer);
	release = buffer;
	commit = buffer;
	nl = strchr(buffer, '\n');
	if (nl) {
		*nl = '\0';
		commit = nl + 1;
		nl = strchr(commit, '\n');
		if (nl)
			*nl = '\0';
	}
	jobs_trim(release);
	jobs_trim(commit);
	if (!release[0])
		release = "unknown";
	if (!commit[0])
		commit = "unknown";
	SETTINGS_copyText(snapshot->release, sizeof(snapshot->release), release);
	SETTINGS_copyText(snapshot->commit, sizeof(snapshot->commit), commit);
	SETTINGS_copyText(snapshot->model, sizeof(snapshot->model),
		PLAT_getModel());
}

static void jobs_fill_timezone(struct settings_snapshot *snapshot)
{
	int offset_minutes;

	if (!snapshot)
		return;

	offset_minutes = jobs_read_timezone_offset();
	snapshot->timezone_offset_minutes = offset_minutes;
	SETTINGS_TIMEZONE_format(offset_minutes, snapshot->timezone_text,
		sizeof(snapshot->timezone_text));
}

static void jobs_apply_pending(struct settings_snapshot *snapshot)
{
	int i;
	const char *pending;

	if (!snapshot)
		return;

	for (i = 0; i < snapshot->wifi_network_count; i++) {
		pending = jobs_wifi_pending_for(snapshot->wifi_networks[i].ssid);
		if (pending) {
			if (jobs_wifi_should_clear_pending(
					&snapshot->wifi_networks[i])) {
				jobs_wifi_pending_clear();
			} else {
			SETTINGS_copyText(snapshot->wifi_networks[i].state,
				sizeof(snapshot->wifi_networks[i].state), pending);
			}
		}
	}

	for (i = 0; i < snapshot->bt_device_count; i++) {
		pending = jobs_bt_pending_for(snapshot->bt_devices[i].addr);
		if (pending) {
			if (jobs_bt_should_clear_pending(&snapshot->bt_devices[i])) {
				jobs_bt_pending_clear();
			} else {
			SETTINGS_copyText(snapshot->bt_devices[i].state,
				sizeof(snapshot->bt_devices[i].state), pending);
			}
		}
	}
}

static void jobs_refresh_snapshot(void)
{
	struct settings_snapshot current;
	struct settings_snapshot next;
	uint32_t generation;

	pthread_mutex_lock(&jobs.lock);
	current = jobs.snapshot;
	pthread_mutex_unlock(&jobs.lock);

	memset(&next, 0, sizeof(next));
	next.prompt = current.prompt;
	next.ntp_syncing = current.ntp_syncing;
	jobs_parse_release(&next);
	jobs_fill_timezone(&next);
	next.ntp_running = jobs_pidfile_exists("/run/ntpd.pid") ||
		jobs_pidfile_exists("/var/run/ntpd.pid");
	next.power_sleep_timeout_ms = PWR_getSleepTimeoutMs();
	next.power_auto_shutdown_timeout_ms = PWR_getAutoShutdownTimeoutMs();
	next.power_lid_behavior = PWR_getLidBehavior();
	next.power_button_behavior = PWR_getPowerButtonBehavior();

	(void)SETTINGS_WIFI_BACKEND_refresh(&next);
	(void)SETTINGS_BT_BACKEND_refresh(&next);
	jobs_merge_cached_wifi(&next, &current);
	jobs_merge_cached_bt(&next, &current);
	jobs_apply_pending(&next);

	pthread_mutex_lock(&jobs.lock);
	generation = jobs.snapshot.generation;
	next.generation = generation;
	if (memcmp(&next, &jobs.snapshot, sizeof(next)) != 0)
		next.generation = generation + 1;
	jobs.snapshot = next;
	pthread_mutex_unlock(&jobs.lock);
}

static void jobs_handle_wifi_toggle(const struct settings_job *job)
{
	int enabled_now;

	pthread_mutex_lock(&jobs.lock);
	enabled_now = jobs.snapshot.wifi_enabled;
	pthread_mutex_unlock(&jobs.lock);
	if (!!enabled_now == !!job->value)
		return;

	if (SETTINGS_WIFI_BACKEND_set_enabled(job->value) != 0) {
		jobs_set_error_prompt("Wi-Fi", "Wi-Fi change failed.", NULL);
		return;
	}
	(void)jobs_write_state_int(WIFI_STATE_FILE, "wifi_enabled",
		job->value ? 1 : 0);
	jobs.wifi_scan_after = 0;
	if (!job->value)
		jobs_wifi_pending_clear();
	jobs.wifi_boot_scan_done = 0;
}

static void jobs_handle_wifi_connect(const struct settings_job *job)
{
	char connected[64];

	if (!job->arg[0])
		return;

	pthread_mutex_lock(&jobs.lock);
	SETTINGS_copyText(connected, sizeof(connected),
		jobs.snapshot.wifi_connected_ssid);
	pthread_mutex_unlock(&jobs.lock);
	if (connected[0] && !strcmp(connected, job->arg))
		return;

	jobs_wifi_pending_set(job->arg, "Connecting");
	jobs_publish_wifi_pending(job->arg, "Connecting");
	if (SETTINGS_WIFI_BACKEND_connect(job->arg,
			job->secret[0] ? job->secret : NULL, 0) != 0) {
		jobs_wifi_pending_clear();
		jobs_set_error_prompt("Wi-Fi", "Wi-Fi connect failed.", job->arg);
		return;
	}
	(void)jobs_write_state_int(WIFI_STATE_FILE, "wifi_enabled", 1);
}

static void jobs_handle_wifi_disconnect(const struct settings_job *job)
{
	if (job->arg[0])
		jobs_wifi_pending_set(job->arg, "Disconnecting");
	jobs_publish_wifi_pending(job->arg, "Disconnecting");
	if (SETTINGS_WIFI_BACKEND_disconnect() != 0) {
		jobs_wifi_pending_clear();
		jobs_set_error_prompt("Wi-Fi", "Wi-Fi disconnect failed.", NULL);
	}
}

static void jobs_handle_wifi_forget(const struct settings_job *job)
{
	if (!job->arg[0])
		return;
	if (SETTINGS_WIFI_BACKEND_forget(job->arg) != 0)
		jobs_set_error_prompt("Wi-Fi", "Wi-Fi forget failed.", job->arg);
}

static void jobs_handle_bt_toggle(const struct settings_job *job)
{
	int enabled_now;

	pthread_mutex_lock(&jobs.lock);
	enabled_now = jobs.snapshot.bt_enabled;
	pthread_mutex_unlock(&jobs.lock);
	if (!!enabled_now == !!job->value)
		return;

	if (SETTINGS_BT_BACKEND_set_enabled(job->value) != 0) {
		jobs_set_error_prompt("Bluetooth", "Bluetooth change failed.",
			NULL);
		return;
	}
	(void)jobs_write_state_int(BT_STATE_FILE, "bluetooth_enabled",
		job->value ? 1 : 0);
	if (!job->value)
		jobs_bt_pending_clear();
	jobs.bt_boot_scan_done = 0;
	jobs.bt_boot_scan_until = 0;
}

static void jobs_handle_bt_toggle_device(const struct settings_job *job)
{
	const char *pending = "Pairing";

	if (job->value & SETTINGS_FLAG_BT_CONNECTED)
		pending = "Disconnecting";
	else if (job->value & SETTINGS_FLAG_BT_PAIRED)
		pending = "Connecting";
	jobs_bt_pending_set(job->arg, pending);
	jobs_publish_bt_pending(job->arg, pending);
	if (SETTINGS_BT_BACKEND_toggle_device(job->arg) != 0) {
		jobs_bt_pending_clear();
		jobs_set_error_prompt("Bluetooth",
			"Bluetooth device action failed.", job->arg);
	}
}

static void jobs_handle_bt_confirm(const struct settings_job *job)
{
	if (SETTINGS_BT_BACKEND_confirm_device(job->arg, 1) != 0)
		jobs_set_error_prompt("Bluetooth",
			"Bluetooth confirmation unsupported.", job->arg);
}

static void jobs_handle_bt_forget(const struct settings_job *job)
{
	if (SETTINGS_BT_BACKEND_forget_device(job->arg) != 0)
		jobs_set_error_prompt("Bluetooth",
			"Bluetooth forget failed.", job->arg);
}

static void jobs_handle_time_sync(void)
{
	int rc;

	pthread_mutex_lock(&jobs.lock);
	jobs.snapshot.ntp_syncing = 1;
	jobs.snapshot.generation++;
	pthread_mutex_unlock(&jobs.lock);

	rc = system(NTP_HELPER " restart >/dev/null 2>&1");

	pthread_mutex_lock(&jobs.lock);
	jobs.snapshot.ntp_syncing = 0;
	jobs.snapshot.generation++;
	pthread_mutex_unlock(&jobs.lock);
	if (rc != 0)
		jobs_set_error_prompt("Time", "Clock sync failed.", NULL);
}

static void jobs_handle_time_set(const struct settings_job *job)
{
	if (PLAT_setDateTime(job->year, job->month, job->day, job->hour,
			job->minute, job->second) != 0) {
		jobs_set_error_prompt("Time", "Time update failed.", NULL);
		return;
	}

	pthread_mutex_lock(&jobs.lock);
	jobs.snapshot.generation++;
	pthread_mutex_unlock(&jobs.lock);
}

static void jobs_handle_timezone_set(const struct settings_job *job)
{
	if (!SETTINGS_TIMEZONE_isValidOffset(job->timezone_offset_minutes)) {
		jobs_set_error_prompt("Time", "Timezone update failed.", NULL);
		return;
	}
	if (jobs_write_state_number(TIMEZONE_STATE_FILE, "gmt_offset_minutes",
			job->timezone_offset_minutes) != 0) {
		jobs_set_error_prompt("Time", "Timezone update failed.", NULL);
		return;
	}

	if (jobs_apply_timezone(job->timezone_offset_minutes) != 0) {
		jobs_set_error_prompt("Time", "Timezone update failed.", NULL);
		return;
	}
	pthread_mutex_lock(&jobs.lock);
	jobs.snapshot.timezone_offset_minutes = job->timezone_offset_minutes;
	SETTINGS_TIMEZONE_format(job->timezone_offset_minutes,
		jobs.snapshot.timezone_text, sizeof(jobs.snapshot.timezone_text));
	jobs.snapshot.generation++;
	pthread_mutex_unlock(&jobs.lock);
}

static void jobs_handle_power_sleep_timeout(const struct settings_job *job)
{
	if (PWR_setSleepTimeoutMs(job->value) != 0 ||
			jobs_write_power_policy() != 0)
		jobs_set_error_prompt("Power", "Sleep timeout update failed.",
			NULL);
}

static void jobs_handle_power_auto_shutdown_timeout(
	const struct settings_job *job)
{
	if (PWR_setAutoShutdownTimeoutMs(job->value) != 0 ||
			jobs_write_power_policy() != 0)
		jobs_set_error_prompt("Power",
			"Auto shutdown timeout update failed.", NULL);
}

static void jobs_handle_power_lid_behavior(const struct settings_job *job)
{
	if (PWR_setLidBehavior(job->value) != 0 ||
			jobs_write_power_policy() != 0)
		jobs_set_error_prompt("Power", "Lid behavior update failed.",
			NULL);
}

static void jobs_handle_power_button_behavior(const struct settings_job *job)
{
	if (PWR_setPowerButtonBehavior(job->value) != 0 ||
			jobs_write_power_policy() != 0)
		jobs_set_error_prompt("Power",
			"Power button behavior update failed.", NULL);
}

static void jobs_handle_job(const struct settings_job *job)
{
	if (!job)
		return;

	pthread_mutex_lock(&jobs.lock);
	jobs_clear_prompt_locked();
	pthread_mutex_unlock(&jobs.lock);

	switch (job->type) {
	case SETTINGS_JOB_WIFI_TOGGLE:
		jobs_handle_wifi_toggle(job);
		break;
	case SETTINGS_JOB_WIFI_CONNECT:
		jobs_handle_wifi_connect(job);
		break;
	case SETTINGS_JOB_WIFI_DISCONNECT:
		jobs_handle_wifi_disconnect(job);
		break;
	case SETTINGS_JOB_WIFI_FORGET:
		jobs_handle_wifi_forget(job);
		break;
	case SETTINGS_JOB_BT_TOGGLE:
		jobs_handle_bt_toggle(job);
		break;
	case SETTINGS_JOB_BT_DEVICE_TOGGLE:
		jobs_handle_bt_toggle_device(job);
		break;
	case SETTINGS_JOB_BT_DEVICE_CONFIRM:
		jobs_handle_bt_confirm(job);
		break;
	case SETTINGS_JOB_BT_DEVICE_FORGET:
		jobs_handle_bt_forget(job);
		break;
	case SETTINGS_JOB_POWER_SLEEP_TIMEOUT:
		jobs_handle_power_sleep_timeout(job);
		break;
	case SETTINGS_JOB_POWER_AUTO_SHUTDOWN_TIMEOUT:
		jobs_handle_power_auto_shutdown_timeout(job);
		break;
	case SETTINGS_JOB_POWER_LID_BEHAVIOR:
		jobs_handle_power_lid_behavior(job);
		break;
	case SETTINGS_JOB_POWER_BUTTON_BEHAVIOR:
		jobs_handle_power_button_behavior(job);
		break;
	case SETTINGS_JOB_POWER_BRIGHTNESS:
		SetBrightness(job->value);
		break;
	case SETTINGS_JOB_POWER_VOLUME:
		SetVolume(job->value);
		break;
	case SETTINGS_JOB_POWER_MUTE:
		SetMute(job->value);
		break;
	case SETTINGS_JOB_TIME_SYNC:
		jobs_handle_time_sync();
		break;
	case SETTINGS_JOB_TIME_SET:
		jobs_handle_time_set(job);
		break;
	case SETTINGS_JOB_TIMEZONE_SET:
		jobs_handle_timezone_set(job);
		break;
	default:
		break;
	}
}

static int jobs_sync_active_scanning(const struct settings_snapshot *snapshot)
{
	uint32_t now;
	int rc;

	if (!snapshot)
		return 0;

	now = SDL_GetTicks();

	if (jobs.active_menu == SETTINGS_MENU_WIFI && snapshot->wifi_enabled) {
		if (!snapshot->wifi_scanning &&
				(!jobs.wifi_scan_after ||
				(int32_t)(now - jobs.wifi_scan_after) >= 0)) {
			rc = SETTINGS_WIFI_BACKEND_set_scanning(1);
			jobs.wifi_scan_after = now + SETTINGS_WIFI_SCAN_MS;
			if (rc != 0)
				jobs_set_error_prompt("Wi-Fi",
					"Wi-Fi scan failed.", NULL);
			else
				return 1;
		}
	} else if (!jobs.wifi_boot_scan_done && snapshot->wifi_enabled) {
		rc = SETTINGS_WIFI_BACKEND_set_scanning(1);
		if (rc == 0) {
			jobs.wifi_boot_scan_done = 1;
			return 1;
		}
	} else {
		jobs.wifi_scan_after = 0;
		(void)SETTINGS_WIFI_BACKEND_set_scanning(0);
	}

	if (jobs.active_menu == SETTINGS_MENU_BT && snapshot->bt_enabled) {
		if (!snapshot->bt_scanning) {
			rc = SETTINGS_BT_BACKEND_set_scanning(1);
			if (rc != 0)
				jobs_set_error_prompt("Bluetooth",
					"Bluetooth scan failed.", NULL);
			else
				return 1;
		}
	} else if (!jobs.bt_boot_scan_done && snapshot->bt_enabled) {
		if (!jobs.bt_boot_scan_until) {
			rc = SETTINGS_BT_BACKEND_set_scanning(1);
			if (rc != 0) {
				jobs.bt_boot_scan_done = 1;
				jobs_set_error_prompt("Bluetooth",
					"Bluetooth scan failed.", NULL);
			} else {
				jobs.bt_boot_scan_until = now +
					SETTINGS_BT_BOOT_SCAN_MS;
				return 1;
			}
		} else if ((int32_t)(now - jobs.bt_boot_scan_until) >= 0) {
			jobs.bt_boot_scan_done = 1;
			jobs.bt_boot_scan_until = 0;
			if (snapshot->bt_scanning) {
				rc = SETTINGS_BT_BACKEND_set_scanning(0);
				if (rc != 0)
					jobs_set_error_prompt("Bluetooth",
						"Bluetooth stop scan failed.", NULL);
				else
					return 1;
			}
		}
	} else if (snapshot->bt_scanning) {
		jobs.bt_boot_scan_until = 0;
		rc = SETTINGS_BT_BACKEND_set_scanning(0);
		if (rc != 0)
			jobs_set_error_prompt("Bluetooth",
				"Bluetooth stop scan failed.", NULL);
		else
			return 1;
	}

	return 0;
}

static void *settings_jobs_worker(void *arg)
{
	struct settings_job job;
	struct settings_snapshot snapshot;
	struct timespec ts;
	int has_job;
	long wait_ms;

	(void)arg;

	(void)SETTINGS_WIFI_BACKEND_init();
	(void)SETTINGS_BT_BACKEND_init();

	pthread_mutex_lock(&jobs.lock);
	while (jobs.running) {
		has_job = jobs_queue_pop(&job);
		if (!has_job) {
			clock_gettime(CLOCK_REALTIME, &ts);
			wait_ms = (jobs.active_menu == SETTINGS_MENU_WIFI ||
				jobs.active_menu == SETTINGS_MENU_BT) ?
				SETTINGS_POLL_MS : 1000;
			ts.tv_nsec += (wait_ms % 1000) * 1000000L;
			ts.tv_sec += wait_ms / 1000 + ts.tv_nsec / 1000000000L;
			ts.tv_nsec %= 1000000000L;
			pthread_cond_timedwait(&jobs.cond, &jobs.lock, &ts);
			if (!jobs.running)
				break;
			memset(&job, 0, sizeof(job));
		}
		pthread_mutex_unlock(&jobs.lock);

		if (has_job)
			jobs_handle_job(&job);
		jobs_refresh_snapshot();
		pthread_mutex_lock(&jobs.lock);
		snapshot = jobs.snapshot;
		pthread_mutex_unlock(&jobs.lock);
		if (jobs_sync_active_scanning(&snapshot))
			jobs_refresh_snapshot();

		pthread_mutex_lock(&jobs.lock);
	}
	pthread_mutex_unlock(&jobs.lock);

	SETTINGS_BT_BACKEND_quit();
	SETTINGS_WIFI_BACKEND_quit();
	return NULL;
}

void SETTINGS_JOBS_init(void)
{
	memset(&jobs, 0, sizeof(jobs));
	pthread_mutex_init(&jobs.lock, NULL);
	pthread_cond_init(&jobs.cond, NULL);
	jobs.running = 1;
	(void)jobs_apply_timezone(jobs_read_timezone_offset());
	jobs_refresh_snapshot();
	pthread_create(&jobs.thread, NULL, settings_jobs_worker, NULL);
}

void SETTINGS_JOBS_quit(void)
{
	pthread_mutex_lock(&jobs.lock);
	jobs.running = 0;
	pthread_cond_signal(&jobs.cond);
	pthread_mutex_unlock(&jobs.lock);
	pthread_join(jobs.thread, NULL);
	pthread_mutex_destroy(&jobs.lock);
	pthread_cond_destroy(&jobs.cond);
}

void SETTINGS_JOBS_setActiveMenu(int menu_id)
{
	pthread_mutex_lock(&jobs.lock);
	jobs.active_menu = menu_id;
	pthread_cond_signal(&jobs.cond);
	pthread_mutex_unlock(&jobs.lock);
}

uint32_t SETTINGS_JOBS_copySnapshot(struct settings_snapshot *snapshot)
{
	uint32_t generation;

	pthread_mutex_lock(&jobs.lock);
	if (snapshot)
		*snapshot = jobs.snapshot;
	generation = jobs.snapshot.generation;
	pthread_mutex_unlock(&jobs.lock);
	return generation;
}

int SETTINGS_JOBS_enqueue(int job_type, int value, const char *arg)
{
	struct settings_job *job;

	pthread_mutex_lock(&jobs.lock);
	if (jobs_queue_full()) {
		pthread_mutex_unlock(&jobs.lock);
		return -ENOSPC;
	}

	job = &jobs.queue[jobs.tail];
	memset(job, 0, sizeof(*job));
	job->type = job_type;
	job->value = value;
	SETTINGS_copyText(job->arg, sizeof(job->arg), arg);
	jobs.tail = (jobs.tail + 1) % SETTINGS_MAX_JOBS;
	jobs.count++;
	pthread_cond_signal(&jobs.cond);
	pthread_mutex_unlock(&jobs.lock);
	return 0;
}

int SETTINGS_JOBS_enqueueWifiConnect(const char *ssid, const char *passphrase)
{
	struct settings_job *job;

	pthread_mutex_lock(&jobs.lock);
	if (jobs_queue_full()) {
		pthread_mutex_unlock(&jobs.lock);
		return -ENOSPC;
	}

	job = &jobs.queue[jobs.tail];
	memset(job, 0, sizeof(*job));
	job->type = SETTINGS_JOB_WIFI_CONNECT;
	SETTINGS_copyText(job->arg, sizeof(job->arg), ssid);
	SETTINGS_copyText(job->secret, sizeof(job->secret), passphrase);
	jobs.tail = (jobs.tail + 1) % SETTINGS_MAX_JOBS;
	jobs.count++;
	pthread_cond_signal(&jobs.cond);
	pthread_mutex_unlock(&jobs.lock);
	return 0;
}

int SETTINGS_JOBS_enqueueTimeSet(int year, int month, int day, int hour,
	int minute, int second)
{
	struct settings_job *job;

	pthread_mutex_lock(&jobs.lock);
	if (jobs_queue_full()) {
		pthread_mutex_unlock(&jobs.lock);
		return -ENOSPC;
	}

	job = &jobs.queue[jobs.tail];
	memset(job, 0, sizeof(*job));
	job->type = SETTINGS_JOB_TIME_SET;
	job->year = year;
	job->month = month;
	job->day = day;
	job->hour = hour;
	job->minute = minute;
	job->second = second;
	jobs.tail = (jobs.tail + 1) % SETTINGS_MAX_JOBS;
	jobs.count++;
	pthread_cond_signal(&jobs.cond);
	pthread_mutex_unlock(&jobs.lock);
	return 0;
}

int SETTINGS_JOBS_enqueueTimezoneSet(int timezone_offset_minutes)
{
	struct settings_job *job;
	int i;
	int index;

	pthread_mutex_lock(&jobs.lock);
	for (i = 0; i < jobs.count; i++) {
		index = (jobs.head + i) % SETTINGS_MAX_JOBS;
		if (jobs.queue[index].type != SETTINGS_JOB_TIMEZONE_SET)
			continue;
		jobs.queue[index].timezone_offset_minutes = timezone_offset_minutes;
		pthread_cond_signal(&jobs.cond);
		pthread_mutex_unlock(&jobs.lock);
		return 0;
	}
	if (jobs_queue_full()) {
		pthread_mutex_unlock(&jobs.lock);
		return -ENOSPC;
	}

	job = &jobs.queue[jobs.tail];
	memset(job, 0, sizeof(*job));
	job->type = SETTINGS_JOB_TIMEZONE_SET;
	job->timezone_offset_minutes = timezone_offset_minutes;
	jobs.tail = (jobs.tail + 1) % SETTINGS_MAX_JOBS;
	jobs.count++;
	pthread_cond_signal(&jobs.cond);
	pthread_mutex_unlock(&jobs.lock);
	return 0;
}

void SETTINGS_JOBS_clearPrompt(void)
{
	pthread_mutex_lock(&jobs.lock);
	jobs_clear_prompt_locked();
	jobs.snapshot.generation++;
	pthread_mutex_unlock(&jobs.lock);
}
