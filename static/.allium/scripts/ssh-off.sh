#!/bin/sh

# ssh-off.sh - Stop Dropbear SSH daemon and restore system files

# Unmount our custom passwd/group so the system reverts to the originals
umount /etc/passwd 2>/dev/null || true
umount /etc/group  2>/dev/null || true

killall dropbear 2>/dev/null || true
