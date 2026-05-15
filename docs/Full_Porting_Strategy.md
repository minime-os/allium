# Full Porting Strategy: Anbernic RG35XXSP

## Overview
This document serves as the comprehensive research report and design specification for porting Allium to the Anbernic RG35XXSP. Unlike the Miyoo Mini, which relies on stock firmware in NAND, the RG35XXSP port will provide a complete, minimal OS built via Buildroot, using a Mainline Linux kernel and an EROFS-based root filesystem.

## 1. Design Decisions (Grilling Session Summary)

The following strategic decisions were reached through consultation:

*   **Architecture:** Target `aarch64` (64-bit) for the Allwinner H700 SoC to leverage modern performance, as there are no known 32-bit-only binary blob constraints.
*   **Graphics:** V1 uses `/dev/fb0` via `DRM_FBDEV_EMULATION`. A DRM/KMS backend can follow after the build-only image works.
*   **Audio:** Implement an **ALSA backend** in Allium userland, bypassing the Miyoo-specific SigmaStar (`mi_ao`) libraries.
*   **Input Handling:** Implement **Dynamic Discovery** by device name (e.g., matching "gpio-keys") using the `evdev` crate. This ensures robustness against kernel `eventX` number changes on Mainline.
*   **Build Integration:** The Buildroot environment will be **integrated** into the Allium monorepo via an external tree (`buildroot-external/`).
*   **Buildroot Sourcing:** The Makefile downloads pinned Buildroot `2026.02.1` into `dist/sp/`.
*   **Kernel Configuration:** Based on `make tinyconfig` for extreme minimality, manually adding only the strictly necessary modules identified from the RG35XXSP Alpine Linux config.
*   **Deployment:** Output a **Flashable SD Card Image (.img)** using Buildroot's `genimage` tool, containing U-Boot, Kernel, and EROFS partitions.
*   **Workspace Management:** Use subdirectories in `dist/` (`dist/miyoo`, `dist/sp`) to allow building for different targets without collisions.

## 2. Kernel Requirements (Allwinner H700 / Mainline 6.18+)

Based on the research of the Alpine Linux kernel configuration (`/Users/ilembitov/Projects/sp/board/rg35xxsp/linux.config`), the Buildroot kernel must include:

### Hardware Support
*   `CONFIG_ARCH_SUNXI=y`: Allwinner SoC support.
*   `CONFIG_PINCTRL_SUN50I_H616=y` / `CONFIG_SUN50I_H616_CCU=y`: Pin control and clock unit for H700.
*   `CONFIG_MMC_SUNXI=y`: SD card support.
*   `CONFIG_PWM_SUN4I=y` / `CONFIG_PWM_SUN20I=y`: Required for screen backlight.

### Display & GPU
*   `CONFIG_DRM_SUN4I=y`: Allwinner Display Engine driver.
*   `CONFIG_DRM_PANEL_MIPI_DSI=y`: MIPI DSI panel support.
*   `CONFIG_DRM_PANFROST=y`: Mali G31 GPU support (for future acceleration).
*   `CONFIG_FB_EFI=y` or `CONFIG_DRM_FBDEV_EMULATION=y`: For initial console output.

### Audio
*   `CONFIG_SND_SUNXI_SUN50IW9_CODEC=y` or `CONFIG_SND_SUN4I_CODEC=y`: On-board audio codec.
*   `CONFIG_SND_SOC=y` / `CONFIG_SND_ALSA=y`: Base audio subsystems.

### Input
*   `CONFIG_INPUT_EVDEV=y`: Generic input events.
*   `CONFIG_KEYBOARD_GPIO=y`: Handling the GPIO-based gaming buttons.
*   `CONFIG_INPUT_MISC=y` / `CONFIG_INPUT_GPIO_DECODER=y`: Potential lid/hall sensor support.

### Filesystems & Efficiency
*   `CONFIG_EROFS_FS=y` + `CONFIG_EROFS_FS_ZIP=y`: Mandatory for the read-only root partition.
*   `CONFIG_VFAT_FS=y`: Required for the writable `/mnt/SDCARD` partition. No writable root overlay is used.

## 3. Buildroot Strategy

### Sourcing Buildroot
The `Makefile` will be updated to:
1. Download Buildroot `2026.02.1` into `dist/sp/`.
2. Extract it as `dist/sp/buildroot`.
3. Use `BR2_EXTERNAL=$(ROOT_DIR)/buildroot-external`.
4. On Linux, run Buildroot directly. On macOS, dispatch the same Linux target through an amd64 OrbStack Ubuntu VM named `allium-buildroot-amd64`.
5. Keep Simon's build speed/perf defaults where compatible: Bootlin external toolchain, ccache, per-package directories, `-O3`, LTO, and `-fuse-ld=mold`. Simon uses Bootlin musl; Allium uses Bootlin aarch64 glibc for PICO-8/DraStic compatibility. Bootlin external toolchains are selected only on x86_64 hosts, so the macOS OrbStack VM is created as amd64 by default. The OrbStack bootstrap installs common host tools (`cmake`, `ccache`, `mold`, `ninja`) so Buildroot can reuse system tools where supported instead of building every host dependency.
6. On macOS, keep the Buildroot source and output tree on OrbStack's native storage (`/home/<user>/allium-sp`) and copy only `allium-rg35xxsp.img.gz` back to repo `dist/sp/images`. Some packages, including ncurses terminfo generation, behave differently on the macOS shared `/Users` filesystem.

### Init System
The RG35XXSP OS will use a minimal **SysV init** or **BusyBox init**. A custom init script will be provided to:
1. Mount essential filesystems (`/proc`, `/sys`, `/tmp`).
2. Mount the second partition of the SD card to `/mnt/SDCARD`.
3. Start the Allium daemon (`alliumd`) as the primary process.
4. Manage device nodes via `mdev` or `udevd`.

### Integrated Configuration (`buildroot-external`)
We will provide an external Buildroot tree in `buildroot-external/` containing:
*   `configs/allium_rg35xxsp_defconfig`: Hardware and package selection.
*   `board/rg35xxsp/`: `genimage.cfg`, post-build scripts, Simon Linux patches, U-Boot config, and kernel config.
*   `package/allium/`: The Buildroot package definition that compiles and installs the Allium userland.

## 4. Buildroot Dependency Checklist

The `buildroot-external` tree and the final OS must supply the following:

### Build-time Dependencies (Host)
*   `host-rustc`: To compile the Allium Rust binaries for `aarch64`.
*   `host-genimage`: To package the final SD card image.
*   `host-u-boot`: To build the secondary program loader for H700.
### Target Packages
*   **Networking:**
    *   `wpa_supplicant`: Mandatory for Wi-Fi support.
    *   `busybox`: Must be configured to include `ftpd`, `telnetd`, `tcpsvd`, `ifconfig`, and `udhcpc`.
    *   `wireless-regdb`: Required for Wi-Fi regulatory compliance.
*   **Libraries:**
    *   `alsa-lib` & `alsa-utils`: Necessary for the new ALSA audio backend.
    *   `libdrm`: For the graphics backend.
*   **Other:**
    *   `udev`: For managing device nodes and assisting in dynamic input discovery.
    *   `retroarch`: To be compiled from source for `aarch64`.
    *   `ca-certificates`: Required for OTA updates and Syncthing.
    *   `syncthing`: Use `aarch64` binaries (standard package in Buildroot).
    *   `dufs`: Compile for `aarch64`.
    *   `ntp`: `ntpd` and `hwclock` support via BusyBox for time synchronization.

## 5. Allium Userland & Script Refactoring

### Hardcoded Shell Scripts
Many of Allium's core services (Wi-Fi, FTP, Telnet) are managed via shell scripts in `static/.allium/scripts/`. These currently contain hardcoded Miyoo Mini paths (e.g., `/customer/app/wpa_supplicant`) and device-specific kernel modules (e.g., `8188fu.ko`).
*   **Refactoring Goal:** Update these scripts to use standard system paths (e.g., `wpa_supplicant` instead of `/customer/app/wpa_supplicant`) and handle device-specific module loading (e.g., `8821cs` for RG35XXSP) conditionally.

### Target Architecture
...

*   **Cargo Feature:** `rg35xxsp` (or `mainline`) to toggle:
    *   Graphics: Swap `framebuffer` crate for a DRM/KMS implementation.
    *   Audio: Swap SigmaStar FFI calls for standard ALSA calls.
    *   Inputs: Refactor `evdev.rs` to scan `/dev/input/` for devices by name instead of assuming `event0`.
*   **Static Assets:** Themes and cores will continue to be managed by the Makefile and rsync'd into the Buildroot overlay directory before the final image generation.

## 6. Makefile Structure Evolution

The Makefile will be refactored into a clear target-based structure:
1.  **`make miyoo`**: Prepares the 32-bit `dist/miyoo` folder.
2.  **`make sp`**: 
    *   Checks for and downloads Buildroot tarball into `dist/sp`.
    *   Extracts and overlays `buildroot-external`.
    *   Prepares themes/assets.
    *   Invokes Buildroot to produce `sdcard.img`.
3.  **`make simulator`**: Assembles the local development environment in `simulator/`.
