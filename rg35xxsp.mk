# rg35xxsp.mk
#
# Standalone, portable Makefile for building and deploying the Allium
# port for the Anbernic RG35xxSP (Allwinner H700, aarch64-musl).

ROOT_DIR = $(CURDIR)
BUILD_DIR = target/aarch64-unknown-linux-musl/release
DIST_DIR = dist-rg35xxsp

-include local.mk

.PHONY: rg35xxsp all build retroarch dist package-build image sdcard-image clean deploy

rg35xxsp: all

all: dist build retroarch package-build image

build:
	cargo zigbuild --release --target=aarch64-unknown-linux-musl --features=rg35xxsp --bin=alliumd --bin=allium-launcher --bin=activity-tracker --bin=screenshot-viewer --bin=screenshot --bin=say --bin=show --bin=play

retroarch:
	@echo "Preparing, patching, and building RetroArch inside OrbStack VM..."
	orb -m allium-sp-alpine -u $(shell id -un) -w /mnt/mac$(ROOT_DIR)/third-party/RetroArch-patch sh -lc " \
		make BUILD_DIR=build-rg35xxsp apply-patches assemble && \
		cd build-rg35xxsp && \
		CFLAGS=\"-DDINGUX\" ./configure --disable-qt --disable-cg --disable-vulkan --enable-alsa --enable-sdl2 --enable-udev --disable-oss --disable-pulse --disable-jack && \
		make DINGUX=1 -j\$\$\$$(nproc) \
	"
	mkdir -p $(DIST_DIR)/RetroArch
	cp third-party/RetroArch-patch/build-rg35xxsp/retroarch $(DIST_DIR)/RetroArch/retroarch

dist:
	mkdir -p $(DIST_DIR)
	rsync -a --exclude='.gitkeep' static/. $(DIST_DIR)/

package-build:
	mkdir -p $(DIST_DIR)/.allium/bin
	mkdir -p $(DIST_DIR)/.tmp_update/bin
	mkdir -p "$(DIST_DIR)/Apps/Activity Tracker.pak"
	mkdir -p "$(DIST_DIR)/Apps/Screenshot Viewer.pak"
	rsync -a $(BUILD_DIR)/alliumd $(DIST_DIR)/.allium/bin/
	rsync -a $(BUILD_DIR)/allium-launcher $(DIST_DIR)/.allium/bin/
	rsync -a $(BUILD_DIR)/screenshot $(DIST_DIR)/.tmp_update/bin/
	rsync -a $(BUILD_DIR)/say $(DIST_DIR)/.tmp_update/bin/
	rsync -a $(BUILD_DIR)/show $(DIST_DIR)/.tmp_update/bin/
	rsync -a $(BUILD_DIR)/activity-tracker "$(DIST_DIR)/Apps/Activity Tracker.pak/"
	rsync -a $(BUILD_DIR)/screenshot-viewer "$(DIST_DIR)/Apps/Screenshot Viewer.pak/"
	rsync -a $(BUILD_DIR)/play $(DIST_DIR)/.allium/bin/

image:
	# Trigger the self-contained Alpine rootfs builder in third-party/alpine
	if [ -d third-party/alpine ]; then \
		$(MAKE) -C third-party/alpine image; \
		rsync -a third-party/alpine/out/artifacts/ $(DIST_DIR)/; \
	fi
	cp -f scripts/flash-bootloader.command $(DIST_DIR)/
	chmod +x $(DIST_DIR)/flash-bootloader.command

# -- Multi-partition SD card image -------------------------------------------------

IMG_SIZE_MB ?= 2048
BOOT_SIZE_MB ?= 128
ROOT_SIZE_MB ?= 512
IMG_FILE := $(DIST_DIR)/allium-rg35xxsp.img

sdcard-image: image
	@echo "Building $(IMG_SIZE_MB)MB multi-partition SD card image..."
	@echo "Layout: BOOT=$(BOOT_SIZE_MB)MB FAT32 / ROOT=$(ROOT_SIZE_MB)MB EROFS / ALLIUM=rest FAT32"
	@set -eu; \
	if [ -z "$$(command -v orb 2>/dev/null)" ]; then \
		echo "ERROR: OrbStack CLI 'orb' not found" >&2; \
		exit 1; \
	fi; \
	if ! orb list --quiet 2>/dev/null | grep -qx 'allium-sp-alpine'; then \
		echo "ERROR: OrbStack machine 'allium-sp-alpine' not found" >&2; \
		echo "Run: make vm  (in third-party/alpine)" >&2; \
		exit 1; \
	fi; \
	orb -m allium-sp-alpine -u root sh -lc ' \
		set -eu; \
		dist="/mnt/mac$(ROOT_DIR)/$(DIST_DIR)"; \
		img="$$dist/allium-rg35xxsp.img"; \
		boot_mb=$(BOOT_SIZE_MB); \
		root_mb=$(ROOT_SIZE_MB); \
		img_mb=$(IMG_SIZE_MB); \
		boot_sec=$$((boot_mb * 2048)); \
		root_sec=$$((root_mb * 2048)); \
		img_sec=$$((img_mb * 2048)); \
		p1_start=2048; \
		p1_end=$$((p1_start + boot_sec - 1)); \
		p2_start=$$((p1_end + 1)); \
		p2_end=$$((p2_start + root_sec - 1)); \
		p3_start=$$((p2_end + 1)); \
		p3_end=$$((img_sec - 1)); \
		p3_sec=$$((p3_end - p3_start + 1)); \
		\
		echo "  Creating blank image ($${img_mb}MiB)..."; \
		rm -f "$$img"; \
		dd if=/dev/zero of="$$img" bs=512 count=$$img_sec status=none; \
		\
		echo "  Partitioning..."; \
		{ \
			echo "label: dos"; \
			echo "$$p1_start,$$boot_sec,c,*"; \
			echo "$$p2_start,$$root_sec,83"; \
			echo "$$p3_start,$$p3_sec,c"; \
		} | sfdisk --force "$$img"; \
		\
		echo "  Attaching loop device..."; \
		loop_dev=$$(losetup -f --show -P "$$img"); \
		trap "losetup -d $$loop_dev || true" EXIT INT TERM; \
		part1="$${loop_dev}p1"; \
		part2="$${loop_dev}p2"; \
		part3="$${loop_dev}p3"; \
		\
		echo "  Formatting BOOT as FAT32..."; \
		mkfs.fat -F 32 -n BOOT "$$part1"; \
		\
		echo "  Copying boot artifacts..."; \
		mkdir -p /tmp/sp-boot; \
		mount "$$part1" /tmp/sp-boot; \
		cp -f "$$dist/Image" "$$dist/sun50i-h700-anbernic-rg35xx-sp.dtb" \
		   "$$dist/uInitrd" "$$dist/boot.scr" /tmp/sp-boot/; \
		umount /tmp/sp-boot; \
		\
		echo "  Writing system.erofs to ROOT partition..."; \
		dd if="$$dist/system.erofs" of="$$part2" bs=4M status=none; \
		\
		echo "  Formatting ALLIUM as FAT32..."; \
		mkfs.fat -F 32 -n ALLIUM "$$part3"; \
		\
		echo "  Copying Allium data..."; \
		mkdir -p /tmp/sp-allium; \
		mount "$$part3" /tmp/sp-allium; \
		rsync -a \
			--exclude=Image \
			--exclude=sun50i-h700-anbernic-rg35xx-sp.dtb \
			--exclude=uInitrd \
			--exclude=boot.scr \
			--exclude=system.erofs \
			--exclude=u-boot-sunxi-with-spl.bin \
			--exclude=flash-bootloader.command \
			--exclude=linux \
			--exclude=u-boot \
			--exclude=.DS_Store \
			"$$dist/" /tmp/sp-allium/; \
		umount /tmp/sp-allium; \
		\
		losetup -d "$$loop_dev"; \
		trap - EXIT INT TERM; \
		echo "  Done. Size: $$(stat -c%s "$$img" | awk '\''{printf "%.0fMB", \$$1/1024/1024}'\'')"; \
	'; \
	echo ""; \
	echo "============================================================"; \
	echo "  SD card image: $(IMG_FILE)"; \
	echo ""; \
	echo "  Flash to SD card:"; \
	echo "    macOS:  sudo dd if=$(IMG_FILE) of=/dev/rdiskX bs=1m"; \
	echo "    Linux:  sudo dd if=$(IMG_FILE) of=/dev/sdX bs=4M status=progress"; \
	echo ""; \
	echo "  Then flash U-Boot SPL to sector 16:"; \
	echo "    sudo dd if=$(DIST_DIR)/u-boot-sunxi-with-spl.bin of=/dev/rdiskX bs=1024 seek=8"; \
	echo "============================================================"

clean:
	rm -rf $(DIST_DIR)
	rm -rf third-party/RetroArch-patch/build-rg35xxsp || true
	if [ -d third-party/alpine ]; then \
		$(MAKE) -C third-party/alpine clean; \
	fi

check-sdcard:
	@if [ -z "$(SDCARD_PATH)" ]; then \
		echo "ERROR: SDCARD_PATH is not set. Create a local.mk file with SDCARD_PATH=/path/to/sdcard or set it as an env variable." >&2; \
		exit 1; \
	fi

deploy: check-sdcard
	@echo "Deploying RG35xxSP Allium + Firmware to $(SDCARD_PATH)..."
	rsync --progress --modify-window=1 --update --recursive --times --verbose $(DIST_DIR)/ $(SDCARD_PATH)/
	@echo "Deployment complete! Remember to eject your SD card properly."
	@echo "============================================================"
	@echo "  One-time setup: flash the U-Boot bootloader to the card"
	@echo "  by running the flash-bootloader script included in $(DIST_DIR)/"
	@echo "============================================================"
