#!/bin/sh

# ssh-on.sh - Start Dropbear SSH daemon on Miyoo Mini Plus
# Note: SSH must be enabled in Allium Settings > Network for this script to run.

set -e

dir=$(dirname "$0")
ROOT="${ROOT:-/mnt/SDCARD}"
LOG="/mnt/SDCARD/.allium/logs/ssh-on.log"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') $1" >>"$LOG"
}

log "=== ssh-on.sh starting ==="

if ! "$dir"/wait-for-wifi.sh; then
    log "wait-for-wifi.sh failed"
    exit 1
fi

cd /mnt/SDCARD/ || exit 1

# Ensure persistent state directory exists
mkdir -p /mnt/SDCARD/.allium/state/ssh

# Generate RSA host key if it doesn't exist
if [ ! -f /mnt/SDCARD/.allium/state/ssh/dropbear_rsa_host_key ]; then
    log "Generating dropbear RSA host key..."
    "$ROOT"/.allium/bin/dropbearkey -t rsa -f /mnt/SDCARD/.allium/state/ssh/dropbear_rsa_host_key
fi

# The Miyoo firmware often sets a non-empty root password.  dropbear's -B flag
# only allows a *blank* password to authenticate if the account's hash is also
# blank.  Try to clear root's password so login with just Enter works.
#
# If /etc/shadow is on a read-only squashfs we remount root RW first.
log "Attempting to clear root password for blank-password SSH..."
if [ -f /etc/shadow ]; then
    mount -o remount,rw / 2>/dev/null || true
    if sed -i 's/^root:[^:]*:/root::/' /etc/shadow 2>/dev/null; then
        log "Cleared root password in /etc/shadow"
    else
        log "Could not modify /etc/shadow (may be read-only)"
    fi
fi

# Also accept pubkey auth if the user placed an authorized_keys file on the SD card.
AUTH_KEYS_SRC="/mnt/SDCARD/.allium/state/ssh/authorized_keys"
AUTH_KEYS_DST="/root/.ssh/authorized_keys"
if [ -f "$AUTH_KEYS_SRC" ]; then
    mkdir -p /root/.ssh
    cp "$AUTH_KEYS_SRC" "$AUTH_KEYS_DST"
    chmod 700 /root/.ssh
    chmod 600 "$AUTH_KEYS_DST"
    log "Installed authorized_keys from SD card"
fi

# Start dropbear daemon.
#   -r : host key file
#   -B : allow blank password
#   -p 22 : listen on port 22
log "Starting dropbear..."
"$ROOT"/.allium/bin/dropbear -r /mnt/SDCARD/.allium/state/ssh/dropbear_rsa_host_key -B -p 22
log "dropbear started OK"
