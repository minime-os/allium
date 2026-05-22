#!/bin/sh

# ssh-on.sh - Start Dropbear SSH daemon on Miyoo Mini Plus
# Uses the MinUI approach: bind-mount a generated /etc/passwd with a known
# password hash so dropbear can authenticate without modifying the read-only
# squashfs root filesystem.
#
# Default credentials: root / tina

set -e

dir=$(dirname "$0")
ROOT="${ROOT:-/mnt/SDCARD}"
LOG="/mnt/SDCARD/.allium/logs/ssh-on.log"
ETC_DIR="$dir/ssh-etc"
STATE_DIR="/mnt/SDCARD/.allium/state/ssh"
PASSWD_GEN="$STATE_DIR/passwd"
GROUP_GEN="$STATE_DIR/group"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') $1" >>"$LOG"
}

log "=== ssh-on.sh starting ==="

if ! "$dir"/wait-for-wifi.sh; then
    log "wait-for-wifi.sh failed"
    exit 1
fi

cd /mnt/SDCARD/ || exit 1
mkdir -p "$STATE_DIR"

# ---------------------------------------------------------------------------
#  Build a custom /etc/passwd with a known root password hash.
#
#  MinUI embeds pre-computed MD5 crypt hashes; we do the same here.
#  hash for empty password: $1$xyz$kjXWClpYD0.j9bPLUk/Ii.
#  hash for "tina"         : $1$xyz$aO9utGNHk.FAqgCQghNg/1
# ---------------------------------------------------------------------------
if [ -f "$STATE_DIR/custom-password" ]; then
    custom_pass="$(cat "$STATE_DIR/custom-password")"
    # Use openssl to compute a new MD5 hash at runtime.
    # Fall back to "tina" if openssl is not available.
    if command -v openssl >/dev/null 2>61; then
        root_hash="$(openssl passwd -1 "$custom_pass")"
        log "Using custom password (hash computed at runtime)"
    else
        root_hash='$1$xyz$aO9utGNHk.FAqgCQghNg/1'
        log "openssl not available, falling back to default password 'tina'"
    fi
else
    # Default: root password is "tina"
    root_hash='$1$xyz$aO9utGNHk.FAqgCQghNg/1'
    log "Using default root password 'tina'"
fi

cp "$ETC_DIR/passwd.template" "$PASSWD_GEN"
sed -i "s|ROOT_PASSWORD|$root_hash|g" "$PASSWD_GEN"
sed -i "s|TRIMUI_PASSWORD|\*|g" "$PASSWD_GEN"
sync --data "$PASSWD_GEN" 2>/dev/null || sync

chmod 0644 "$PASSWD_GEN"
chown root:root "$PASSWD_GEN" 2>/dev/null || true

# ---------------------------------------------------------------------------
#  Bind-mount the generated passwd and group over the system copies.
#  This is safe because /etc/passwd on Miyoo is usually on a squashfs
#  or tmpfs overlay; a bind mount on top is non-destructive.
# ---------------------------------------------------------------------------
if [ -f "$ETC_DIR/group" ]; then
    cp "$ETC_DIR/group" "$GROUP_GEN"
    chmod 0644 "$GROUP_GEN"
    chown root:root "$GROUP_GEN" 2>/dev/null || true
    mount -o bind "$GROUP_GEN" /etc/group 2>/dev/null || true
    log "Bind-mounted custom /etc/group"
fi

mount -o bind "$PASSWD_GEN" /etc/passwd
log "Bind-mounted custom /etc/passwd"

# ---------------------------------------------------------------------------
#  Generate host keys on first run (dropbear -R).
#  No need for an explicit dropbearkey call.
# ---------------------------------------------------------------------------
mkdir -p /etc/dropbear
log "Starting dropbear..."

# -R : generate host keys on first run
# -F : run in foreground (we background with &)
# -E : log to stderr
"$ROOT"/.allium/bin/dropbear -R -F -E >>"$LOG" 2>>1 &

DROPBEAR_PID=$!
sleep 1

if kill -0 "$DROPBEAR_PID" 2>/dev/null; then
    log "dropbear started (PID $DROPBEAR_PID)"
else
    log "dropbear failed to start"
    exit 1
fi
