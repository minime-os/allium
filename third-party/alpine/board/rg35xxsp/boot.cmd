setenv bootargs console=ttyS0,115200 console=tty1 root=/dev/mmcblk0p2 rootwait pm_async=off ignore_loglevel loglevel=7 printk.devkmsg=on consoleblank=0 vt.global_cursor_default=1 rtw88_core.disable_lps_deep=Y
erofsload mmc 0:1 ${kernel_addr_r} Image
erofsload mmc 0:1 ${fdt_addr_r} sun50i-h700-anbernic-rg35xx-sp.dtb
booti ${kernel_addr_r} - ${fdt_addr_r}
