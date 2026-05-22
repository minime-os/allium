#!/bin/sh

# ssh-off.sh - Stop Dropbear SSH daemon

killall dropbear 2>/dev/null || true
