#!/bin/sh

PATH="/usr/sbin:/sbin:/usr/bin:/bin"
INIT_SCRIPT="/etc/init.d/S40bluetoothd"

if [ ! -x "$INIT_SCRIPT" ]; then
	echo "missing init script: $INIT_SCRIPT" >&2
	exit 1
fi

case "$1" in
	start|stop|restart|reload|scan-on|scan-off)
		exec "$INIT_SCRIPT" "$1"
		;;
	*)
		echo "Usage: $0 {start|stop|restart|reload|scan-on|scan-off}" >&2
		exit 1
		;;
esac
