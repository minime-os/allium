#!/bin/sh

dir=$(dirname "$0")
if "$dir"/wait-for-wifi.sh; then
    cd /mnt/sdcard || exit
    /usr/bin/dufs --allow-all --bind 0.0.0.0 --port 80 >/dev/null 2>&1 &
    exit 0
fi

exit 1
