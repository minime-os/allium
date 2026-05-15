#!/bin/sh

# Shared helpers for Allium board init scripts. Keep functions POSIX-sh simple so
# Bluetooth can source this early during boot without extra deps.

simon_sdcard2_mounted() {
	mountpoint -q /mnt/SDCARD2 >/dev/null 2>&1
}

simon_sdcard_mounted() {
	mountpoint -q /mnt/SDCARD >/dev/null 2>&1
}

simon_device_mounted() {
	dev="$1"
	[ -n "$dev" ] || return 1
	awk -v dev="$dev" '$1 == dev { found=1 } END { exit(found ? 0 : 1) }' /proc/mounts >/dev/null 2>&1
}

simon_mount_source_for_target() {
	dst="$1"
	[ -n "$dst" ] || return 1
	awk -v dst="$dst" '$2 == dst { print $1; exit }' /proc/mounts 2>/dev/null
}

simon_bind_mount_active() {
	src="$1"
	dst="$2"
	[ -n "$src" ] && [ -n "$dst" ] || return 1
	awk -v src="$src" -v dst="$dst" '$1 == src && $2 == dst { found=1 } END { exit(found ? 0 : 1) }' /proc/mounts >/dev/null 2>&1
}
