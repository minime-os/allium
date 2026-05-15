setenv bootargs console=ttyS0,115200 root=/dev/mmcblk0p1 rootwait ro rootfstype=erofs panic=10 pm_async=off quiet loglevel=3 vt.global_cursor_default=0
erofsload mmc 0:1 ${kernel_addr_r} /boot/Image
erofsload mmc 0:1 ${fdt_addr_r} /boot/sun50i-h700-anbernic-rg35xx-sp.dtb
booti ${kernel_addr_r} - ${fdt_addr_r}
