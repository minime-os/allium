#!/bin/sh
#
# build-initrd.sh
#
# Builds the tiny boot helper ramdisk (uInitrd) for the RG35xxSP.
# Packs a statically linked BusyBox and a POSIX /init script that mounts
# partition 2 (EROFS) as the root filesystem and partition 3 (FAT32) as
# /mnt/SDCARD before pivoting into the real system.

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
mkdir -p "$WORK_DIR/sysroot"
mkdir -p "$WORK_DIR/sysroot/mnt/SDCARD"

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

# Mount the EROFS root partition (check SD1 and SD2 slots, partition 2)
ROOT_PART=""
for dev in /dev/mmcblk0p2 /dev/mmcblk1p2; do
    if mount -o ro "$dev" /sysroot 2>/dev/null; then
        ROOT_PART="$dev"
        break
    fi
done

if [ -z "$ROOT_PART" ]; then
    echo "ERROR: Could not find EROFS root partition (mmcblkXp2)!"
    exec /bin/sh
fi

echo "Mounted root from $ROOT_PART"

# Mount the ALLIUM FAT32 data partition (partition 3)
for dev in /dev/mmcblk0p3 /dev/mmcblk1p3; do
    if mount -o rw,flush "$dev" /sysroot/mnt/SDCARD 2>/dev/null; then
        echo "Mounted ALLIUM partition $dev at /mnt/SDCARD"
        break
    fi
done

if [ -z "$(mount | grep '/sysroot/mnt/SDCARD')" ]; then
    echo "WARNING: Could not mount ALLIUM partition (games will not persist)"
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
