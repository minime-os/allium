#!/bin/bash
# flash-bootloader.command
#
# One-time bootloader installer for Allium RG35xxSP on macOS.
# Run this from the root of your SD card (or the extracted archive).
# Requires sudo for raw disk access.

set -euo pipefail

cd "$(dirname "$0")"

BOOTLOADER="u-boot-sunxi-with-spl.bin"

if [ ! -f "$BOOTLOADER" ]; then
    echo "Error: $BOOTLOADER not found in current directory." >&2
    echo "Make sure you extracted the full Allium archive here." >&2
    exit 1
fi

echo "=========================================="
echo " Allium RG35xxSP Bootloader Flasher"
echo "=========================================="
echo ""
echo "This will write the bootloader to your SD card."
echo "This only needs to be done ONCE per SD card."
echo ""

diskutil list external

echo ""
echo "From the list above, identify your SD card's disk number"
echo "(e.g., 4 for /dev/disk4)."
echo "WARNING: writing to the wrong disk will DESTROY data."
echo ""
read -rp "Enter disk number: " DISK
read -rp "Re-enter disk number to confirm: " CONFIRM

if [ "$DISK" != "$CONFIRM" ]; then
    echo "Confirmation mismatch. Aborting." >&2
    exit 1
fi

DEVICE="/dev/disk${DISK}"

echo ""
echo "About to write $BOOTLOADER -> $DEVICE at offset 8K"
echo "Press RETURN to continue or Ctrl-C to abort."
read

echo "Unmounting any mounted partitions on $DEVICE..."
diskutil unmountDisk "$DEVICE" 2>/dev/null || true

echo "Writing bootloader..."
# H700 Boot ROM reads SPL from offset 8KB (sector 16)
sudo dd if="$BOOTLOADER" of="$DEVICE" bs=1024 seek=8 conv=sync status=progress
sync

echo ""
echo "Done. Bootloader written to $DEVICE."
echo "Eject the card and insert it into your RG35xxSP."
