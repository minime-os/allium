#!/bin/sh

set -eu

BOARD_DIR="$(dirname "$0")"
SDCARD_SEED_DIR="${BINARIES_DIR}/sdcard-seed"

prepare_root_mounts() {
	sed -i '\#/mnt/SDCARD#d' "${TARGET_DIR}/etc/fstab"
	echo '/dev/mmcblk0p1 / erofs ro 0 1' >> "${TARGET_DIR}/etc/fstab"
	echo '/dev/mmcblk0p2 /mnt/SDCARD vfat rw,noatime,utf8,uid=0,gid=0,fmask=0000,dmask=0000 0 2' >> "${TARGET_DIR}/etc/fstab"
	mkdir -p "${TARGET_DIR}/boot" "${TARGET_DIR}/mnt/SDCARD"
}

install_boot_files() {
	mkimage -C none -A arm -T script -d "${BOARD_DIR}/boot.cmd" "${BINARIES_DIR}/boot.scr"
	cp -a "${BINARIES_DIR}/Image" "${TARGET_DIR}/boot/Image"
	cp -a "${BINARIES_DIR}/sun50i-h700-anbernic-rg35xx-sp.dtb" "${TARGET_DIR}/boot/"
	cp -a "${BINARIES_DIR}/boot.scr" "${TARGET_DIR}/boot/"
}

prepare_sdcard_seed() {
	rm -rf "${SDCARD_SEED_DIR}"
	mkdir -p \
		"${SDCARD_SEED_DIR}/BIOS" \
		"${SDCARD_SEED_DIR}/Roms" \
		"${SDCARD_SEED_DIR}/Saves" \
		"${SDCARD_SEED_DIR}/.allium/config" \
		"${SDCARD_SEED_DIR}/.allium/logs" \
		"${SDCARD_SEED_DIR}/.allium/state" \
		"${SDCARD_SEED_DIR}/.allium/bin" \
		"${SDCARD_SEED_DIR}/.allium/cores"

	if [ -d "${TARGET_DIR}/usr/share/allium-sdcard" ]; then
		cp -a "${TARGET_DIR}/usr/share/allium-sdcard/." "${SDCARD_SEED_DIR}/"
		rm -rf "${TARGET_DIR}/usr/share/allium-sdcard"
	fi

	cp -a "${BOARD_DIR}/asound.conf" "${SDCARD_SEED_DIR}/.allium/config/asound.conf"
	touch "${SDCARD_SEED_DIR}/.allium/config/wpa_supplicant.conf"
}

link_persistent_config() {
	rm -f "${TARGET_DIR}/etc/asound.conf"
	ln -snf /mnt/SDCARD/.allium/config/asound.conf "${TARGET_DIR}/etc/asound.conf"
	mkdir -p "${TARGET_DIR}/root/.ssh"
	ln -snf /mnt/SDCARD/.allium/config/authorized_keys "${TARGET_DIR}/root/.ssh/authorized_keys"
}

prepare_root_mounts
install_boot_files
prepare_sdcard_seed
link_persistent_config
