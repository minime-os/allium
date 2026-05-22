#!/bin/sh

# ssh-on.sh - Start Dropbear SSH daemon on Miyoo Mini Plus
# Note: SSH must be enabled in Allium Settings > Network for this script to run.

set -e

dir=$(dirname "$0")
ROOT="${ROOT:-/mnt/SDCARD}"

if ! "$dir"/wait-for-wifi.sh; then
    exit 1
fi

cd /mnt/SDCARD/ || exit 1

# Ensure persistent state directory exists
mkdir -p /mnt/SDCARD/.allium/state/ssh

# Generate RSA host key if it doesn't exist
if [ ! -f /mnt/SDCARD/.allium/state/ssh/dropbear_rsa_host_key ]; then
    "$ROOT"/.allium/bin/dropbearkey -t rsa -f /mnt/SDCARD/.allium/state/ssh/dropbear_rsa_host_key
fi

# Start dropbear daemon.
#   -r : host key file
#   -B : allow blank password (Miyoo root has no password)
#   -p 22 : listen on port 22
"$ROOT"/.allium/bin/dropbear -r /mnt/SDCARD/.allium/state/ssh/dropbear_rsa_host_key -B -p 22
