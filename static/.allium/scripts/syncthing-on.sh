#!/bin/sh

dir=$(dirname "$0")
if "$dir"/wait-for-wifi.sh; then
    cd /mnt/sdcard || exit
    if [ ! -d "/mnt/sdcard/.syncthing/config" ]; then
        mkdir -p "/mnt/sdcard/.syncthing/config"
    fi
    /usr/bin/syncthing --gui-address=0.0.0.0:8384 --home=/mnt/sdcard/.syncthing/config \
        >/mnt/sdcard/.syncthing/serve.log 2>&1 &
    exit 0
fi

exit 1
