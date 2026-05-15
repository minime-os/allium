#!/bin/sh

PATH="/usr/sbin:/sbin:/usr/bin:/bin"

WIFI_INTERFACE="wlan0"
IWCTL_BIN="${MINUI_IWCTL_BIN:-/usr/bin/iwctl}"
PERSIST_DIR="/mnt/sdcard/system/wifi/iwd"
IWD_STATE_DIR="/var/lib/iwd"
WIFI_DRIVER_MODULE="rtw88_8821cs"
WIFI_DRIVER_WAIT_SECONDS=15
UDHCPC_PID_FILE="/run/udhcpc.${WIFI_INTERFACE}.pid"
UDHCPC_LOG_FILE="/tmp/udhcpc.${WIFI_INTERFACE}.log"
CONNECT_PID_FILE="/run/wifi-connect.pid"
CONNECT_LOG_FILE="/tmp/wifi-connect.log"
IP_WAIT_SECONDS=12

log() {
	line="$(date +"%F %T") $*"
	echo "$line" >> /tmp/wifi-init.log
	echo "$line"
}

run_iwctl_timeout() {
	timeout_s="$1"
	shift
	if [ ! -x "$IWCTL_BIN" ]; then
		echo "iwctl missing: $IWCTL_BIN" > /tmp/wifi-iwctl.log
		return 127
	fi
	TERM=dumb "$IWCTL_BIN" "$@" >/tmp/wifi-iwctl.log 2>&1 &
	pid=$!
	i=0
	while kill -0 "$pid" >/dev/null 2>&1; do
		if [ "$i" -ge "$timeout_s" ]; then
			kill "$pid" >/dev/null 2>&1 || true
			wait "$pid" >/dev/null 2>&1 || true
			return 124
		fi
		i=$((i + 1))
		sleep 1
	done
	wait "$pid" >/dev/null 2>&1
	return $?
}

ensure_interface_up() {
	rfkill unblock wifi >/dev/null 2>&1 || true

	if ! ip link show "$WIFI_INTERFACE" >/dev/null 2>&1; then
		if command -v modprobe >/dev/null 2>&1; then
			modprobe "$WIFI_DRIVER_MODULE" >/dev/null 2>&1 || true
		fi
	fi

	i=0
	while [ "$i" -lt "$WIFI_DRIVER_WAIT_SECONDS" ]; do
		if ip link show "$WIFI_INTERFACE" >/dev/null 2>&1; then
			ip link set "$WIFI_INTERFACE" up >/dev/null 2>&1 || true
			ip link show "$WIFI_INTERFACE" >/dev/null 2>&1 || return 1
			if [ -e /sys/class/net/wlan0/queues/rx-0/rps_cpus ]; then
				echo f > /sys/class/net/wlan0/queues/rx-0/rps_cpus
			fi
			return 0
		fi
		i=$((i + 1))
		sleep 1
	done

	return 1
}

wait_iwd_ready() {
	timeout_s="${1:-5}"
	i=0
	while [ "$i" -lt "$timeout_s" ]; do
		if pidof iwd >/dev/null 2>&1 && run_iwctl_timeout 2 station "$WIFI_INTERFACE" show; then
			return 0
		fi
		i=$((i + 1))
		sleep 1
	done
	return 1
}

state_dir_is_bound() {
	awk -v src="$PERSIST_DIR" -v dst="$IWD_STATE_DIR" '''$1 == src && $2 == dst { found=1 } END { exit(found ? 0 : 1) }''' /proc/mounts >/dev/null 2>&1
}

sync_profiles_from_persist() {
	mkdir -p "$PERSIST_DIR" "$IWD_STATE_DIR"
	cp -a "$PERSIST_DIR"/. "$IWD_STATE_DIR"/ 2>/dev/null || true
	chmod 700 "$IWD_STATE_DIR" 2>/dev/null || true
	chmod 600 "$IWD_STATE_DIR"/*.psk "$IWD_STATE_DIR"/*.8021x 2>/dev/null || true
}

sync_profiles_to_persist() {
	mkdir -p "$PERSIST_DIR"
	if state_dir_is_bound; then
		return 0
	fi
	rm -f "$PERSIST_DIR"/*.psk "$PERSIST_DIR"/*.8021x 2>/dev/null || true
	cp -a "$IWD_STATE_DIR"/*.psk "$IWD_STATE_DIR"/*.8021x "$PERSIST_DIR"/ 2>/dev/null || true
}

start_dbus_if_needed() {
	if ! pidof dbus-daemon >/dev/null 2>&1; then
		if [ -x /etc/init.d/S30dbus-daemon ]; then
			/etc/init.d/S30dbus-daemon start >/dev/null 2>&1 || return 1
		fi
	fi
	return 0
}

start_iwd_if_needed() {
	if pidof iwd >/dev/null 2>&1; then
		return 0
	fi

	if [ -x /etc/init.d/S40iwd ]; then
		MINUI_IWD_FORCE_START=1 /etc/init.d/S40iwd start >/dev/null 2>&1 || true
	fi

	pidof iwd >/dev/null 2>&1
}

stop_dhcp_client() {
	if [ -f "$UDHCPC_PID_FILE" ]; then
		pid="$(cat "$UDHCPC_PID_FILE" 2>/dev/null || true)"
		if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
			kill "$pid" >/dev/null 2>&1 || true
		fi
		rm -f "$UDHCPC_PID_FILE"
	fi
}

start_dhcp_client() {
	if ! command -v udhcpc >/dev/null 2>&1; then
		return 1
	fi

	if [ -f "$UDHCPC_PID_FILE" ]; then
		pid="$(cat "$UDHCPC_PID_FILE" 2>/dev/null || true)"
		if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
			return 0
		fi
		rm -f "$UDHCPC_PID_FILE"
	fi

	udhcpc -b -R -O search -O staticroutes -p "$UDHCPC_PID_FILE" -i "$WIFI_INTERFACE" \
		-s /usr/share/udhcpc/default.script >"$UDHCPC_LOG_FILE" 2>&1 || return 1
	return 0
}

wait_ipv4_address() {
	timeout_s="${1:-$IP_WAIT_SECONDS}"
	i=0
	while [ "$i" -lt "$timeout_s" ]; do
		if ip -4 addr show "$WIFI_INTERFACE" 2>/dev/null | grep -q "inet "; then
			return 0
		fi
		i=$((i + 1))
		sleep 1
	done
	return 1
}

refresh_dhcp_lease() {
	stop_dhcp_client
	start_dhcp_client || return 1
	wait_ipv4_address "$IP_WAIT_SECONDS"
}

stop_connect_worker() {
	if [ -f "$CONNECT_PID_FILE" ]; then
		pid="$(cat "$CONNECT_PID_FILE" 2>/dev/null || true)"
		if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
			kill "$pid" >/dev/null 2>&1 || true
		fi
		rm -f "$CONNECT_PID_FILE"
	fi
}

spawn_connect_worker() {
	ssid="$1"
	passphrase="${2:-}"

	stop_connect_worker
	(
		log "connect worker start: ssid='$ssid'"
		run_iwctl_timeout 4 station "$WIFI_INTERFACE" scan || true
		if [ -n "$passphrase" ]; then
			run_iwctl_timeout 30 --passphrase "$passphrase" station "$WIFI_INTERFACE" connect "$ssid"
		else
			run_iwctl_timeout 30 station "$WIFI_INTERFACE" connect "$ssid"
		fi
		rc=$?
		if [ "$rc" -eq 0 ]; then
			run_iwctl_timeout 3 known-networks "$ssid" set-property AutoConnect yes >/dev/null 2>&1 || true
			refresh_dhcp_lease >/dev/null 2>&1 || true
			sync_profiles_to_persist
			log "connect worker success: ssid='$ssid'"
		else
			log "connect worker failed: ssid='$ssid' rc=$rc"
		fi
		exit "$rc"
	) >>"$CONNECT_LOG_FILE" 2>&1 &
	echo $! > "$CONNECT_PID_FILE"
}

list_known_network_ssids() {
	TERM=dumb "$IWCTL_BIN" known-networks list 2>/dev/null | awk '
		{
			gsub(/\x1B\[[0-9;]*[[:alpha:]]/, "", $0)
		}
		/Known Networks/ { next }
		/Name/ && /Security/ { next }
		/^-+/ { next }
		NF {
			name = ""
			for (i = 1; i <= NF; i++) {
				token = tolower($i)
				if (token == "open" || token == "psk" || token == "sae" || token == "wep" || token == "8021x") {
					break
				}
				name = (name == "" ? $i : name " " $i)
			}
			sub(/^[ \t]+/, "", name)
			sub(/[ \t]+$/, "", name)
			if (name != "") {
				print name
			}
		}
	'
}

enable_all_saved_autoconnect() {
	pass=0
	while [ "$pass" -lt 2 ]; do
		list_known_network_ssids | while IFS= read -r ssid; do
			[ -n "$ssid" ] || continue
			run_iwctl_timeout 5 known-networks "$ssid" set-property AutoConnect yes >/dev/null 2>&1 || true
		done
		pass=$((pass + 1))
		sleep 1
	done
}

station_has_connection() {
	TERM=dumb "$IWCTL_BIN" station "$WIFI_INTERFACE" show 2>/dev/null | awk '
		{
			gsub(/\x1B\[[0-9;]*[[:alpha:]]/, "", $0)
		}
		/Connected network/ {
			line = $0
			sub(/^.*Connected network[[:space:]]*:?[[:space:]]*/, "", line)
			sub(/^[ \t]+/, "", line)
			sub(/[ \t]+$/, "", line)
			if (line != "" && line != "--") {
				found = 1
			}
		}
		END { exit(found ? 0 : 1) }
	'
}

current_connected_ssid() {
	TERM=dumb "$IWCTL_BIN" station "$WIFI_INTERFACE" show 2>/dev/null | awk '
		{
			gsub(/\x1B\[[0-9;]*[[:alpha:]]/, "", $0)
		}
		/Connected network/ {
			line = $0
			sub(/^.*Connected network[[:space:]]*:?[[:space:]]*/, "", line)
			sub(/^[ \t]+/, "", line)
			sub(/[ \t]+$/, "", line)
			if (line != "" && line != "--") {
				print line
				exit 0
			}
		}
	'
}

wait_station_connected() {
	timeout_s="${1:-10}"
	i=0
	while [ "$i" -lt "$timeout_s" ]; do
		if station_has_connection; then
			return 0
		fi
		i=$((i + 1))
		sleep 1
	done
	return 1
}

connect_first_known_network() {
	ssid="$(list_known_network_ssids | head -n 1)"
	[ -n "$ssid" ] || return 1
	log "autoconnect fallback: connect '$ssid'"
	run_iwctl_timeout 25 station "$WIFI_INTERFACE" connect "$ssid" >/dev/null 2>&1 || return 1
	return 0
}

start_wifi() {
	iwd_was_running=0

	log "start_wifi begin"
	if pidof iwd >/dev/null 2>&1; then
		iwd_was_running=1
	fi
	start_dbus_if_needed || return 1
	if [ "$iwd_was_running" -eq 0 ]; then
		sync_profiles_from_persist
	fi
	start_iwd_if_needed || {
		log "iwd not running"
		return 1
	}
	ensure_interface_up || {
		log "wifi driver/interface not ready"
		return 1
	}
	if ! wait_iwd_ready 20; then
		log "iwd not ready"
		return 1
	fi

	if station_has_connection; then
		return 0
	fi

	enable_all_saved_autoconnect
	start_dhcp_client >/dev/null 2>&1 || true

	wait_station_connected 12 || true
	if station_has_connection; then
		wait_ipv4_address 8 || refresh_dhcp_lease >/dev/null 2>&1 || true
		return 0
	fi

	connect_first_known_network || true
	wait_station_connected 10 || true
	if station_has_connection; then
		wait_ipv4_address 8 || refresh_dhcp_lease >/dev/null 2>&1 || true
	else
		log "autoconnect pending"
	fi
	return 0
}

stop_wifi() {
	stop_connect_worker
	sync_profiles_to_persist
	run_iwctl_timeout 4 station "$WIFI_INTERFACE" disconnect || true
	stop_dhcp_client
	ip addr flush dev "$WIFI_INTERFACE" scope global >/dev/null 2>&1 || true
	ip link set "$WIFI_INTERFACE" down >/dev/null 2>&1 || true
	rfkill block wifi >/dev/null 2>&1 || true
	return 0
}

manual_connect() {
	ssid="$1"
	passphrase="${2:-}"
	current_ssid=""
	[ -n "$ssid" ] || return 1
	start_wifi || return 1
	current_ssid="$(current_connected_ssid)"
	if [ "$current_ssid" = "$ssid" ]; then
		log "already connected: ssid='$ssid'"
		return 0
	fi
	spawn_connect_worker "$ssid" "$passphrase"
	return 0
}

manual_disconnect() {
	stop_connect_worker
	run_iwctl_timeout 4 station "$WIFI_INTERFACE" disconnect || true
	stop_dhcp_client
	ip addr flush dev "$WIFI_INTERFACE" scope global >/dev/null 2>&1 || true
	sync_profiles_to_persist
	return 0
}

manual_scan() {
	start_wifi || return 1
	run_iwctl_timeout 10 station "$WIFI_INTERFACE" scan
}

case "$1" in
	start)
		start_wifi
		exit $?
		;;
	stop)
		stop_wifi
		exit $?
		;;
	restart)
		stop_wifi
		start_wifi
		exit $?
		;;
	connect)
		manual_connect "$2" "$3"
		exit $?
		;;
	disconnect)
		manual_disconnect
		exit $?
		;;
	scan)
		manual_scan
		exit $?
		;;
	*)
		echo "Usage: $0 {start|stop|restart|scan|connect <ssid> [passphrase]|disconnect}" >&2
		exit 1
		;;
esac
