#!/bin/sh
#
# build-initrd.sh
#
# Builds the tiny, high-performance boot helper ramdisk (uInitrd) for the
# RG35xxSP. Packs a statically linked BusyBox and a POSIX `/init` script that
# dynamically loop-mounts our LZ4-compressed `system.erofs` firmware image
# from the FAT32 partition, and moves the writeable partition to `/mnt/SDCARD`.

set -eu

WORK_DIR=$(mktemp -d /tmp/initrd-build.XXXXXX)
trap 'rm -rf "$WORK_DIR"' EXIT INT TERM

echo "Building boot helper initramfs..."

# 1. Install static busybox in our builder environment
apk add --quiet busybox-static u-boot-tools

# 2. Create minimal directory layout
mkdir -p "$WORK_DIR/bin"
mkdir -p "$WORK_DIR/dev"
mkdir -p "$WORK_DIR/proc"
mkdir -p "$WORK_DIR/sys"
mkdir -p "$WORK_DIR/mnt/FAT32"
mkdir -p "$WORK_DIR/sysroot"

# 3. Copy static busybox and establish symlinks
cp /bin/busybox.static "$WORK_DIR/bin/busybox"
for applet in sh mount umount mkdir switch_root sleep echo cat grep; do
    ln -sf busybox "$WORK_DIR/bin/$applet"
done

# 4. Write custom POSIX /init script
cat << 'EOF' > "$WORK_DIR/init"
#!/bin/sh

# Mount virtual filesystems
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev

echo "=== Allium SP Boot Helper Active ==="

# Mount the FAT32 SD card partition (check SD1 and SD2 slots)
SD_PART=""
for dev in /dev/mmcblk0p1 /dev/mmcblk1p1; do
    if mount -t vfat -o ro "$dev" /mnt/FAT32 2>/dev/null; then
        SD_PART="$dev"
        break
    fi
done

if [ -z "$SD_PART" ]; then
    echo "ERROR: Could not find FAT32 SD card partition!"
    exec /bin/sh
fi

# Remount FAT32 as writeable now that we found it
mount -o remount,rw "$SD_PART" /mnt/FAT32

# Loop-mount the LZ4-compressed system EROFS loop image
echo "Mounting system.erofs loop image..."
if ! mount -o loop,ro /mnt/FAT32/system.erofs /sysroot; then
    echo "ERROR: Failed to loop-mount /mnt/FAT32/system.erofs!"
    exec /bin/sh
fi

# Move the FAT32 partition to be the active /mnt/SDCARD inside our new rootfs
mkdir -p /sysroot/mnt/SDCARD
if ! mount --move /mnt/FAT32 /sysroot/mnt/SDCARD; then
    echo "ERROR: Failed to shift FAT32 partition to /mnt/SDCARD!"
    exec /bin/sh
fi

# Clean up devtmpfs, sysfs, and proc so they don't block switch_root
umount /dev
umount /sys
umount /proc

# Execute switch_root into the EROFS system partition
echo "Shifting control to Alpine system rootfs..."
exec switch_root /sysroot /sbin/init
EOF

chmod +x "$WORK_DIR/init"

# 5. Pack into cpio and run mkimage to produce uInitrd
cd "$WORK_DIR"
find . -print0 | cpio --null -ov --format=newc 2>/dev/null | gzip -9 > /tmp/initrd.cpio.gz

mkdir -p /mnt/mac/Users/ilembitov/Projects/allium/third-party/alpine/out/artifacts
mkimage -A arm64 -O linux -T ramdisk -C gzip -d /tmp/initrd.cpio.gz \
    /mnt/mac/Users/ilembitov/Projects/allium/third-party/alpine/out/artifacts/uInitrd >/dev/null

rm -f /tmp/initrd.cpio.gz
echo "uInitrd successfully generated!"
