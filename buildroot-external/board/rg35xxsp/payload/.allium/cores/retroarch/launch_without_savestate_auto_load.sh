#!/bin/sh
DIR=/mnt/SDCARD/RetroArch
CONFIG="$DIR/.retroarch/retroarch.cfg"

if [ -f "$CONFIG" ]; then
	cp "$CONFIG" /tmp/retroarch.cfg
	sed -i 's/savestate_auto_load = "true"/savestate_auto_load = "false"/g' /tmp/retroarch.cfg
	CONFIG=/tmp/retroarch.cfg
fi

ROOT=/mnt/SDCARD HOME="$DIR" exec "$DIR/retroarch" -v -L "$DIR/.retroarch/cores/$1_libretro.so" "$2" -c "$CONFIG"
