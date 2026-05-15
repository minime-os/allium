#!/bin/sh
DIR=/mnt/SDCARD/RetroArch
ROOT=/mnt/SDCARD HOME="$DIR" exec "$DIR/retroarch" -v -L "$DIR/.retroarch/cores/$1_libretro.so" "$2"
