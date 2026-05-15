#include <dbus/dbus.h>

#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#include "wifi_backend.h"

#define WIFI_SERVICE "net.connman.iwd"
#define WIFI_ROOT "/"
#define WIFI_OBJMGR_IFACE "org.freedesktop.DBus.ObjectManager"
#define WIFI_PROPS_IFACE "org.freedesktop.DBus.Properties"
#define WIFI_DEVICE_IFACE "net.connman.iwd.Device"
#define WIFI_STATION_IFACE "net.connman.iwd.Station"
#define WIFI_NETWORK_IFACE "net.connman.iwd.Network"
#define WIFI_KNOWN_IFACE "net.connman.iwd.KnownNetwork"

#define WIFI_MAX_PATH 192
#define WIFI_MAX_CACHE_NETWORKS 32
#define WIFI_MAX_CACHE_KNOWN 32
#define WIFI_PROFILE_DIR "/var/lib/iwd"

///////////////////////////////////////
struct wifi_known {
	char ssid[64];
	char path[WIFI_MAX_PATH];
	int secure;
};

struct wifi_network {
	char ssid[64];
	char path[WIFI_MAX_PATH];
	char known_path[WIFI_MAX_PATH];
	int connected;
	int known;
	int secure;
	int signal;
};

struct wifi_backend_state {
	DBusConnection *conn;
	char device_path[WIFI_MAX_PATH];
	char station_path[WIFI_MAX_PATH];
	char connected_path[WIFI_MAX_PATH];
	char connected_ssid[64];
	char forgotten_ssid[64];
	char station_state[24];
	int powered;
	int scanning;
	struct wifi_network networks[WIFI_MAX_CACHE_NETWORKS];
	int network_count;
	struct wifi_known known[WIFI_MAX_CACHE_KNOWN];
	int known_count;
};

static struct wifi_backend_state wifi_state;

///////////////////////////////////////
static void wifi_reset_cache(void)
{
	memset(wifi_state.device_path, 0, sizeof(wifi_state.device_path));
	memset(wifi_state.station_path, 0, sizeof(wifi_state.station_path));
	memset(wifi_state.connected_path, 0, sizeof(wifi_state.connected_path));
	memset(wifi_state.connected_ssid, 0, sizeof(wifi_state.connected_ssid));
	memset(wifi_state.station_state, 0, sizeof(wifi_state.station_state));
	wifi_state.powered = 0;
	wifi_state.scanning = 0;
	memset(wifi_state.networks, 0, sizeof(wifi_state.networks));
	wifi_state.network_count = 0;
	memset(wifi_state.known, 0, sizeof(wifi_state.known));
	wifi_state.known_count = 0;
}

static int wifi_is_forgotten_ssid(const char *ssid)
{
	return ssid && ssid[0] && wifi_state.forgotten_ssid[0] &&
		!strcmp(wifi_state.forgotten_ssid, ssid);
}

static void wifi_mark_forgotten_ssid(const char *ssid)
{
	SETTINGS_copyText(wifi_state.forgotten_ssid,
		sizeof(wifi_state.forgotten_ssid), ssid);
}

static void wifi_clear_forgotten_ssid(const char *ssid)
{
	if (!ssid || !ssid[0] || wifi_is_forgotten_ssid(ssid))
		wifi_state.forgotten_ssid[0] = '\0';
}

static DBusMessage *wifi_call(DBusMessage *msg, int timeout_ms)
{
	DBusError err;
	DBusMessage *reply;

	if (!wifi_state.conn || !msg)
		return NULL;

	dbus_error_init(&err);
	reply = dbus_connection_send_with_reply_and_block(wifi_state.conn, msg,
		timeout_ms, &err);
	dbus_message_unref(msg);
	if (dbus_error_is_set(&err)) {
		dbus_error_free(&err);
		if (reply)
			dbus_message_unref(reply);
		return NULL;
	}
	return reply;
}

static DBusMessage *wifi_call_noarg(const char *path, const char *iface,
	const char *method, int timeout_ms)
{
	DBusMessage *msg;

	if (!path || !iface || !method)
		return NULL;
	msg = dbus_message_new_method_call(WIFI_SERVICE, path, iface, method);
	if (!msg)
		return NULL;
	return wifi_call(msg, timeout_ms);
}

static DBusMessage *wifi_call_string(const char *path, const char *iface,
	const char *method, const char *value, int timeout_ms)
{
	DBusMessage *msg;

	if (!path || !iface || !method || !value)
		return NULL;
	msg = dbus_message_new_method_call(WIFI_SERVICE, path, iface, method);
	if (!msg)
		return NULL;
	if (!dbus_message_append_args(msg, DBUS_TYPE_STRING, &value,
			DBUS_TYPE_INVALID)) {
		dbus_message_unref(msg);
		return NULL;
	}
	return wifi_call(msg, timeout_ms);
}

static int wifi_reply_ok(DBusMessage *reply)
{
	int ok;

	if (!reply)
		return 0;
	ok = dbus_message_get_type(reply) != DBUS_MESSAGE_TYPE_ERROR;
	dbus_message_unref(reply);
	return ok;
}

static int wifi_variant_bool(DBusMessageIter *variant, int *value)
{
	DBusMessageIter inner;
	dbus_bool_t tmp;

	if (!variant || !value ||
			dbus_message_iter_get_arg_type(variant) != DBUS_TYPE_VARIANT)
		return 0;
	dbus_message_iter_recurse(variant, &inner);
	if (dbus_message_iter_get_arg_type(&inner) != DBUS_TYPE_BOOLEAN)
		return 0;
	dbus_message_iter_get_basic(&inner, &tmp);
	*value = tmp ? 1 : 0;
	return 1;
}

static int wifi_variant_string(DBusMessageIter *variant, char *value,
	size_t value_size)
{
	DBusMessageIter inner;
	const char *tmp;
	int type;

	if (!variant || !value ||
			dbus_message_iter_get_arg_type(variant) != DBUS_TYPE_VARIANT)
		return 0;
	dbus_message_iter_recurse(variant, &inner);
	type = dbus_message_iter_get_arg_type(&inner);
	if (type != DBUS_TYPE_STRING && type != DBUS_TYPE_OBJECT_PATH)
		return 0;
	dbus_message_iter_get_basic(&inner, &tmp);
	SETTINGS_copyText(value, value_size, tmp);
	return 1;
}

static int wifi_variant_signal(DBusMessageIter *variant, int *value)
{
	DBusMessageIter inner;
	int type;

	if (!variant || !value ||
			dbus_message_iter_get_arg_type(variant) != DBUS_TYPE_VARIANT)
		return 0;
	dbus_message_iter_recurse(variant, &inner);
	type = dbus_message_iter_get_arg_type(&inner);
	if (type == DBUS_TYPE_INT16) {
		dbus_int16_t tmp;

		dbus_message_iter_get_basic(&inner, &tmp);
		*value = (int)tmp;
		return 1;
	}
	if (type == DBUS_TYPE_INT32) {
		dbus_int32_t tmp;

		dbus_message_iter_get_basic(&inner, &tmp);
		*value = (int)tmp;
		return 1;
	}
	if (type == DBUS_TYPE_BYTE) {
		unsigned char tmp;

		dbus_message_iter_get_basic(&inner, &tmp);
		*value = (int)tmp;
		return 1;
	}
	return 0;
}

static int wifi_signal_percent(int raw_signal)
{
	int dbm;

	dbm = raw_signal;
	if (dbm <= -200)
		dbm /= 100;
	if (dbm >= -45)
		return 100;
	if (dbm <= -90)
		return 0;
	return (dbm + 90) * 100 / 45;
}

static int wifi_is_secure_type(const char *type)
{
	if (!type || !type[0])
		return 0;
	return strcmp(type, "open") != 0;
}

static struct wifi_known *wifi_find_known(const char *ssid)
{
	int i;

	for (i = 0; i < wifi_state.known_count; i++) {
		if (!strcmp(wifi_state.known[i].ssid, ssid))
			return &wifi_state.known[i];
	}
	return NULL;
}

static struct wifi_network *wifi_find_network_path(const char *path)
{
	int i;

	for (i = 0; i < wifi_state.network_count; i++) {
		if (!strcmp(wifi_state.networks[i].path, path))
			return &wifi_state.networks[i];
	}
	return NULL;
}

static void wifi_swap_networks(int a, int b)
{
	struct wifi_network tmp;

	if (a < 0 || b < 0 || a >= wifi_state.network_count ||
			b >= wifi_state.network_count || a == b)
		return;

	tmp = wifi_state.networks[a];
	wifi_state.networks[a] = wifi_state.networks[b];
	wifi_state.networks[b] = tmp;
}

static struct wifi_network *wifi_find_network_ssid(const char *ssid)
{
	int i;

	for (i = 0; i < wifi_state.network_count; i++) {
		if (!strcmp(wifi_state.networks[i].ssid, ssid))
			return &wifi_state.networks[i];
	}
	return NULL;
}

static void wifi_parse_device(DBusMessageIter *props, const char *path)
{
	int powered = 0;

	while (dbus_message_iter_get_arg_type(props) == DBUS_TYPE_DICT_ENTRY) {
		DBusMessageIter entry;
		DBusMessageIter value;
		const char *key;
		char name[32];

		dbus_message_iter_recurse(props, &entry);
		dbus_message_iter_get_basic(&entry, &key);
		dbus_message_iter_next(&entry);
		value = entry;

		if (!strcmp(key, "Name")) {
			name[0] = '\0';
			wifi_variant_string(&value, name, sizeof(name));
			if (!strcmp(name, "wlan0"))
				SETTINGS_copyText(wifi_state.device_path,
					sizeof(wifi_state.device_path), path);
		} else if (!strcmp(key, "Powered")) {
			wifi_variant_bool(&value, &powered);
		}
		dbus_message_iter_next(props);
	}

	if (wifi_state.device_path[0] && !strcmp(wifi_state.device_path, path))
		wifi_state.powered = powered;
}

static void wifi_parse_station(DBusMessageIter *props, const char *path)
{
	SETTINGS_copyText(wifi_state.station_path,
		sizeof(wifi_state.station_path), path);
	while (dbus_message_iter_get_arg_type(props) == DBUS_TYPE_DICT_ENTRY) {
		DBusMessageIter entry;
		DBusMessageIter value;
		const char *key;

		dbus_message_iter_recurse(props, &entry);
		dbus_message_iter_get_basic(&entry, &key);
		dbus_message_iter_next(&entry);
		value = entry;

		if (!strcmp(key, "Scanning")) {
			wifi_variant_bool(&value, &wifi_state.scanning);
		} else if (!strcmp(key, "State")) {
			wifi_variant_string(&value, wifi_state.station_state,
				sizeof(wifi_state.station_state));
		} else if (!strcmp(key, "ConnectedNetwork")) {
			wifi_variant_string(&value, wifi_state.connected_path,
				sizeof(wifi_state.connected_path));
		}
		dbus_message_iter_next(props);
	}
}

static void wifi_parse_known(DBusMessageIter *props, const char *path)
{
	struct wifi_known *known;
	char sec_type[24];

	if (wifi_state.known_count >= WIFI_MAX_CACHE_KNOWN)
		return;
	known = &wifi_state.known[wifi_state.known_count];
	memset(known, 0, sizeof(*known));
	SETTINGS_copyText(known->path, sizeof(known->path), path);
	sec_type[0] = '\0';

	while (dbus_message_iter_get_arg_type(props) == DBUS_TYPE_DICT_ENTRY) {
		DBusMessageIter entry;
		DBusMessageIter value;
		const char *key;

		dbus_message_iter_recurse(props, &entry);
		dbus_message_iter_get_basic(&entry, &key);
		dbus_message_iter_next(&entry);
		value = entry;

		if (!strcmp(key, "Name")) {
			wifi_variant_string(&value, known->ssid, sizeof(known->ssid));
		} else if (!strcmp(key, "Type")) {
			wifi_variant_string(&value, sec_type, sizeof(sec_type));
		}
		dbus_message_iter_next(props);
	}

	if (!known->ssid[0])
		return;
	known->secure = wifi_is_secure_type(sec_type);
	wifi_state.known_count++;
}

static void wifi_parse_network(DBusMessageIter *props, const char *path)
{
	struct wifi_network *network;
	char sec_type[24];

	if (wifi_state.network_count >= WIFI_MAX_CACHE_NETWORKS)
		return;
	network = &wifi_state.networks[wifi_state.network_count];
	memset(network, 0, sizeof(*network));
	SETTINGS_copyText(network->path, sizeof(network->path), path);
	sec_type[0] = '\0';
	network->signal = -1;

	while (dbus_message_iter_get_arg_type(props) == DBUS_TYPE_DICT_ENTRY) {
		DBusMessageIter entry;
		DBusMessageIter value;
		const char *key;
		int signal;

		dbus_message_iter_recurse(props, &entry);
		dbus_message_iter_get_basic(&entry, &key);
		dbus_message_iter_next(&entry);
		value = entry;

		if (!strcmp(key, "Name")) {
			wifi_variant_string(&value, network->ssid,
				sizeof(network->ssid));
		} else if (!strcmp(key, "Connected")) {
			wifi_variant_bool(&value, &network->connected);
		} else if (!strcmp(key, "KnownNetwork")) {
			wifi_variant_string(&value, network->known_path,
				sizeof(network->known_path));
			if (network->known_path[0] &&
					strcmp(network->known_path, "/"))
				network->known = 1;
		} else if (!strcmp(key, "Type")) {
			wifi_variant_string(&value, sec_type, sizeof(sec_type));
		} else if (!strcmp(key, "SignalStrength") ||
				!strcmp(key, "Strength") || !strcmp(key, "RSSI")) {
			if (wifi_variant_signal(&value, &signal))
				network->signal = wifi_signal_percent(signal);
		}
		dbus_message_iter_next(props);
	}

	if (!network->ssid[0])
		return;
	network->secure = wifi_is_secure_type(sec_type);
	wifi_state.network_count++;
}

static void wifi_parse_managed_objects(DBusMessage *reply)
{
	DBusMessageIter iter;
	DBusMessageIter objects;

	if (!reply || !dbus_message_iter_init(reply, &iter) ||
			dbus_message_iter_get_arg_type(&iter) != DBUS_TYPE_ARRAY)
		return;
	dbus_message_iter_recurse(&iter, &objects);
	while (dbus_message_iter_get_arg_type(&objects) == DBUS_TYPE_DICT_ENTRY) {
		DBusMessageIter object_entry;
		DBusMessageIter interfaces;
		const char *path;

		dbus_message_iter_recurse(&objects, &object_entry);
		dbus_message_iter_get_basic(&object_entry, &path);
		dbus_message_iter_next(&object_entry);
		dbus_message_iter_recurse(&object_entry, &interfaces);

		while (dbus_message_iter_get_arg_type(&interfaces) ==
				DBUS_TYPE_DICT_ENTRY) {
			DBusMessageIter iface_entry;
			DBusMessageIter props;
			const char *iface;

			dbus_message_iter_recurse(&interfaces, &iface_entry);
			dbus_message_iter_get_basic(&iface_entry, &iface);
			dbus_message_iter_next(&iface_entry);
			dbus_message_iter_recurse(&iface_entry, &props);

			if (!strcmp(iface, WIFI_DEVICE_IFACE))
				wifi_parse_device(&props, path);
			else if (!strcmp(iface, WIFI_STATION_IFACE))
				wifi_parse_station(&props, path);
			else if (!strcmp(iface, WIFI_NETWORK_IFACE))
				wifi_parse_network(&props, path);
			else if (!strcmp(iface, WIFI_KNOWN_IFACE))
				wifi_parse_known(&props, path);

			dbus_message_iter_next(&interfaces);
		}
		dbus_message_iter_next(&objects);
	}
}

static void wifi_load_ordered_signals(void)
{
	DBusMessage *reply;
	DBusMessageIter iter;
	DBusMessageIter array;
	int ordered_index = 0;

	if (!wifi_state.station_path[0])
		return;

	reply = wifi_call_noarg(wifi_state.station_path, WIFI_STATION_IFACE,
		"GetOrderedNetworks", 4000);
	if (!reply)
		return;
	if (!dbus_message_iter_init(reply, &iter) ||
			dbus_message_iter_get_arg_type(&iter) != DBUS_TYPE_ARRAY) {
		dbus_message_unref(reply);
		return;
	}

	dbus_message_iter_recurse(&iter, &array);
	while (dbus_message_iter_get_arg_type(&array) == DBUS_TYPE_STRUCT) {
		DBusMessageIter entry;
		const char *path;
		dbus_int16_t signal;
		struct wifi_network *network;
		int network_index = -1;
		int i;

		dbus_message_iter_recurse(&array, &entry);
		dbus_message_iter_get_basic(&entry, &path);
		dbus_message_iter_next(&entry);
		dbus_message_iter_get_basic(&entry, &signal);
		for (i = 0; i < wifi_state.network_count; i++) {
			if (!strcmp(wifi_state.networks[i].path, path)) {
				network_index = i;
				break;
			}
		}
		if (network_index >= 0) {
			wifi_swap_networks(ordered_index, network_index);
			network = &wifi_state.networks[ordered_index];
			network->signal = wifi_signal_percent((int)signal);
			ordered_index++;
		}
		dbus_message_iter_next(&array);
	}
	dbus_message_unref(reply);
}

static void wifi_finalize_networks(void)
{
	int i;

	wifi_state.connected_ssid[0] = '\0';

	for (i = 0; i < wifi_state.network_count; i++) {
		struct wifi_network *network = &wifi_state.networks[i];

		if (!network->known && network->ssid[0] &&
				wifi_find_known(network->ssid))
			network->known = 1;
		if (wifi_state.connected_path[0] &&
				!strcmp(network->path, wifi_state.connected_path))
			network->connected = 1;
		if (network->connected && !wifi_state.connected_ssid[0])
			SETTINGS_copyText(wifi_state.connected_ssid,
				sizeof(wifi_state.connected_ssid), network->ssid);
	}
}

static int wifi_refresh_cache(void)
{
	DBusMessage *reply;

	wifi_reset_cache();
	reply = wifi_call_noarg(WIFI_ROOT, WIFI_OBJMGR_IFACE,
		"GetManagedObjects", 4000);
	if (!reply)
		return -ENOTCONN;
	wifi_parse_managed_objects(reply);
	dbus_message_unref(reply);
	wifi_load_ordered_signals();
	wifi_finalize_networks();
	return 0;
}

static int wifi_connect_bus(void)
{
	DBusError err;

	if (wifi_state.conn)
		return 0;

	dbus_error_init(&err);
	wifi_state.conn = dbus_bus_get_private(DBUS_BUS_SYSTEM, &err);
	if (!wifi_state.conn) {
		dbus_error_free(&err);
		return -ENOTCONN;
	}
	dbus_connection_set_exit_on_disconnect(wifi_state.conn, 0);
	return 0;
}

static int wifi_set_bool_property(const char *path, const char *iface,
	const char *property, int value)
{
	DBusMessage *msg;
	DBusMessageIter iter;
	DBusMessageIter variant;
	dbus_bool_t flag;

	if (!path || !iface || !property)
		return -EINVAL;

	msg = dbus_message_new_method_call(WIFI_SERVICE, path, WIFI_PROPS_IFACE,
		"Set");
	if (!msg)
		return -ENOMEM;

	dbus_message_iter_init_append(msg, &iter);
	if (!dbus_message_iter_append_basic(&iter, DBUS_TYPE_STRING, &iface) ||
			!dbus_message_iter_append_basic(&iter, DBUS_TYPE_STRING,
			&property) ||
			!dbus_message_iter_open_container(&iter, DBUS_TYPE_VARIANT,
			"b", &variant)) {
		dbus_message_unref(msg);
		return -ENOMEM;
	}

	flag = value ? 1 : 0;
	if (!dbus_message_iter_append_basic(&variant, DBUS_TYPE_BOOLEAN, &flag) ||
			!dbus_message_iter_close_container(&iter, &variant)) {
		dbus_message_unref(msg);
		return -ENOMEM;
	}
	return wifi_reply_ok(wifi_call(msg, 4000)) ? 0 : -EIO;
}

static int wifi_profile_path(const char *ssid, int secure, char *path,
	size_t path_size)
{
	char encoded[160];
	size_t used;
	const unsigned char *ptr;
	int simple;

	if (!ssid || !ssid[0] || !path || !path_size)
		return -EINVAL;

	simple = 1;
	for (ptr = (const unsigned char *)ssid; *ptr; ptr++) {
		if (isalnum(*ptr) || *ptr == ' ' || *ptr == '_' || *ptr == '-')
			continue;
		simple = 0;
		break;
	}

	encoded[0] = '\0';
	if (simple) {
		SETTINGS_copyText(encoded, sizeof(encoded), ssid);
	} else {
		used = 0;
		encoded[used++] = '=';
		for (ptr = (const unsigned char *)ssid; *ptr &&
				used + 2 < sizeof(encoded); ptr++)
			used += snprintf(encoded + used, sizeof(encoded) - used,
				"%02x", *ptr);
		encoded[used] = '\0';
	}

	snprintf(path, path_size, WIFI_PROFILE_DIR "/%s.%s", encoded,
		secure ? "psk" : "open");
	return 0;
}

static int wifi_write_all(int fd, const char *buf, size_t len)
{
	while (len > 0) {
		ssize_t wrote;

		wrote = write(fd, buf, len);
		if (wrote <= 0)
			return -1;
		buf += wrote;
		len -= (size_t)wrote;
	}
	return 0;
}

static int wifi_fsync_parent(const char *path)
{
	char dir[256];
	char *slash;
	int fd;

	SETTINGS_copyText(dir, sizeof(dir), path);
	slash = strrchr(dir, '/');
	if (!slash)
		return 0;
	if (slash == dir)
		slash[1] = '\0';
	else
		*slash = '\0';

	fd = open(dir, O_RDONLY | O_DIRECTORY | O_CLOEXEC);
	if (fd < 0)
		return -1;
	if (fsync(fd) != 0) {
		close(fd);
		return -1;
	}
	close(fd);
	return 0;
}

static int wifi_write_profile(const char *ssid, const char *passphrase)
{
	char path[256];
	char tmp[264];
	char buf[256];
	int fd;
	size_t len;

	if (!ssid || !ssid[0] || !passphrase || !passphrase[0])
		return -EINVAL;

	if (wifi_profile_path(ssid, 1, path, sizeof(path)) != 0)
		return -EINVAL;
	snprintf(tmp, sizeof(tmp), "%s.tmp", path);
	snprintf(buf, sizeof(buf),
		"[Security]\n"
		"Passphrase=%s\n"
		"\n"
		"[Settings]\n"
		"AutoConnect=true\n",
		passphrase);
	len = strlen(buf);

	fd = open(tmp, O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0600);
	if (fd < 0)
		return -errno;
	if (wifi_write_all(fd, buf, len) != 0 || fsync(fd) != 0) {
		close(fd);
		unlink(tmp);
		return -EIO;
	}
	close(fd);
	if (rename(tmp, path) != 0) {
		unlink(tmp);
		return -errno;
	}
	if (wifi_fsync_parent(path) != 0)
		return -EIO;
	return 0;
}

static int wifi_set_autoconnect(const char *known_path, int enabled)
{
	if (!known_path || !known_path[0])
		return -EINVAL;
	return wifi_set_bool_property(known_path, WIFI_KNOWN_IFACE,
		"AutoConnect", enabled);
}

int SETTINGS_WIFI_BACKEND_init(void)
{
	return wifi_connect_bus();
}

void SETTINGS_WIFI_BACKEND_quit(void)
{
	if (!wifi_state.conn)
		return;
	dbus_connection_close(wifi_state.conn);
	dbus_connection_unref(wifi_state.conn);
	wifi_state.conn = NULL;
	wifi_reset_cache();
}

int SETTINGS_WIFI_BACKEND_refresh(struct settings_snapshot *snapshot)
{
	int i;
	int out = 0;

	if (!snapshot)
		return -EINVAL;
	if (wifi_connect_bus() != 0)
		return -ENOTCONN;
	if (wifi_refresh_cache() != 0)
		return -EIO;

	snapshot->wifi_enabled = wifi_state.powered ? 1 : 0;
	snapshot->wifi_scanning = wifi_state.scanning ? 1 : 0;
	snapshot->wifi_network_count = 0;
	snapshot->wifi_connected_ssid[0] = '\0';

	for (i = 0; i < wifi_state.network_count && out < SETTINGS_MAX_NETWORKS;
			i++) {
		struct settings_wifi_network *dst;
		struct wifi_network *src = &wifi_state.networks[i];
		int forgotten;

		dst = &snapshot->wifi_networks[out++];
		memset(dst, 0, sizeof(*dst));
		SETTINGS_copyText(dst->ssid, sizeof(dst->ssid), src->ssid);
		forgotten = wifi_is_forgotten_ssid(src->ssid);
		dst->connected = src->connected;
		dst->known = forgotten ? 0 : src->known;
		dst->secure = src->secure;
		dst->signal = src->signal;
		if (src->connected) {
			SETTINGS_copyText(dst->state, sizeof(dst->state),
				"Connected");
			SETTINGS_copyText(snapshot->wifi_connected_ssid,
				sizeof(snapshot->wifi_connected_ssid), src->ssid);
			wifi_clear_forgotten_ssid(src->ssid);
		} else if (!forgotten && src->known) {
			SETTINGS_copyText(dst->state, sizeof(dst->state), "Saved");
		}
	}

	for (i = 0; i < wifi_state.known_count && out < SETTINGS_MAX_NETWORKS;
			i++) {
		struct settings_wifi_network *dst;
		struct wifi_known *known = &wifi_state.known[i];

		if (wifi_is_forgotten_ssid(known->ssid))
			continue;
		if (wifi_find_network_ssid(known->ssid))
			continue;
		dst = &snapshot->wifi_networks[out++];
		memset(dst, 0, sizeof(*dst));
		SETTINGS_copyText(dst->ssid, sizeof(dst->ssid), known->ssid);
		dst->known = 1;
		dst->secure = known->secure;
		SETTINGS_copyText(dst->state, sizeof(dst->state), "Saved");
	}

	snapshot->wifi_network_count = out;
	return 0;
}

int SETTINGS_WIFI_BACKEND_set_enabled(int enabled)
{
	if (wifi_connect_bus() != 0)
		return -ENOTCONN;
	if (wifi_refresh_cache() != 0)
		return -EIO;
	if (!wifi_state.device_path[0])
		return -ENODEV;
	if (wifi_set_bool_property(wifi_state.device_path, WIFI_DEVICE_IFACE,
			"Powered", enabled) != 0)
		return -EIO;
	if (!enabled && wifi_state.station_path[0])
		(void)wifi_reply_ok(wifi_call_noarg(wifi_state.station_path,
			WIFI_STATION_IFACE, "Disconnect", 4000));
	return 0;
}

int SETTINGS_WIFI_BACKEND_set_scanning(int enabled)
{
	if (!enabled)
		return 0;
	if (wifi_connect_bus() != 0)
		return -ENOTCONN;
	if (wifi_refresh_cache() != 0)
		return -EIO;
	if (wifi_state.scanning)
		return 0;
	if (!wifi_state.station_path[0])
		return -ENODEV;
	return wifi_reply_ok(wifi_call_noarg(wifi_state.station_path,
		WIFI_STATION_IFACE, "Scan", 12000)) ? 0 : -EIO;
}

int SETTINGS_WIFI_BACKEND_connect(const char *ssid, const char *passphrase,
	int hidden)
{
	struct wifi_network *network;
	struct wifi_known *known;

	if (!ssid || !ssid[0])
		return -EINVAL;
	if (wifi_connect_bus() != 0)
		return -ENOTCONN;
	if (wifi_refresh_cache() != 0)
		return -EIO;
	if (wifi_state.connected_ssid[0] &&
			!strcmp(wifi_state.connected_ssid, ssid))
		return 0;
	wifi_clear_forgotten_ssid(ssid);
	if (!wifi_state.powered &&
			SETTINGS_WIFI_BACKEND_set_enabled(1) != 0)
		return -EIO;
	if (passphrase && passphrase[0] &&
			wifi_write_profile(ssid, passphrase) != 0)
		return -EIO;

	if (hidden) {
		if (!wifi_state.station_path[0])
			return -ENODEV;
		if (!wifi_reply_ok(wifi_call_string(wifi_state.station_path,
				WIFI_STATION_IFACE, "ConnectHiddenNetwork", ssid,
				20000)))
			return -EIO;
	} else {
		network = wifi_find_network_ssid(ssid);
		if (!network) {
			if (!wifi_state.station_path[0])
				return -ENODEV;
			if (!wifi_reply_ok(wifi_call_string(wifi_state.station_path,
					WIFI_STATION_IFACE, "ConnectHiddenNetwork",
					ssid, 20000)))
				return -EIO;
		} else if (!wifi_reply_ok(wifi_call_noarg(network->path,
				WIFI_NETWORK_IFACE, "Connect", 20000))) {
			return -EIO;
		}
	}

	if (wifi_refresh_cache() == 0) {
		known = wifi_find_known(ssid);
		if (known)
			(void)wifi_set_autoconnect(known->path, 1);
	}
	return 0;
}

int SETTINGS_WIFI_BACKEND_disconnect(void)
{
	if (wifi_connect_bus() != 0)
		return -ENOTCONN;
	if (wifi_refresh_cache() != 0)
		return -EIO;
	if (!wifi_state.station_path[0])
		return -ENODEV;
	return wifi_reply_ok(wifi_call_noarg(wifi_state.station_path,
		WIFI_STATION_IFACE, "Disconnect", 12000)) ? 0 : -EIO;
}

int SETTINGS_WIFI_BACKEND_forget(const char *ssid)
{
	struct wifi_known *known;

	if (!ssid || !ssid[0])
		return -EINVAL;
	if (wifi_connect_bus() != 0)
		return -ENOTCONN;
	if (wifi_refresh_cache() != 0)
		return -EIO;
	if (wifi_state.connected_ssid[0] &&
			!strcmp(wifi_state.connected_ssid, ssid))
		(void)SETTINGS_WIFI_BACKEND_disconnect();
	if (wifi_refresh_cache() != 0)
		return -EIO;

	known = wifi_find_known(ssid);
	if (!known)
		return -ENOENT;
	if (!wifi_reply_ok(wifi_call_noarg(known->path, WIFI_KNOWN_IFACE,
			"Forget", 5000)))
		return -EIO;
	wifi_mark_forgotten_ssid(ssid);
	return 0;
}
