# Implementation Plan - RG35xxSP Port of Allium

We are designing and building a new port of Allium for the **Anbernic RG35xxSP** (Allwinner H700 chipset). This plan outlines our strategy to replace a heavy, hours-long Buildroot system with an ultra-lightweight, fast-to-build, and highly-maintainable custom OS based on an **LZ4-compressed EROFS loop image** running on a **single, writable FAT32 SD partition**.

---

## User Review Required

Please review the primary decisions we have made during the grilling phase:

> [!IMPORTANT]
> **Key Decisions to Approve:**
> 1. **Vanilla Allium Compatibility**: Allium will *not* live on the read-only root (`/`). It will run entirely from `/mnt/SDCARD/.allium/` (matching the Miyoo Mini). This allows users to update Allium simply by copy-pasting a new folder onto the FAT32 partition of their SD card, without flashing a new firmware image.
> 2. **Single Partition with LZ4-Compressed EROFS Loop Image**: The SD card is formatted as a single FAT32 partition (perfectly visible on any Windows/Mac). The firmware rootfs is packed as a file (`system.erofs`) compressed using high-ratio LZ4 (`mkfs.erofs -zlz4hc`). At boot, a tiny 2MB boot ramdisk (`uInitrd`) mounts this file as a loop device, shifts control via `switch_root`, and mounts the rest of the FAT32 filesystem at `/mnt/SDCARD`. This provides complete OS read-only protection with **zero extra RAM overhead** and **zero EROFS requirement in U-Boot**.
> 3. **Compilation Target**: Allium's RG35xxSP binaries will target `aarch64-unknown-linux-musl` (native to Alpine). We will bundle a minimal native GNU C Library (glibc) dynamic loader (`/lib/ld-linux-aarch64.so.1`) and its base libraries in EROFS so closed-source glibc binaries (DraStic, native Pico-8) run natively and flawlessly on `/mnt/SDCARD`.
> 4. **Graphics & Inputs Phase-in**: We will start with a simple `/dev/fb0` (framebuffer) display driver and standard `evdev` inputs. Once everything works, we will upgrade to DRM/KMS and hardware GPU acceleration (with Panfrost) for optimal performance and custom emulator shaders.
> 5. **Unmodified Allium Crates**: All Allium crates (such as `allium-launcher`, `allium-menu`, etc.) will remain 100% unmodified. We will not add custom paths, device-specific conditionals, or hacky logic inside the core applications. All hardware-specific integration will reside strictly within the `DefaultPlatform` implementation of the `common::platform` layer. This completely eliminates code churn and ensures that any upstream Allium updates will compile and run on the SP "as is."

---

## Proposed Changes

We will group our changes into a new platform feature (`rg35xxsp`) in Allium and introduce a new local build system for the firmware.

### 1. Self-Sufficient Build System & Packaging
To keep Allium completely self-sufficient and independent of external repos at build time, we will house our entire firmware build infrastructure under `third-party/alpine/` in the Allium repository. 

While we use `reference/sp` (which is git-ignored and not tracked in Allium's git history) as a reference blueprint for kernel configs, patches, and Alpine packaging, our own build folder will be **100% self-sufficient**. It will use **OrbStack** on macOS to run native ARM64 virtual machines for compiling:

#### [NEW] [third-party/alpine/](file:///Users/ilembitov/Projects/allium/third-party/alpine/)
Our dedicated, self-contained firmware build directory:
* **`Makefile`**: A custom Makefile leveraging OrbStack. Exposes targets like `make vm` (provisions the ARM64 builder container) and `make image` (compiles and packages the EROFS/uInitrd images).
* **`aports/`**: Local Alpine package build definitions (`APKBUILD`) for:
  * `linux-sp-rg35xxsp/`: The custom Allwinner H700 BSP kernel and screen/audio patches.
  * `u-boot-sp-rg35xxsp/`: Pre-configured bootloader builds.
  * `sp-base/`: Generic startup scripts, network tools, and local configs.
* **`board/rg35xxsp/`**: Board overlay configurations (e.g. `fstab`, `inittab`, OpenRC services configuration, and `genimage` configurations).
* **`world/`**: Base package selection sets (`sp.world`, `base.world`) defining the final system footprint.

#### [NEW] [rg35xxsp.mk](file:///Users/ilembitov/Projects/allium/rg35xxsp.mk)
A dedicated Makefile containing the entire build, target configuration, and packaging pipelines for the RG35xxSP. This keeps the SP build logic fully isolated:

```make
TARGET_TRIPLE = aarch64-unknown-linux-musl
BUILD_DIR = target/aarch64-unknown-linux-musl/release
DIST_DIR = dist-rg35xxsp

.PHONY: rg35xxsp all build package-build clean deploy deploy-all

rg35xxsp: all

all: dist build package-build clean-image image

build:
	cargo zigbuild --release --target=$(TARGET_TRIPLE) --features=rg35xxsp --bin=alliumd --bin=allium-launcher --bin=activity-tracker --bin=screenshot-viewer --bin=screenshot --bin=say --bin=show --bin=myctl --bin=play

# ... other targets like dist, package-build, clean, and deploy ...
```

#### [MODIFY] [Makefile](file:///Users/ilembitov/Projects/allium/Makefile)
The modification to the main Allium `Makefile` is kept incredibly **surgical**. By wrapping the entire original content in a conditional goal filter using GNU Make's native `MAKECMDGOALS`, we delegate all execution to `rg35xxsp.mk` if the `rg35xxsp` target is passed, ensuring absolute safety for the Miyoo Mini codebase and a native developer experience on macOS:

* **At the very top of `Makefile` (Line 1):**
  ```make
  ifneq (,$(filter rg35xxsp,$(MAKECMDGOALS)))
  include rg35xxsp.mk
  else
  ```
* **At the very bottom of `Makefile` (last line):**
  ```make
  endif
  ```

#### **How to Use the Unified API:**
* **`make rg35xxsp`**: Automatically compiles Allium for `aarch64-musl`, triggers the OrbStack VM, and outputs a complete bootable FAT32 seed to `dist-rg35xxsp/`.
* **`make deploy rg35xxsp`**: Syncs all game files, emulators, Allium binaries, AND the bootloader/firmware loop image to the mounted SD card. If the card isn't bootable, it prompts the developer with a simple `dd` command to flash U-Boot to sector 16 of their raw block device.
* **`make clean rg35xxsp`**: Cleans the specific `dist-rg35xxsp/` directory and triggers a clean within the `third-party/alpine/` builder VM.

---

### 2. Allium Core Porting (Platform Layer)
We will implement the standard Linux driver interface as a new platform target in the workspace.

#### [NEW] [rg35xxsp.rs](file:///Users/ilembitov/Projects/allium/crates/common/src/platform/rg35xxsp/mod.rs)
* **Design comment**: *Every module starts with a short comment explaining its function. Program flow is easy to read, with short single-responsibility functions (max 20 LOC, max 3 indent levels).*
* Implements the `Platform` trait for the `Rg35xxspPlatform` struct:
  * **Display**: Write ARGB/RGB565 pixels directly into mapped `/dev/fb0` memory.
  * **Inputs**: Non-blocking loop reading `/dev/input/event*` devices for D-pad, face buttons, menu button, volume keys, and the lid sensor.
  * **Battery**: Resolves stats through `/sys/class/power_supply/battery/`.
  * **Brightness**: Writes values to `/sys/class/backlight/backlight/brightness`.
  * **Audio**: Controls volume via ALSA control mixer (`amixer` or sound library FFI).

#### [MODIFY] [crates/common/src/platform/mod.rs](file:///Users/ilembitov/Projects/allium/crates/common/src/platform/mod.rs)
Integrate the new platform target under conditional compile features:
```rust
#[cfg(feature = "rg35xxsp")]
pub mod rg35xxsp;

#[cfg(feature = "rg35xxsp")]
pub type DefaultPlatform = rg35xxsp::Rg35xxspPlatform;
```

#### [MODIFY] [crates/common/Cargo.toml](file:///Users/ilembitov/Projects/allium/crates/common/Cargo.toml)
Add the `rg35xxsp` feature and its dependencies (e.g., `evdev`, `memmap2` or similar low-level utilities).

---

## Verification Plan

### Cross-Compilation & Packaging
* Verify that running the compilation scripts compiles the Allium source targeting `aarch64-unknown-linux-musl` and outputs clean binaries.
* Verify that `system.erofs` is compiled cleanly under 20MB and is inspectable on the host.

### Local Mocking & Emulation
* Build a local test runner or QEMU environment to smoke-test the generic Alpine rootfs before flashing the actual SD card.

### Hardware Boot Test
* Flash U-Boot and copy `Image`, `uInitrd`, `system.erofs`, and the Allium folder onto a single FAT32 SD card.
* Insert into the RG35xxSP and verify that U-Boot boots the kernel and tiny `uInitrd`, the ramdisk mounts `system.erofs` dynamically as a loop device at `/`, mounts the FAT32 partition to `/mnt/SDCARD`, and hands control over to `/mnt/SDCARD/.allium/bin/alliumd`.

---

## Base Firmware & OS Dependencies (Alpine Linux)

To ensure that the generic EROFS firmware partition can boot the system and run vanilla Allium/RetroArch flawlessly, the Alpine Linux rootfs must be provisioned with specific system packages, dynamically linked libraries, and kernel configuration drivers.

### 1. Userland Packages (`apk add`)
Since Allium operates strictly on the dynamic loader and interacts with standard POSIX commands, we need to install the following minimal dependencies during our Alpine rootfs bootstrap process:

* **`busybox`** (Default core in Alpine):
  * Provides base system utilities: `sh` (shell execution), `df` (disk space monitoring), `date` and `/sbin/hwclock` (RTC sync), `pkill` (process lifecycle control), `sync` (safe shutdown flushing), `reboot`/`poweroff` (power lifecycle).
  * Network utilities: `ip` (link querying), `telnetd` (Telnet server), `tcpsvd` and `ftpd` (which combine to provide our lightweight FTP server). All of these are built-in BusyBox applets, meaning we don't need any heavy external FTP or Telnet packages!
  * Console helper: `fbset` (framebuffer geometry adjustment).
* **`alsa-lib` & `alsa-utils`**:
  * Provides `libasound.so.2` and default ALSA sound configs. Required for emulator cores and sound card handoffs.
* **Minimal Glibc Environment & `libstdc++`**:
  * Bundled dynamic loader (`/lib/ld-linux-aarch64.so.1`) and standard libraries. Crucial for running closed-source, pre-compiled glibc binaries on musl Alpine (e.g. DraStic, native Pico-8, and other vendor glibc-linked RetroArch cores) natively at full speed.
* **`wpa_supplicant` & `dhcpcd`**:
  * Required to run the custom networking scripts (`wifi-on.sh`, etc.) to establish and acquire IP addresses.
* **`dropbear` or `openssh`**:
  * Necessary for starting SSH background terminals (`ssh-on.sh`).

### 2. Required Kernel Modules & sysfs Configurations
The custom Allwinner H700 BSP kernel must expose the following hardware interfaces to `/dev` and `/sys` so Allium's platform layer can run without code modifications:

* **Display Panel (`/dev/fb0`)**:
  * A working Linux framebuffer mapping physical pixels. (Phase 1).
* **Inputs (`evdev` / `/dev/input/event*`)**:
  * Key drivers matching the physical buttons (D-pad, face buttons, Menu, Power, Lid Close lid-switch, Vol+, Vol-) to standard Linux input values.
* **Audio SOC (`snd-soc-sunxi-...`)**:
  * Exposes default PCM output interfaces to ALSA.
* **Power Management & PMIC (AXP2202 sysfs)**:
  * Exposes battery levels through `/sys/class/power_supply/battery/` or `/sys/class/power_supply/axp2202-battery/`.
  * Exposes display backlight parameters through `/sys/class/backlight/backlight/brightness` (or generic backlight controls).

