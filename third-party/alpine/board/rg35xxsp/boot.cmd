# Fallback addresses for sun50i-h700 if env vars are missing
if test -z ${kernel_addr_r}; then setenv kernel_addr_r 0x44000000; fi
if test -z ${fdt_addr_r}; then setenv fdt_addr_r 0x48000000; fi
if test -z ${ramdisk_addr_r}; then setenv ramdisk_addr_r 0x4a000000; fi

setenv bootargs console=ttyS0,115200 console=tty1 pm_async=off ignore_loglevel loglevel=7 printk.devkmsg=on consoleblank=0 vt.global_cursor_default=1 rtw88_core.disable_lps_deep=Y

echo "Loading kernel..."
fatload mmc 0:1 ${kernel_addr_r} Image

echo "Loading device tree..."
fatload mmc 0:1 ${fdt_addr_r} sun50i-h700-anbernic-rg35xx-sp.dtb

echo "Loading initramfs..."
fatload mmc 0:1 ${ramdisk_addr_r} uInitrd

fdt addr ${fdt_addr_r}
fdt resize

echo "Booting..."
booti ${kernel_addr_r} ${ramdisk_addr_r} ${fdt_addr_r}
