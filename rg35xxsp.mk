# rg35xxsp.mk
#
# Standalone, portable Makefile for building and deploying the Allium
# port for the Anbernic RG35xxSP (Allwinner H700, aarch64-musl).

ROOT_DIR = $(CURDIR)
BUILD_DIR = target/aarch64-unknown-linux-musl/release
DIST_DIR = dist-rg35xxsp

.PHONY: rg35xxsp all build retroarch dist package-build clean deploy deploy-all check-sdcard

rg35xxsp: all

all: dist build retroarch package-build image

build:
	cargo zigbuild --release --target=aarch64-unknown-linux-musl --features=rg35xxsp --bin=alliumd --bin=allium-launcher --bin=activity-tracker --bin=screenshot-viewer --bin=screenshot --bin=say --bin=show --bin=play

retroarch:
	@echo "Preparing, patching, and building RetroArch inside OrbStack VM..."
	orb -m allium-sp-alpine -u $(shell id -un) -w /mnt/mac$(ROOT_DIR)/third-party/RetroArch-patch sh -lc " \
		make apply-patches assemble && \
		cd build && \
		CFLAGS=\"-DDINGUX\" ./configure --disable-qt --disable-cg --disable-vulkan --enable-alsa --enable-sdl2 --enable-udev --disable-oss --disable-pulse --disable-jack && \
		make DINGUX=1 -j\$$(nproc) \
	"
	mkdir -p $(DIST_DIR)/RetroArch
	cp third-party/RetroArch-patch/build/retroarch $(DIST_DIR)/RetroArch/retroarch

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
	# Trigger the self-contained Alpine rootfs loop builder in third-party/alpine
	if [ -d third-party/alpine ]; then \
		$(MAKE) -C third-party/alpine image; \
		rsync -a third-party/alpine/out/artifacts/ $(DIST_DIR)/; \
	fi

clean:
	rm -rf $(DIST_DIR)
	if [ -d third-party/alpine ]; then \
		$(MAKE) -C third-party/alpine clean; \
	fi
	$(MAKE) -C third-party/RetroArch-patch clean || true

check-sdcard:
	@if [ -z "$(SDCARD_PATH)" ]; then \
		echo "ERROR: SDCARD_PATH is not set. Create a local.mk file with SDCARD_PATH=/path/to/sdcard or set it as an env variable." >&2; \
		exit 1; \
	fi

deploy: check-sdcard
	@echo "Deploying RG35xxSP Allium + Firmware to $(SDCARD_PATH)..."
	rsync --progress --modify-window=1 --update --recursive --times --verbose $(DIST_DIR)/ $(SDCARD_PATH)/
	@echo "Deployment complete! Remember to eject your SD card properly."

deploy-all: check-sdcard
	@echo "Deploying full RG35xxSP dist to $(SDCARD_PATH)..."
	rsync --progress --modify-window=1 --update --recursive --times --verbose --delete $(DIST_DIR)/ $(SDCARD_PATH)/
	@echo "Full deployment complete! Remember to eject your SD card properly."
