#include <dbus/dbus.h>

#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <strings.h>

#include "bt_backend.h"

#define BT_SERVICE "org.bluez"
#define BT_ROOT "/"
#define BT_OBJMGR_IFACE "org.freedesktop.DBus.ObjectManager"
#define BT_PROPS_IFACE "org.freedesktop.DBus.Properties"
#define BT_ADAPTER_IFACE "org.bluez.Adapter1"
#define BT_DEVICE_IFACE "org.bluez.Device1"

#define BT_MAX_PATH 192
#define BT_MAX_CACHE_DEVICES 32

///////////////////////////////////////
struct bt_device {
	char path[BT_MAX_PATH];
	char addr[18];
	char name[64];
	char icon[32];
	unsigned int class_id;
	int paired;
	int connected;
	int trusted;
	int kind;
};

struct bt_backend_state {
	DBusConnection *conn;
	char adapter_path[BT_MAX_PATH];
	int powered;
	int scanning;
	struct bt_device devices[BT_MAX_CACHE_DEVICES];
	int device_count;
};

static struct bt_backend_state bt_state;

///////////////////////////////////////
static DBusMessage *bt_call(DBusMessage *msg, int timeout_ms)
{
	DBusError err;
	DBusMessage *reply;

	if (!bt_state.conn || !msg)
		return NULL;

	dbus_error_init(&err);
	reply = dbus_connection_send_with_reply_and_block(bt_state.conn, msg,
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

static DBusMessage *bt_call_noarg(const char *path, const char *iface,
	const char *method, int timeout_ms)
{
	DBusMessage *msg;

	msg = dbus_message_new_method_call(BT_SERVICE, path, iface, method);
	if (!msg)
		return NULL;
	return bt_call(msg, timeout_ms);
}

static int bt_reply_ok(DBusMessage *reply)
{
	int ok;

	if (!reply)
		return 0;
	ok = dbus_message_get_type(reply) != DBUS_MESSAGE_TYPE_ERROR;
	dbus_message_unref(reply);
	return ok;
}

static int bt_variant_bool(DBusMessageIter *variant, int *value)
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

static int bt_variant_string(DBusMessageIter *variant, char *value,
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

static int bt_variant_uint(DBusMessageIter *variant, unsigned int *value)
{
	DBusMessageIter inner;
	dbus_uint32_t tmp;

	if (!variant || !value ||
			dbus_message_iter_get_arg_type(variant) != DBUS_TYPE_VARIANT)
		return 0;
	dbus_message_iter_recurse(variant, &inner);
	if (dbus_message_iter_get_arg_type(&inner) != DBUS_TYPE_UINT32)
		return 0;
	dbus_message_iter_get_basic(&inner, &tmp);
	*value = tmp;
	return 1;
}

static int bt_variant_has_uuid(DBusMessageIter *variant, const char *uuid)
{
	DBusMessageIter inner;
	DBusMessageIter array;

	if (!variant || !uuid ||
			dbus_message_iter_get_arg_type(variant) != DBUS_TYPE_VARIANT)
		return 0;
	dbus_message_iter_recurse(variant, &inner);
	if (dbus_message_iter_get_arg_type(&inner) != DBUS_TYPE_ARRAY)
		return 0;
	dbus_message_iter_recurse(&inner, &array);
	while (dbus_message_iter_get_arg_type(&array) == DBUS_TYPE_STRING) {
		const char *candidate;

		dbus_message_iter_get_basic(&array, &candidate);
		if (candidate && !strcasecmp(candidate, uuid))
			return 1;
		dbus_message_iter_next(&array);
	}
	return 0;
}

static int bt_device_kind(const char *icon, unsigned int class_id,
	DBusMessageIter *uuids)
{
	if (icon && strstr(icon, "audio"))
		return SETTINGS_BT_DEVICE_AUDIO;
	if (icon && (strstr(icon, "game") || strstr(icon, "input")))
		return SETTINGS_BT_DEVICE_GAMEPAD;
	if ((class_id & 0x1f00) == 0x0500)
		return SETTINGS_BT_DEVICE_GAMEPAD;
	if (uuids && bt_variant_has_uuid(uuids,
			"00001124-0000-1000-8000-00805f9b34fb"))
		return SETTINGS_BT_DEVICE_GAMEPAD;
	if (uuids && (bt_variant_has_uuid(uuids,
			"0000110b-0000-1000-8000-00805f9b34fb") ||
			bt_variant_has_uuid(uuids,
			"0000110e-0000-1000-8000-00805f9b34fb") ||
			bt_variant_has_uuid(uuids,
			"00001108-0000-1000-8000-00805f9b34fb")))
		return SETTINGS_BT_DEVICE_AUDIO;
	return SETTINGS_BT_DEVICE_UNKNOWN;
}

static void bt_reset_cache(void)
{
	memset(bt_state.adapter_path, 0, sizeof(bt_state.adapter_path));
	bt_state.powered = 0;
	bt_state.scanning = 0;
	memset(bt_state.devices, 0, sizeof(bt_state.devices));
	bt_state.device_count = 0;
}

static void bt_parse_adapter(DBusMessageIter *props, const char *path)
{
	SETTINGS_copyText(bt_state.adapter_path,
		sizeof(bt_state.adapter_path), path);
	while (dbus_message_iter_get_arg_type(props) == DBUS_TYPE_DICT_ENTRY) {
		DBusMessageIter entry;
		DBusMessageIter value;
		const char *key;

		dbus_message_iter_recurse(props, &entry);
		dbus_message_iter_get_basic(&entry, &key);
		dbus_message_iter_next(&entry);
		value = entry;

		if (!strcmp(key, "Powered"))
			bt_variant_bool(&value, &bt_state.powered);
		else if (!strcmp(key, "Discovering"))
			bt_variant_bool(&value, &bt_state.scanning);
		dbus_message_iter_next(props);
	}
}

static void bt_parse_device(DBusMessageIter *props, const char *path)
{
	struct bt_device *device;
	DBusMessageIter uuids;
	int have_uuids = 0;

	if (bt_state.device_count >= BT_MAX_CACHE_DEVICES)
		return;

	device = &bt_state.devices[bt_state.device_count];
	memset(device, 0, sizeof(*device));
	SETTINGS_copyText(device->path, sizeof(device->path), path);

	while (dbus_message_iter_get_arg_type(props) == DBUS_TYPE_DICT_ENTRY) {
		DBusMessageIter entry;
		DBusMessageIter value;
		const char *key;

		dbus_message_iter_recurse(props, &entry);
		dbus_message_iter_get_basic(&entry, &key);
		dbus_message_iter_next(&entry);
		value = entry;

		if (!strcmp(key, "Address")) {
			bt_variant_string(&value, device->addr,
				sizeof(device->addr));
		} else if (!strcmp(key, "Alias")) {
			bt_variant_string(&value, device->name,
				sizeof(device->name));
		} else if (!strcmp(key, "Name") && !device->name[0]) {
			bt_variant_string(&value, device->name,
				sizeof(device->name));
		} else if (!strcmp(key, "Icon")) {
			bt_variant_string(&value, device->icon,
				sizeof(device->icon));
		} else if (!strcmp(key, "Paired")) {
			bt_variant_bool(&value, &device->paired);
		} else if (!strcmp(key, "Connected")) {
			bt_variant_bool(&value, &device->connected);
		} else if (!strcmp(key, "Trusted")) {
			bt_variant_bool(&value, &device->trusted);
		} else if (!strcmp(key, "Class")) {
			bt_variant_uint(&value, &device->class_id);
		} else if (!strcmp(key, "UUIDs")) {
			uuids = value;
			have_uuids = 1;
		}
		dbus_message_iter_next(props);
	}

	if (!device->addr[0])
		return;
	device->kind = bt_device_kind(device->icon, device->class_id,
		have_uuids ? &uuids : NULL);
	bt_state.device_count++;
}

static void bt_parse_managed_objects(DBusMessage *reply)
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

			if (!strcmp(iface, BT_ADAPTER_IFACE))
				bt_parse_adapter(&props, path);
			else if (!strcmp(iface, BT_DEVICE_IFACE))
				bt_parse_device(&props, path);

			dbus_message_iter_next(&interfaces);
		}
		dbus_message_iter_next(&objects);
	}
}

static int bt_connect_bus(void)
{
	DBusError err;

	if (bt_state.conn)
		return 0;

	dbus_error_init(&err);
	bt_state.conn = dbus_bus_get_private(DBUS_BUS_SYSTEM, &err);
	if (!bt_state.conn) {
		dbus_error_free(&err);
		return -ENOTCONN;
	}
	dbus_connection_set_exit_on_disconnect(bt_state.conn, 0);
	return 0;
}

static int bt_refresh_cache(void)
{
	DBusMessage *reply;

	bt_reset_cache();
	reply = bt_call_noarg(BT_ROOT, BT_OBJMGR_IFACE, "GetManagedObjects",
		4000);
	if (!reply)
		return -ENOTCONN;
	bt_parse_managed_objects(reply);
	dbus_message_unref(reply);
	return 0;
}

static int bt_set_bool_property(const char *path, const char *iface,
	const char *property, int enabled)
{
	DBusMessage *msg;
	DBusMessageIter iter;
	DBusMessageIter variant;
	dbus_bool_t flag;

	msg = dbus_message_new_method_call(BT_SERVICE, path, BT_PROPS_IFACE,
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

	flag = enabled ? 1 : 0;
	if (!dbus_message_iter_append_basic(&variant, DBUS_TYPE_BOOLEAN, &flag) ||
			!dbus_message_iter_close_container(&iter, &variant)) {
		dbus_message_unref(msg);
		return -ENOMEM;
	}

	return bt_reply_ok(bt_call(msg, 4000)) ? 0 : -EIO;
}

static struct bt_device *bt_find_device(const char *addr)
{
	int i;

	for (i = 0; i < bt_state.device_count; i++) {
		if (!strcmp(bt_state.devices[i].addr, addr))
			return &bt_state.devices[i];
	}
	return NULL;
}

int SETTINGS_BT_BACKEND_init(void)
{
	return bt_connect_bus();
}

void SETTINGS_BT_BACKEND_quit(void)
{
	if (!bt_state.conn)
		return;
	dbus_connection_close(bt_state.conn);
	dbus_connection_unref(bt_state.conn);
	bt_state.conn = NULL;
	bt_reset_cache();
}

int SETTINGS_BT_BACKEND_refresh(struct settings_snapshot *snapshot)
{
	int i;
	int out = 0;

	if (!snapshot)
		return -EINVAL;
	if (bt_connect_bus() != 0)
		return -ENOTCONN;
	if (bt_refresh_cache() != 0)
		return -EIO;

	snapshot->bt_enabled = bt_state.powered ? 1 : 0;
	snapshot->bt_scanning = bt_state.scanning ? 1 : 0;
	snapshot->bt_device_count = 0;
	snapshot->bt_connected_name[0] = '\0';

	for (i = 0; i < bt_state.device_count && out < SETTINGS_MAX_BT_DEVICES;
			i++) {
		struct bt_device *src = &bt_state.devices[i];
		struct settings_bt_device *dst;

		if (src->kind == SETTINGS_BT_DEVICE_UNKNOWN)
			continue;
		dst = &snapshot->bt_devices[out++];
		memset(dst, 0, sizeof(*dst));
		SETTINGS_copyText(dst->addr, sizeof(dst->addr), src->addr);
		SETTINGS_copyText(dst->name, sizeof(dst->name),
			src->name[0] ? src->name : src->addr);
		dst->paired = src->paired;
		dst->connected = src->connected;
		dst->kind = src->kind;
		if (src->connected) {
			SETTINGS_copyText(dst->state, sizeof(dst->state),
				"connected");
			SETTINGS_copyText(snapshot->bt_connected_name,
				sizeof(snapshot->bt_connected_name), dst->name);
		} else if (src->paired) {
			SETTINGS_copyText(dst->state, sizeof(dst->state), "paired");
		} else {
			SETTINGS_copyText(dst->state, sizeof(dst->state), "new");
		}
	}

	snapshot->bt_device_count = out;
	return 0;
}

int SETTINGS_BT_BACKEND_set_enabled(int enabled)
{
	if (bt_connect_bus() != 0)
		return -ENOTCONN;
	if (bt_refresh_cache() != 0)
		return -EIO;
	if (!bt_state.adapter_path[0])
		return -ENODEV;
	if (!enabled)
		(void)SETTINGS_BT_BACKEND_set_scanning(0);
	return bt_set_bool_property(bt_state.adapter_path, BT_ADAPTER_IFACE,
		"Powered", enabled);
}

int SETTINGS_BT_BACKEND_set_scanning(int enabled)
{
	if (bt_connect_bus() != 0)
		return -ENOTCONN;
	if (bt_refresh_cache() != 0)
		return -EIO;
	if (!bt_state.adapter_path[0])
		return -ENODEV;
	if (enabled) {
		(void)bt_set_bool_property(bt_state.adapter_path,
			BT_ADAPTER_IFACE, "Pairable", 1);
		return bt_reply_ok(bt_call_noarg(bt_state.adapter_path,
			BT_ADAPTER_IFACE, "StartDiscovery", 5000)) ? 0 : -EIO;
	}
	(void)bt_set_bool_property(bt_state.adapter_path, BT_ADAPTER_IFACE,
		"Pairable", 0);
	return bt_reply_ok(bt_call_noarg(bt_state.adapter_path,
		BT_ADAPTER_IFACE, "StopDiscovery", 5000)) ? 0 : -EIO;
}

int SETTINGS_BT_BACKEND_toggle_device(const char *addr)
{
	struct bt_device *device;

	if (!addr || !addr[0])
		return -EINVAL;
	if (bt_connect_bus() != 0)
		return -ENOTCONN;
	if (bt_refresh_cache() != 0)
		return -EIO;

	device = bt_find_device(addr);
	if (!device)
		return -ENOENT;

	if (device->connected)
		return bt_reply_ok(bt_call_noarg(device->path, BT_DEVICE_IFACE,
			"Disconnect", 12000)) ? 0 : -EIO;

	if (!device->paired) {
		if (bt_state.adapter_path[0])
			(void)bt_set_bool_property(bt_state.adapter_path,
				BT_ADAPTER_IFACE, "Pairable", 1);
		if (!bt_reply_ok(bt_call_noarg(device->path, BT_DEVICE_IFACE,
				"Pair", 30000)))
			return -EIO;
		(void)bt_set_bool_property(device->path, BT_DEVICE_IFACE,
			"Trusted", 1);
	}

	return bt_reply_ok(bt_call_noarg(device->path, BT_DEVICE_IFACE,
		"Connect", 12000)) ? 0 : -EIO;
}

int SETTINGS_BT_BACKEND_forget_device(const char *addr)
{
	struct bt_device *device;
	DBusMessage *msg;

	if (!addr || !addr[0])
		return -EINVAL;
	if (bt_connect_bus() != 0)
		return -ENOTCONN;
	if (bt_refresh_cache() != 0)
		return -EIO;
	if (!bt_state.adapter_path[0])
		return -ENODEV;

	device = bt_find_device(addr);
	if (!device)
		return -ENOENT;

	msg = dbus_message_new_method_call(BT_SERVICE, bt_state.adapter_path,
		BT_ADAPTER_IFACE, "RemoveDevice");
	if (!msg)
		return -ENOMEM;
	if (!dbus_message_append_args(msg, DBUS_TYPE_OBJECT_PATH, &device->path,
			DBUS_TYPE_INVALID)) {
		dbus_message_unref(msg);
		return -ENOMEM;
	}
	return bt_reply_ok(bt_call(msg, 5000)) ? 0 : -EIO;
}

int SETTINGS_BT_BACKEND_confirm_device(const char *addr, int accept)
{
	(void)addr;
	(void)accept;
	return -EOPNOTSUPP;
}
