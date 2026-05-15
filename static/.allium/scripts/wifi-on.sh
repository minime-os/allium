#!/bin/sh
# Adapted from DotUI-X: https://github.com/anzz1/DotUI-X/blob/master/extras/Tools/WiFi.pak/wifion.sh

WPA_CONF="${ALLIUM_WPA_SUPPLICANT_CONF:-/mnt/SDCARD/.allium/config/wpa_supplicant.conf}"
WPA_BIN="${ALLIUM_WPA_SUPPLICANT_BIN:-wpa_supplicant}"

if [ -f /proc/modules ] && grep -q 8188fu /proc/modules; then
	:
elif [ -f /mnt/SDCARD/.tmp_update/8188fu.ko ]; then
	insmod /mnt/SDCARD/.tmp_update/8188fu.ko
fi
ifconfig lo up
[ ! -x /customer/app/axp_test ] || /customer/app/axp_test wifion
[ ! -x /usr/sbin/rfkill ] || /usr/sbin/rfkill unblock wifi
sleep 2
ifconfig wlan0 up
"$WPA_BIN" -B -D nl80211 -iwlan0 -c "$WPA_CONF"
ln -sf /dev/null /tmp/udhcpc.log
udhcpc -i wlan0 -s /etc/init.d/udhcpc.script > /dev/null 2>&1 &
