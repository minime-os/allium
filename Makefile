ROOT_DIR := $(shell pwd)
HOST_OS := $(shell uname -s)
BUILD_DIR := target/armv7-unknown-linux-gnueabihf/release
DIST_DIR := dist
SP_BUILDROOT_VERSION := 2026.02.1
SP_DIST_DIR := $(DIST_DIR)/sp
SP_BUILDROOT_DIR := $(SP_DIST_DIR)/buildroot
SP_BUILDROOT_TARBALL := $(SP_DIST_DIR)/buildroot-$(SP_BUILDROOT_VERSION).tar.xz
SP_BUILDROOT_URL := https://buildroot.org/downloads/buildroot-$(SP_BUILDROOT_VERSION).tar.xz
SP_EXTERNAL_DIR := $(ROOT_DIR)/buildroot-external
SP_BUILDROOT_HOST_STAMP := $(SP_BUILDROOT_DIR)/output/.allium-host-os
SP_DEFCONFIG := $(SP_EXTERNAL_DIR)/configs/allium_rg35xxsp_defconfig
SP_KERNEL_CONFIG := $(SP_EXTERNAL_DIR)/board/rg35xxsp/linux.config
SP_GENERATED_KERNEL_CONFIG := $(abspath $(SP_DIST_DIR))/linux.config
SP_PAYLOAD_DIR := $(abspath $(SP_DIST_DIR))/payload
SP_PAYLOAD_SCRIPT := $(ROOT_DIR)/scripts/build-sp-payload.sh
ORBSTACK_MACHINE ?= allium-buildroot-amd64
ORBSTACK_ARCH ?= amd64
SP_ORBSTACK_DIST_DIR ?= /home/$(USER)/allium-sp
RETROARCH := third-party/RetroArch-patch
TOOLCHAIN := mholdg16/miyoomini-toolchain:latest

TARGET_TRIPLE := armv7-unknown-linux-gnueabihf
GLIBC_VERSION := 2.28

comma := ,
FEATURES ?=

-include local.mk

.PHONY: all
all: dist build package-build $(DIST_DIR)/RetroArch/retroarch $(DIST_DIR)/.allium/bin/dufs $(DIST_DIR)/.allium/bin/syncthing $(DIST_DIR)/.allium/cores/drastic/drastic $(DIST_DIR)/Themes migrations strip-all

.PHONY: clean
clean:
	rm -r $(DIST_DIR) || true
	# Needs sudo because RetroArch build runs in docker as root
	cd $(RETROARCH) && sudo make clean || true

simulator-env: simulator/Themes
	mkdir -p simulator
	rsync -ar static/ simulator/

simulator/Themes:
	TEMP_DIR=$$(mktemp -d) && \
		git clone --depth 1 "$(THEMES_URL)" "$$TEMP_DIR" && \
		rsync -a "$$TEMP_DIR/Themes/" "simulator/Themes/" && \
		rm -rf "$$TEMP_DIR"

.PHONY: simulator
simulator: simulator-env
	RUST_LOG=debug RUST_BACKTRACE=1 ALLIUM_DATABASE=$(ROOT_DIR)/simulator/allium.db ALLIUM_BASE_DIR=$(ROOT_DIR)/simulator/.allium ALLIUM_SD_ROOT=$(ROOT_DIR)/simulator cargo run --bin $(bin) --features=simulator$(if $(FEATURES),$(comma)$(FEATURES)) $(args)

.PHONY: dist
dist:
	mkdir -p $(DIST_DIR)
	rsync -a --exclude='.gitkeep' static/. $(DIST_DIR)

.PHONY: sp
sp: sp-check
ifeq ($(HOST_OS),Linux)
	$(MAKE) sp-linux
else ifeq ($(HOST_OS),Darwin)
	$(MAKE) sp-orbstack
else
	$(error unsupported host OS for make sp: $(HOST_OS))
endif

.PHONY: sp-orbstack
sp-orbstack:
	@command -v orb >/dev/null 2>&1 || { \
		echo "OrbStack CLI 'orb' is required on macOS. Run ./scripts/setup-mac.sh, then open OrbStack once."; \
		exit 1; \
	}
	@orb -m "$(ORBSTACK_MACHINE)" uname -s >/dev/null 2>&1 || orb create --arch "$(ORBSTACK_ARCH)" ubuntu "$(ORBSTACK_MACHINE)"
	orb -m "$(ORBSTACK_MACHINE)" sh -lc 'for tool in gcc g++ make cmake ccache mold ninja curl; do command -v "$$tool" >/dev/null 2>&1 || missing=1; done; test -f /usr/include/openssl/bio.h || missing=1; if [ "$$missing" = 1 ]; then sudo apt-get update && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y bash bc bison build-essential bzip2 ca-certificates ccache cmake cpio curl file findutils flex g++ gcc git gzip libncurses-dev libssl-dev locales lzip make mold ninja-build patch perl python3 rsync sed tar unzip wget xz-utils zstd; fi'
	orb -m "$(ORBSTACK_MACHINE)" sh -lc 'if ! command -v cargo >/dev/null 2>&1; then curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal; fi'
	orb -m "$(ORBSTACK_MACHINE)" sh -lc '. "$$HOME/.cargo/env" 2>/dev/null || true; cd "$(ROOT_DIR)" && make sp-linux SP_DIST_DIR="$(SP_ORBSTACK_DIST_DIR)" SP_BUILDROOT_DIR="$(SP_ORBSTACK_DIST_DIR)/buildroot" SP_BUILDROOT_TARBALL="$(SP_ORBSTACK_DIST_DIR)/buildroot-$(SP_BUILDROOT_VERSION).tar.xz" SP_BUILDROOT_HOST_STAMP="$(SP_ORBSTACK_DIST_DIR)/buildroot/output/.allium-host-os"'
	orb -m "$(ORBSTACK_MACHINE)" sh -lc 'mkdir -p "$(ROOT_DIR)/$(SP_DIST_DIR)/images" && cp -a "$(SP_ORBSTACK_DIST_DIR)/images/allium-rg35xxsp.img.gz" "$(ROOT_DIR)/$(SP_DIST_DIR)/images/"'
	orb -m "$(ORBSTACK_MACHINE)" sh -lc 'rm -rf "$(ROOT_DIR)/$(SP_DIST_DIR)/payload" && cp -a "$(SP_ORBSTACK_DIST_DIR)/payload" "$(ROOT_DIR)/$(SP_DIST_DIR)/payload"'

.PHONY: sp-check
sp-check:
	@test -f "$(SP_DEFCONFIG)"
	@test -f "$(SP_KERNEL_CONFIG)"
	@grep -qx 'BR2_DL_DIR="$$(TOPDIR)/../dl"' "$(SP_DEFCONFIG)"
	@grep -qx 'BR2_TOOLCHAIN_EXTERNAL=y' "$(SP_DEFCONFIG)"
	@grep -qx 'BR2_TOOLCHAIN_EXTERNAL_BOOTLIN=y' "$(SP_DEFCONFIG)"
	@grep -qx 'BR2_TOOLCHAIN_EXTERNAL_BOOTLIN_AARCH64_GLIBC_STABLE=y' "$(SP_DEFCONFIG)"
	@grep -qx 'BR2_TARGET_LDFLAGS="-fuse-ld=mold"' "$(SP_DEFCONFIG)"
	@grep -qx 'BR2_CCACHE=y' "$(SP_DEFCONFIG)"
	@grep -qx 'BR2_OPTIMIZE_3=y' "$(SP_DEFCONFIG)"
	@grep -qx 'BR2_ENABLE_LTO=y' "$(SP_DEFCONFIG)"
	@! grep -q 'BR2_PER_PACKAGE_DIRECTORIES=y' "$(SP_DEFCONFIG)" || { echo "SP external payloads require a shared Buildroot host/sysroot"; exit 1; }
	@! grep -q 'BR2_PACKAGE_HOST_RUSTC=y' "$(SP_DEFCONFIG)" || { echo "SP Buildroot must not build Rust"; exit 1; }
	@! grep -q 'BR2_PACKAGE_LIBRETRO_.*=y' "$(SP_DEFCONFIG)" || { echo "SP Buildroot must not build libretro cores"; exit 1; }
	@test -x "$(SP_PAYLOAD_SCRIPT)"
	@grep -qx '# CONFIG_MODULES is not set' "$(SP_KERNEL_CONFIG)"
	@! grep -q '=m$$' "$(SP_KERNEL_CONFIG)" || { echo "kernel config contains modules"; grep '=m$$' "$(SP_KERNEL_CONFIG)"; exit 1; }
	@for option in \
		CONFIG_ARCH_SUNXI \
		CONFIG_MMC_SUNXI \
		CONFIG_DRM_FBDEV_EMULATION \
		CONFIG_DRM_SUN4I \
		CONFIG_DRM_SUN6I_DSI \
		CONFIG_DRM_PANEL_MIPI \
		CONFIG_BACKLIGHT_PWM \
		CONFIG_INPUT_EVDEV \
		CONFIG_KEYBOARD_GPIO \
		CONFIG_BATTERY_AXP20X \
		CONFIG_SND_SOC_SUNXI_SUN50IW9_CODEC \
		CONFIG_RTW88_8821CS \
		CONFIG_BT_HCIUART_RTL \
		CONFIG_EROFS_FS \
		CONFIG_VFAT_FS \
		CONFIG_DEVTMPFS_MOUNT; do \
		grep -qx "$$option=y" "$(SP_KERNEL_CONFIG)" || { echo "missing $$option=y"; exit 1; }; \
	done

.PHONY: sp-linux
sp-linux: sp-check sp-buildroot-host
	$(MAKE) -C $(SP_BUILDROOT_DIR) BR2_EXTERNAL=$(SP_EXTERNAL_DIR) allium_rg35xxsp_defconfig
	mkdir -p "$(SP_DIST_DIR)"
	sed 's#__ALLIUM_BOARD_FIRMWARE_DIR__#$(SP_EXTERNAL_DIR)/board/rg35xxsp/firmware#g' "$(SP_KERNEL_CONFIG)" > "$(SP_GENERATED_KERNEL_CONFIG)"
	sed -i 's#^BR2_LINUX_KERNEL_CUSTOM_CONFIG_FILE=.*#BR2_LINUX_KERNEL_CUSTOM_CONFIG_FILE="$(SP_GENERATED_KERNEL_CONFIG)"#' "$(SP_BUILDROOT_DIR)/.config"
	if grep -q '^BR2_PACKAGE_ALLIUM_PAYLOAD_DIR=' "$(SP_BUILDROOT_DIR)/.config"; then \
		sed -i 's#^BR2_PACKAGE_ALLIUM_PAYLOAD_DIR=.*#BR2_PACKAGE_ALLIUM_PAYLOAD_DIR="$(SP_PAYLOAD_DIR)"#' "$(SP_BUILDROOT_DIR)/.config"; \
	else \
		printf '%s\n' 'BR2_PACKAGE_ALLIUM_PAYLOAD_DIR="$(SP_PAYLOAD_DIR)"' >> "$(SP_BUILDROOT_DIR)/.config"; \
	fi
	$(MAKE) -C $(SP_BUILDROOT_DIR) BR2_EXTERNAL=$(SP_EXTERNAL_DIR) olddefconfig
	$(MAKE) sp-buildroot-deps
	$(MAKE) sp-payload
	$(MAKE) -C $(SP_BUILDROOT_DIR) BR2_EXTERNAL=$(SP_EXTERNAL_DIR)
	mkdir -p $(SP_DIST_DIR)/images
	cp -a $(SP_BUILDROOT_DIR)/output/images/allium-rg35xxsp.img.gz $(SP_DIST_DIR)/images/

.PHONY: sp-buildroot-deps
sp-buildroot-deps:
	$(MAKE) -C $(SP_BUILDROOT_DIR) BR2_EXTERNAL=$(SP_EXTERNAL_DIR) toolchain host-pkgconf alsa-lib zlib sdl2 libpng freetype eudev

.PHONY: sp-payload
sp-payload:
	ROOT_DIR="$(ROOT_DIR)" \
	SP_DIST_DIR="$(SP_DIST_DIR)" \
	SP_BUILDROOT_DIR="$(SP_BUILDROOT_DIR)" \
	SP_EXTERNAL_DIR="$(SP_EXTERNAL_DIR)" \
	SP_PAYLOAD_DIR="$(SP_PAYLOAD_DIR)" \
	RETROARCH_DIR="$(ROOT_DIR)/$(RETROARCH)" \
	"$(SP_PAYLOAD_SCRIPT)"

.PHONY: sp-buildroot-host
sp-buildroot-host: $(SP_BUILDROOT_DIR)/.stamp-extracted
	@if [ -d "$(SP_BUILDROOT_DIR)/output" ] && { [ ! -f "$(SP_BUILDROOT_HOST_STAMP)" ] || [ "$$(cat "$(SP_BUILDROOT_HOST_STAMP)")" != "$$(uname -s)-$$(uname -m)" ]; }; then \
		echo "Resetting Buildroot output for $$(uname -s)-$$(uname -m) host"; \
		rm -rf "$(SP_BUILDROOT_DIR)/output"; \
	fi
	@if [ -d "$(SP_BUILDROOT_DIR)/output/per-package" ] && [ ! -d "$(SP_BUILDROOT_DIR)/output/host" ]; then \
		echo "Resetting Buildroot output to create shared host/sysroot for external SP payloads"; \
		rm -rf "$(SP_BUILDROOT_DIR)/output"; \
	fi
	@mkdir -p "$(SP_BUILDROOT_DIR)/output"
	@printf '%s-%s\n' "$$(uname -s)" "$$(uname -m)" > "$(SP_BUILDROOT_HOST_STAMP)"

$(SP_BUILDROOT_TARBALL):
	mkdir -p $(SP_DIST_DIR)
	wget -O $@ $(SP_BUILDROOT_URL)

$(SP_BUILDROOT_DIR)/.stamp-extracted: $(SP_BUILDROOT_TARBALL)
	rm -rf $(SP_BUILDROOT_DIR)
	mkdir -p $(SP_DIST_DIR)
	tar -C $(SP_DIST_DIR) -xf $(SP_BUILDROOT_TARBALL)
	mv $(SP_DIST_DIR)/buildroot-$(SP_BUILDROOT_VERSION) $(SP_BUILDROOT_DIR)
	touch $@

third-party/my283:
	wget -O third-party/my283.tar.xz https://github.com/shauninman/miyoomini-toolchain-buildroot/raw/main/support/my283.tar.xz
	cd third-party/ && tar xf my283.tar.xz
	rm third-party/my283.tar.xz

.PHONY: build
build: third-party/my283
	cargo zigbuild --release --target=$(TARGET_TRIPLE).$(GLIBC_VERSION) --features=miyoo --bin=alliumd --bin=allium-launcher --bin=activity-tracker --bin=screenshot-viewer --bin=screenshot --bin=say --bin=show --bin=myctl --bin=play
	patchelf \
		--replace-needed third-party/my283/usr/lib/libcam_os_wrapper.so libcam_os_wrapper.so \
		--replace-needed third-party/my283/usr/lib/libmi_sys.so libmi_sys.so \
		target/$(TARGET_TRIPLE)/release/myctl

.PHONY: debug
debug: third-party/my283
	cargo zigbuild --target=$(TARGET_TRIPLE).$(GLIBC_VERSION) --features=miyoo --bin=alliumd --bin=allium-launcher --bin=activity-tracker --bin=screenshot-viewer --bin=screenshot --bin=say --bin=show --bin=myctl

.PHONY: strip-all
strip-all:
	docker run --rm -i -v $(ROOT_DIR):/root/workspace $(TOOLCHAIN) \
		find dist static migrations \
			-type f \
			-not -path "static/.tmp_update/8188fu.ko" \
			-not -path "dist/.tmp_update/8188fu.ko" \
			-exec sh -c 'file "{}" | grep "not stripped"' \; \
			-exec /opt/miyoomini-toolchain/usr/bin/arm-linux-gnueabihf-strip -s {} \;

.PHONY: package-build
package-build:
	mkdir -p $(DIST_DIR)/.allium/bin
	rsync -a $(BUILD_DIR)/alliumd $(DIST_DIR)/.allium/bin/
	rsync -a $(BUILD_DIR)/allium-launcher $(DIST_DIR)/.allium/bin/
	rsync -a $(BUILD_DIR)/screenshot $(DIST_DIR)/.tmp_update/bin/
	rsync -a $(BUILD_DIR)/say $(DIST_DIR)/.tmp_update/bin/
	rsync -a $(BUILD_DIR)/show $(DIST_DIR)/.tmp_update/bin/
	rsync -a $(BUILD_DIR)/activity-tracker "$(DIST_DIR)/Apps/Activity Tracker.pak/"
	rsync -a $(BUILD_DIR)/screenshot-viewer "$(DIST_DIR)/Apps/Screenshot Viewer.pak/"
	rsync -a $(BUILD_DIR)/myctl $(DIST_DIR)/.tmp_update/bin/
	rsync -a $(BUILD_DIR)/play $(DIST_DIR)/.allium/bin/
	@# Write version.txt: use git tag if available, otherwise nightly-<hash>
	@TAG=$$(git describe --exact-match --tags HEAD 2>/dev/null | grep -v '^nightly$$'); \
	if [ -n "$$TAG" ]; then \
		echo "$$TAG" > $(DIST_DIR)/.allium/version.txt; \
	else \
		echo "nightly-$$(git rev-parse --short HEAD)" > $(DIST_DIR)/.allium/version.txt; \
	fi

MIGRATIONS_DIR := $(DIST_DIR)/.allium/migrations
.PHONY: migrations
migrations: $(MIGRATIONS_DIR)/0000-retroarch-config/retroarch-config.zip $(MIGRATIONS_DIR)/0001-retroarch-core-overrides/retroarch-core-overrides.zip

$(MIGRATIONS_DIR)/0000-retroarch-config/retroarch-config.zip:
	migrations/0000-retroarch-config/package.sh

$(MIGRATIONS_DIR)/0001-retroarch-core-overrides/retroarch-core-overrides.zip:
	migrations/0001-retroarch-core-overrides/package.sh

.PHONY: retroarch
retroarch: $(RETROARCH)/retroarch

$(DIST_DIR)/RetroArch/retroarch: $(RETROARCH)/bin/retroarch_miyoo354
	cp "$(RETROARCH)/bin/retroarch_miyoo354" "$(DIST_DIR)/RetroArch/retroarch"

$(RETROARCH)/bin/retroarch_miyoo354:
	docker run --rm -v /$(ROOT_DIR)/$(RETROARCH):/root/workspace $(TOOLCHAIN) bash -c "source /root/.bashrc; make all"

$(DIST_DIR)/.allium/bin/dufs:
	cd third-party/dufs && LZMA_API_STATIC=1 cargo zigbuild --release --target=$(TARGET_TRIPLE).$(GLIBC_VERSION)
	cp "third-party/dufs/target/$(TARGET_TRIPLE)/release/dufs" "$(DIST_DIR)/.allium/bin/"

SYNCTHING_VERSION := "v2.0.10"
SYNCTHING_URL := "https://github.com/syncthing/syncthing/releases/download/$(SYNCTHING_VERSION)/syncthing-linux-arm-$(SYNCTHING_VERSION).tar.gz"
$(DIST_DIR)/.allium/bin/syncthing:
	TEMP_DIR=$$(mktemp --directory) && \
		wget "$(SYNCTHING_URL)" -O "$$TEMP_DIR/syncthing.tar.gz" && \
		tar xf "$$TEMP_DIR/syncthing.tar.gz" --directory="$$TEMP_DIR" && \
		mv "$$TEMP_DIR/syncthing-linux-arm-$(SYNCTHING_VERSION)/syncthing" "$(DIST_DIR)/.allium/bin/syncthing"

DRASTIC_URL := https://github.com/steward-fu/nds/releases/download/v1.8/drastic-v1.8_miyoo.zip
$(DIST_DIR)/.allium/cores/drastic/drastic:
	wget "$(DRASTIC_URL)" -O /tmp/drastic.zip
	mkdir -p $(DIST_DIR)/.allium/cores/drastic
	unzip -o /tmp/drastic.zip -d $(DIST_DIR)/.allium/cores/drastic
	rm /tmp/drastic.zip

THEMES_URL := https://github.com/goweiwen/Allium-Themes.git
$(DIST_DIR)/Themes:
	TEMP_DIR=$$(mktemp -d) && \
		git clone --depth 1 "$(THEMES_URL)" "$$TEMP_DIR" && \
		rsync -a "$$TEMP_DIR/Themes/" "$(DIST_DIR)/Themes/" && \
		rm -rf "$$TEMP_DIR"

.PHONY: lint
lint:
	cargo clippy --fix --allow-dirty --allow-staged --all-targets -- -D warnings
	cargo fmt --all

.PHONY: bump-version
bump-version: lint
	sed -i'' -e "s/^version = \".*\"/version = \"$(version)\"/" crates/allium-launcher/Cargo.toml
	sed -i'' -e "s/^version = \".*\"/version = \"$(version)\"/" crates/allium-menu/Cargo.toml
	sed -i'' -e "s/^version = \".*\"/version = \"$(version)\"/" crates/alliumd/Cargo.toml
	sed -i'' -e "s/^version = \".*\"/version = \"$(version)\"/" crates/activity-tracker/Cargo.toml
	sed -i'' -e "s/^version = \".*\"/version = \"$(version)\"/" crates/screenshot-viewer/Cargo.toml
	sed -i'' -e "s/^version = \".*\"/version = \"$(version)\"/" crates/common/Cargo.toml
	cargo check
	git add crates/allium-launcher/Cargo.toml
	git add crates/allium-menu/Cargo.toml
	git add crates/alliumd/Cargo.toml
	git add crates/activity-tracker/Cargo.toml
	git add crates/screenshot-viewer/Cargo.toml
	git add crates/common/Cargo.toml
	git add Cargo.lock
	git commit -m "chore: bump version to v$(version)"
	git tag "v$(version)" -a

.PHONY: deploy
deploy:
ifndef SDCARD_PATH
	$(error SDCARD_PATH is not set. Create a local.mk file with SDCARD_PATH=/path/to/sdcard or set it as an environment variable)
endif
	@echo "Deploying to $(SDCARD_PATH)..."
	rsync --progress --modify-window=1 --update --recursive --times --verbose $(DIST_DIR)/.allium $(DIST_DIR)/.tmp_update $(DIST_DIR)/Apps $(DIST_DIR)/RetroArch $(DIST_DIR)/Themes $(SDCARD_PATH)/
	@echo "Deployment complete! Remember to eject your SD card properly."

.PHONY: deploy-all
deploy-all:
ifndef SDCARD_PATH
	$(error SDCARD_PATH is not set. Create a local.mk file with SDCARD_PATH=/path/to/sdcard or set it as an environment variable)
endif
	@echo "Deploying full dist to $(SDCARD_PATH)..."
	rsync --progress --modify-window=1 --update --recursive --times --verbose --delete $(DIST_DIR)/.allium $(DIST_DIR)/.tmp_update $(DIST_DIR)/Apps $(DIST_DIR)/Themes $(SDCARD_PATH)/
	@echo "Full deployment complete! Remember to eject your SD card properly."

.PHONY: toolchain
toolchain:
	docker run --rm -it -v $(ROOT_DIR):/root/workspace $(TOOLCHAIN) bash
