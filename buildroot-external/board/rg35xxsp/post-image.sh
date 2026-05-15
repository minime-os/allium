#!/bin/sh

set -eu

usage() {
	echo "Usage: ${0##*/} -c GENIMAGE_CONFIG_FILE" >&2
}

GENIMAGE_CFG=""
opts="$(getopt -n "${0##*/}" -o c: -- "$@")" || exit $?
eval set -- "$opts"
while true; do
	case "$1" in
		-c)
			GENIMAGE_CFG="$2"
			shift 2
			;;
		--)
			shift
			break
			;;
		*)
			usage
			exit 1
			;;
	esac
done

if [ -z "$GENIMAGE_CFG" ]; then
	usage
	exit 1
fi

GENIMAGE_TMP="${BUILD_DIR}/genimage.tmp"
ROOTPATH_TMP="$(mktemp -d)"
FINAL_IMG="${BINARIES_DIR}/allium-rg35xxsp.img"
FINAL_IMG_GZ="${FINAL_IMG}.gz"

trap "rm -rf \"${ROOTPATH_TMP}\"" EXIT

if [ -d "${BINARIES_DIR}/sdcard-seed" ]; then
	cp -a "${BINARIES_DIR}/sdcard-seed" "${ROOTPATH_TMP}/"
fi

rm -rf "${GENIMAGE_TMP}"
rm -f "${BINARIES_DIR}/sdcard.img" "${FINAL_IMG}" "${FINAL_IMG_GZ}"

genimage \
	--rootpath "${ROOTPATH_TMP}" \
	--tmppath "${GENIMAGE_TMP}" \
	--inputpath "${BINARIES_DIR}" \
	--outputpath "${BINARIES_DIR}" \
	--config "${GENIMAGE_CFG}"

if [ ! -f "${FINAL_IMG}" ]; then
	echo "ERROR: expected image not found: ${FINAL_IMG}" >&2
	exit 1
fi

gzip -f -9 "${FINAL_IMG}"
echo "Image produced: ${FINAL_IMG_GZ}"
