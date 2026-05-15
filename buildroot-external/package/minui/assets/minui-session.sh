#!/bin/sh

SDCARD_PATH="/mnt/sdcard"
SDCARD2_PATH="/mnt/sdcard2"
SD2_HELPER="/usr/bin/minui-sd2.sh"
SYSTEM_SRC_ROOT="/usr/share/minui"
SYSTEM_PATH="${SYSTEM_SRC_ROOT}"
CORES_PATH="/usr/lib/minui/cores"
USERDATA_PATH="${SDCARD_PATH}/system/minui"
SHARED_USERDATA_PATH="${SDCARD_PATH}/saves"
TIMEZONE_STATE_PATH="${SDCARD_PATH}/system/time/timezone.conf"
# `system/minui` is MinUI-private; the service-owned `system/*` paths below
# are intentionally shared with early boot/system init scripts.
LOGS_PATH="${SDCARD_PATH}/system/logs"
DATETIME_PATH="${SDCARD_PATH}/system/time/datetime.state"
AUDIO_CONF_PATH="${SDCARD_PATH}/system/audio/asound.conf"
AUDIO_CONF_TMP_PATH="${SDCARD_PATH}/system/audio/asound.conf.tmp"
AUDIO_RC_COMPAT_PATH="${SDCARD_PATH}/system/audio/asoundrc.compat"
AUDIO_DEFAULT_CONF_SRC="/usr/share/minui/defaults/asound.conf"
HDMI_EXPORT_PATH="/tmp/hdmi_export.sh"
BOOT_LOG="${LOGS_PATH}/boot.log"
SESSION_LOG_DIR="/tmp/minui-logs"
MERGED_BIOS_PATH="/tmp/minui-bios-merged"
SD2_PROMPT_FLAG="/tmp/minui-sd2-init-required"
SD2_FORMAT_REQUEST_FLAG="/tmp/minui-sd2-format-request"
MINUI_SD2_PRESENT="0"
MINUI_SD2_READY="0"
MINUI_BIOS_DIR="${MERGED_BIOS_PATH}"
MINUI_SD2_MARKER_NAME=".extended"
MINUI_SD_LOGS="${MINUI_SD_LOGS:-0}"
EARLY_BOOT_LOG="/tmp/boot-launch-early.txt"
BOOT_SPLASH_FLAG="/tmp/minui-boot-splash-shown"
MINUI_SESSION_SPLASH_FALLBACK="${MINUI_SESSION_SPLASH_FALLBACK:-0}"
MINUI_DEFER_SERVICES_SECS="${MINUI_DEFER_SERVICES_SECS:-5}"
MINUI_DEFER_HDMIMON_SECS="${MINUI_DEFER_HDMIMON_SECS:-8}"
EXEC_PATH="/tmp/minui_exec"
NEXT_PATH="/tmp/next"
BEZELS_SRC_ROOT="${SYSTEM_PATH}/bezels"
BEZELS_SEED_MARKER="${USERDATA_PATH}/.bezels.seeded"

KEYMON_PID=""
HDMIMON_PID=""
SD2_FORMAT_SPLASH_PID=""

boot_uptime_ms() {
	awk '{ printf "%d", ($1 * 1000) }' /proc/uptime 2>/dev/null || echo 0
}

log() {
	line="$(date +"%F %T") $*"
	echo "$line" >> "$EARLY_BOOT_LOG"
	if [ -d "$SESSION_LOG_DIR" ]; then
		echo "$line" >> "$BOOT_LOG"
	fi
}

mark_stage_end() {
	stage="$1"
	started_ms="$2"
	ended_ms="$(boot_uptime_ms)"
	duration_ms=$((ended_ms - started_ms))
	log "stage=${stage} duration_ms=${duration_ms} uptime_ms=${ended_ms}"
}

show_boot_splash_once() {
	if [ -e "$BOOT_SPLASH_FLAG" ]; then
		log "boot_splash skipped already_shown=1 uptime_ms=$(boot_uptime_ms)"
		return 0
	fi
	if [ ! -r /usr/share/minui/splash.fb ] || [ ! -w /dev/fb0 ]; then
		log "boot_splash skipped missing_fb=1 uptime_ms=$(boot_uptime_ms)"
		return 0
	fi
	if [ "$MINUI_SESSION_SPLASH_FALLBACK" != "1" ]; then
		log "boot_splash skipped session_fallback_disabled=1 uptime_ms=$(boot_uptime_ms)"
		return 0
	fi
	log "boot_splash session_fallback start uptime_ms=$(boot_uptime_ms)"
	dd if=/usr/share/minui/splash.fb of=/dev/fb0 bs=4096 2>/dev/null || true
	: > "$BOOT_SPLASH_FLAG"
	log "boot_splash session_fallback done uptime_ms=$(boot_uptime_ms)"
}

seed_sdcard_layout() {
	src_root="$1"
	dst_root="$2"

	[ -d "$src_root" ] || return 0

	find "$src_root" -mindepth 1 -maxdepth 1 -type d | while IFS= read -r src_dir; do
		mkdir -p "$dst_root/$(basename "$src_dir")"
	done
}

ensure_game_media_layout() {
	dst_root="$1"

	mkdir -p \
		"${dst_root}/artwork" \
		"${dst_root}/bios" \
		"${dst_root}/collections" \
		"${dst_root}/roms" \
		"${dst_root}/saves"
}

seed_bezels_once() {
	dst_root="${SDCARD_PATH}/bezels"
	seeded_paths=""

	[ -d "$BEZELS_SRC_ROOT" ] || return 0
	mkdir -p "$dst_root"

	if [ -f "$BEZELS_SEED_MARKER" ]; then
		return 0
	fi

	seeded_paths="$(find "$dst_root" -mindepth 1 -maxdepth 1 -print -quit \
		2>/dev/null || true)"
	if [ -n "$seeded_paths" ]; then
		: > "$BEZELS_SEED_MARKER"
		return 0
	fi

	cp -a "${BEZELS_SRC_ROOT}/." "$dst_root/"
	: > "$BEZELS_SEED_MARKER"
}

link_tree_into() {
	src_root="$1"
	dst_root="$2"

	[ -d "$src_root" ] || return 0

	find "$src_root" -mindepth 1 | while IFS= read -r src; do
		rel="${src#${src_root}/}"
		[ "$rel" = "$src" ] && continue
		dst="${dst_root}/${rel}"
		if [ -d "$src" ]; then
			mkdir -p "$dst"
		else
			mkdir -p "$(dirname "$dst")"
			rm -rf "$dst"
			ln -s "$src" "$dst"
		fi
	done
}

rebuild_merged_bios_dir() {
	rm -rf "$MERGED_BIOS_PATH"
	mkdir -p "$MERGED_BIOS_PATH"
	link_tree_into "${SDCARD_PATH}/bios" "$MERGED_BIOS_PATH"
	if [ "$MINUI_SD2_READY" = "1" ] && mountpoint -q "$SDCARD2_PATH"; then
		link_tree_into "${SDCARD2_PATH}/bios" "$MERGED_BIOS_PATH"
	fi
}

setup_sd2() {
	rm -f "$SD2_PROMPT_FLAG"
	MINUI_SD2_PRESENT="0"
	MINUI_SD2_READY="0"

	[ -x "$SD2_HELPER" ] || return 0
	if ! "$SD2_HELPER" present >/dev/null 2>&1; then
		return 0
	fi

	MINUI_SD2_PRESENT="1"
	log "sd2 present=1 uptime_ms=$(boot_uptime_ms)"

	if ! "$SD2_HELPER" ready >/dev/null 2>&1; then
		: > "$SD2_PROMPT_FLAG"
		log "sd2 prompt reason=not-ready marker=${MINUI_SD2_MARKER_NAME} uptime_ms=$(boot_uptime_ms)"
		return 0
	fi

	if ! "$SD2_HELPER" mount >> "${SESSION_LOG_DIR}/sd2.log" 2>&1; then
		log "sd2 mount failed uptime_ms=$(boot_uptime_ms)"
		return 0
	fi

	if ! "$SD2_HELPER" initialized >/dev/null 2>&1; then
		: > "$SD2_PROMPT_FLAG"
		log "sd2 prompt reason=missing-marker marker=${MINUI_SD2_MARKER_NAME} uptime_ms=$(boot_uptime_ms)"
		return 0
	fi

	"$SD2_HELPER" ensure-layout >> "${SESSION_LOG_DIR}/sd2.log" 2>&1 || true
	ensure_game_media_layout "$SDCARD2_PATH"
	MINUI_SD2_READY="1"
	log "sd2 ready=1 uptime_ms=$(boot_uptime_ms)"
}

ensure_shared_userdata_layout() {
	mkdir -p "${USERDATA_PATH}/resume"
}

apply_system_timezone() {
	offset_minutes=180
	valid_offsets=",-720,-660,-600,-570,-540,-480,-420,-360,-300,-240,-210,-180,-150,-120,-60,0,60,120,180,210,240,270,300,330,345,360,390,420,480,525,540,570,600,630,660,720,765,780,825,840,"

	if [ -r "$TIMEZONE_STATE_PATH" ]; then
		offset_minutes="$(grep -m1 '^gmt_offset_minutes=' "$TIMEZONE_STATE_PATH" | cut -d= -f2)"
	fi

	case "$offset_minutes" in
		''|*[!0-9-]*) offset_minutes=180 ;;
	esac
	case "$valid_offsets" in
		*,"$offset_minutes",*) ;;
		*) offset_minutes=180 ;;
	esac

	if [ "$offset_minutes" -lt 0 ]; then
		label_sign="-"
		label_minutes=$(( -offset_minutes ))
	else
		label_sign="+"
		label_minutes=$offset_minutes
	fi

	label_hours=$(( label_minutes / 60 ))
	label_remainder=$(( label_minutes % 60 ))
	zone_name="$(printf 'GMT%s%02d_%02d' "$label_sign" "$label_hours" "$label_remainder")"
	zone_path="/usr/share/zoneinfo/minui/${zone_name}"

	if [ ! -f "$zone_path" ]; then
		zone_path="/usr/share/zoneinfo/minui/GMT+03_00"
		label_sign="+"
		label_hours=3
		label_remainder=0
	fi

	ln -sfn "$zone_path" /etc/localtime
	printf 'GMT %s%02d:%02d\n' "$label_sign" "$label_hours" \
		"$label_remainder" > /etc/timezone
}

cleanup_only() {
	rm -f "$EXEC_PATH"
	if [ -n "$SD2_FORMAT_SPLASH_PID" ]; then
		kill "$SD2_FORMAT_SPLASH_PID" 2>/dev/null || true
		wait "$SD2_FORMAT_SPLASH_PID" 2>/dev/null || true
		SD2_FORMAT_SPLASH_PID=""
	fi
	if [ -n "$KEYMON_PID" ]; then
		kill "$KEYMON_PID" 2>/dev/null || true
	fi
	if [ -n "$HDMIMON_PID" ]; then
		kill "$HDMIMON_PID" 2>/dev/null || true
	fi
}

cleanup_exit() {
	cleanup_only
	exit 0
}

start_sd2_format_splash() {
	SD2_FORMAT_SPLASH_PID=""
	if [ -x /usr/bin/bootsplash ] && [ -r /usr/share/minui/splash.fb ]; then
		/usr/bin/bootsplash --fb /dev/fb0 --image /usr/share/minui/splash.fb --unblank --overlay sd2-format --animate >/dev/null 2>&1 &
		SD2_FORMAT_SPLASH_PID=$!
		log "sd2 format splash pid=${SD2_FORMAT_SPLASH_PID} uptime_ms=$(boot_uptime_ms)"
	fi
}

stop_sd2_format_splash() {
	if [ -n "$SD2_FORMAT_SPLASH_PID" ]; then
		kill "$SD2_FORMAT_SPLASH_PID" 2>/dev/null || true
		wait "$SD2_FORMAT_SPLASH_PID" 2>/dev/null || true
		SD2_FORMAT_SPLASH_PID=""
	fi
}

start_saved_network_services() {
	WIFI_STATE_PATH="${SDCARD_PATH}/system/wifi/wifi.conf"
	BT_STATE_PATH="${SDCARD_PATH}/system/bluetooth/bluetooth.conf"
	WIFI_SETTING=1
	BT_SETTING=1

	if [ -r "$WIFI_STATE_PATH" ]; then
		WIFI_SETTING=$(grep -m1 "^wifi_enabled=" "$WIFI_STATE_PATH" | cut -d= -f2)
		case "$WIFI_SETTING" in
			0|1) ;;
			*) WIFI_SETTING=1 ;;
		esac
	fi

	if [ -r "$BT_STATE_PATH" ]; then
		BT_SETTING=$(grep -m1 "^bluetooth_enabled=" "$BT_STATE_PATH" | cut -d= -f2)
		case "$BT_SETTING" in
			0|1) ;;
			*) BT_SETTING=1 ;;
		esac
	fi

	if [ -x /etc/init.d/S40iwd ]; then
		log "starting iwd service (wifi_enabled=${WIFI_SETTING}) uptime_ms=$(boot_uptime_ms)"
		(
			log "S40iwd start uptime_ms=$(boot_uptime_ms)"
			/etc/init.d/S40iwd start >> "${SESSION_LOG_DIR}/wifi.log" 2>&1 || true

			if [ "$WIFI_SETTING" = "0" ]; then
				log "wifi_enabled=0, keeping iwd running and disabling radio uptime_ms=$(boot_uptime_ms)"
				rfkill block wifi >/dev/null 2>&1 || true
				ip link set wlan0 down >/dev/null 2>&1 || true
			fi
			) &
	fi

	if [ -x /etc/init.d/S40bluetoothd ] && [ "$BT_SETTING" = "1" ]; then
		log "starting bluetooth service (bluetooth_enabled=${BT_SETTING}) uptime_ms=$(boot_uptime_ms)"
		(
			log "S40bluetoothd start uptime_ms=$(boot_uptime_ms)"
			/etc/init.d/S40bluetoothd start >> "${SESSION_LOG_DIR}/bluetooth.log" 2>&1 || true
		) &
	fi
}

start_saved_network_services_deferred() {
	delay_secs="$MINUI_DEFER_SERVICES_SECS"
	case "$delay_secs" in
		''|*[!0-9]*) delay_secs=5 ;;
	esac
	if [ "$delay_secs" -le 0 ]; then
		log "network_services deferred=0 uptime_ms=$(boot_uptime_ms)"
		start_saved_network_services
		return 0
	fi
	log "network_services defer_secs=${delay_secs} uptime_ms=$(boot_uptime_ms)"
	(
		sleep "$delay_secs"
		log "network_services deferred_start uptime_ms=$(boot_uptime_ms)"
		start_saved_network_services
	) &
}
trap cleanup_exit INT TERM

# Show splash as early as possible (session fallback is opt-in; early init should have shown it already)
if [ "$MINUI_SESSION_SPLASH_FALLBACK" = "1" ]; then
	show_boot_splash_once
else
	log "boot_splash session_fallback disabled=1 uptime_ms=$(boot_uptime_ms)"
fi

log "session init start uptime_ms=$(boot_uptime_ms)"

stage_start_ms="$(boot_uptime_ms)"
mkdir -p "$SDCARD_PATH"
if ! mountpoint -q "$SDCARD_PATH"; then
	mount "$SDCARD_PATH" >/tmp/minui-mount.log 2>&1 || true
fi
mark_stage_end "mount_sdcard" "$stage_start_ms"

stage_start_ms="$(boot_uptime_ms)"
mkdir -p \
	"${SDCARD_PATH}/artwork" \
	"${SDCARD_PATH}/bios" \
	"${SDCARD_PATH}/collections" \
	"${SDCARD_PATH}/bezels" \
	"${SDCARD_PATH}/roms" \
	"${SDCARD_PATH}/saves" \
	"${SDCARD_PATH}/system" \
	"${USERDATA_PATH}" \
	"${LOGS_PATH}" \
	"${SDCARD_PATH}/system/wifi" \
	"${SDCARD_PATH}/system/bluetooth" \
	"${SDCARD_PATH}/system/ssh" \
	"${SDCARD_PATH}/system/audio" \
	"${SDCARD_PATH}/system/time" \
	"${SDCARD_PATH}/system/cache"
ensure_game_media_layout "$SDCARD_PATH"
ensure_shared_userdata_layout
seed_bezels_once
if [ ! -f "$AUDIO_CONF_PATH" ] && [ -f "$AUDIO_DEFAULT_CONF_SRC" ]; then
	cp -a "$AUDIO_DEFAULT_CONF_SRC" "$AUDIO_CONF_PATH" 2>/dev/null || true
elif [ ! -f "$AUDIO_CONF_PATH" ] && [ -f /etc/asound.conf ]; then
	cp -a /etc/asound.conf "$AUDIO_CONF_PATH" 2>/dev/null || true
fi
mark_stage_end "seed_layout" "$stage_start_ms"

if [ "$MINUI_SD_LOGS" = "1" ]; then
	SESSION_LOG_DIR="$LOGS_PATH"
else
	mkdir -p "$SESSION_LOG_DIR"
fi
BOOT_LOG="${SESSION_LOG_DIR}/boot.log"

touch "$BOOT_LOG"
log "session start uptime_ms=$(boot_uptime_ms)"

setup_sd2
rebuild_merged_bios_dir

apply_system_timezone

export RGXX_MODEL="${RGXX_MODEL:-RG35XXSP}"
export SDCARD_PATH
export SDCARD2_PATH
export MINUI_SD2_PRESENT
export MINUI_SD2_READY
export MINUI_BIOS_DIR
export BIOS_PATH="${MINUI_BIOS_DIR}"
export SAVES_PATH="${SDCARD_PATH}/saves"
export SYSTEM_PATH
export CORES_PATH
	export USERDATA_PATH
	export SHARED_USERDATA_PATH
	export LOGS_PATH
export DATETIME_PATH
export HDMI_EXPORT_PATH

# Keep HDMI env file valid even if hdmimon is not running.
echo "unset AUDIODEV" > "$HDMI_EXPORT_PATH"
echo "unset DEVICE" >> "$HDMI_EXPORT_PATH"
log "hdmi_export primed uptime_ms=$(boot_uptime_ms)"

# Delegate stale BlueALSA route recovery to the board bluetooth init script so
# audio-route policy lives in one place (S40bluetoothd).
if [ -x /etc/init.d/S40bluetoothd ]; then
	/etc/init.d/S40bluetoothd restore-audio-route-if-stale >/dev/null 2>&1 || true
fi

start_saved_network_services_deferred

if [ -x "/usr/libexec/minui/keymon" ]; then
	"/usr/libexec/minui/keymon" >> "${SESSION_LOG_DIR}/keymon.log" 2>&1 &
	KEYMON_PID=$!
	log "keymon pid=${KEYMON_PID} uptime_ms=$(boot_uptime_ms)"
else
	log "missing keymon"
fi

if [ -x "/usr/libexec/minui/hdmimon.sh" ]; then
	hdmimon_delay="$MINUI_DEFER_HDMIMON_SECS"
	case "$hdmimon_delay" in
		''|*[!0-9]*) hdmimon_delay=8 ;;
	esac
	(
		if [ "$hdmimon_delay" -gt 0 ]; then
			log "hdmimon defer_secs=${hdmimon_delay} uptime_ms=$(boot_uptime_ms)"
			sleep "$hdmimon_delay"
		fi
		log "hdmimon start uptime_ms=$(boot_uptime_ms)"
		exec "/usr/libexec/minui/hdmimon.sh" >> "${SESSION_LOG_DIR}/hdmimon.log" 2>&1
	) &
	HDMIMON_PID=$!
	log "hdmimon pid=${HDMIMON_PID} uptime_ms=$(boot_uptime_ms)"
fi

touch "$EXEC_PATH"
log "exec marker ready uptime_ms=$(boot_uptime_ms)"

# Persist GPU shader cache on SD card across firmware updates
export MESA_SHADER_CACHE_DIR="${SDCARD_PATH}/system/cache/mesa"
mkdir -p "$MESA_SHADER_CACHE_DIR"

while [ -f "$EXEC_PATH" ]; do
	if [ -f "$HDMI_EXPORT_PATH" ]; then
		. "$HDMI_EXPORT_PATH"
	fi

	if [ -x "/usr/bin/minui" ]; then
		log "starting minui uptime_ms=$(boot_uptime_ms)"
		"/usr/bin/minui" >> "${SESSION_LOG_DIR}/minui.log" 2>&1
		MINUI_RC=$?
		log "minui exited code=${MINUI_RC} uptime_ms=$(boot_uptime_ms)"
	else
		log "missing minui"
	fi

	if [ -f "$SD2_FORMAT_REQUEST_FLAG" ]; then
		rm -f "$SD2_FORMAT_REQUEST_FLAG"
		log "sd2 format request start uptime_ms=$(boot_uptime_ms)"
		start_sd2_format_splash
		if [ -x "$SD2_HELPER" ] && "$SD2_HELPER" format-init >> "${SESSION_LOG_DIR}/sd2.log" 2>&1; then
			stop_sd2_format_splash
			log "sd2 format request success reboot=1 uptime_ms=$(boot_uptime_ms)"
			sync
			reboot >/dev/null 2>&1 || reboot -f >/dev/null 2>&1 || poweroff >/dev/null 2>&1 || true
			sleep 2
		else
			stop_sd2_format_splash
			log "sd2 format request failed uptime_ms=$(boot_uptime_ms)"
		fi
	fi

	date +"%F %T" > "$DATETIME_PATH"

	if [ -f "$NEXT_PATH" ]; then
		log "next handoff begin uptime_ms=$(boot_uptime_ms)"
	else
		sync

		# Ensure clean black display during transition to shutdown/poweroff only.
		dd if=/dev/zero bs=1228800 count=1 of=/dev/fb0 2>/dev/null || true
	fi

	if [ -f "$NEXT_PATH" ]; then
		. "$NEXT_PATH"
		rm -f "$NEXT_PATH"
		date +"%F %T" > "$DATETIME_PATH"
		log "next handoff end uptime_ms=$(boot_uptime_ms)"
	fi

done

cleanup_only
log "Shutting down system..."
poweroff
