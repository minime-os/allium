# third-party/alpine/Makefile
#
# Custom self-contained Makefile for building the Allium Alpine firmware
# rootfs, custom kernel, bootloader, and ramdisk using OrbStack on macOS.

SHELL := /bin/sh

ROOT_DIR := $(CURDIR)
LINUX_ROOT := /mnt/mac$(ROOT_DIR)

ORB_MACHINE ?= allium-sp-alpine
ORB_DISTRO ?= alpine
ORB_ARCH ?= arm64
ORB_USER ?= $(shell id -un 2>/dev/null || printf '%s\n' builder)
APORT_JOBS ?= 1

OUT_DIR := $(ROOT_DIR)/out
PACKAGE_DIR := $(OUT_DIR)/packages/aarch64
PACKAGE_NOARCH_DIR := $(OUT_DIR)/packages/noarch
ROOTFS_DIR := $(OUT_DIR)/rootfs
ARTIFACT_DIR := $(OUT_DIR)/artifacts
APORTS_DIR := $(ROOT_DIR)/aports
BOARD_DIR := $(ROOT_DIR)/board/rg35xxsp
WORLD_DIR := $(ROOT_DIR)/world

APORTS := $(sort $(wildcard $(APORTS_DIR)/*/APKBUILD))
PACKAGES := $(notdir $(patsubst %/,%,$(dir $(APORTS))))

CORES := $(filter libretro-%,$(PACKAGES))
CORES := $(filter-out libretro-common,$(CORES))

IMAGE_PACKAGES := \
	board-rg35xxsp \
	linux-rg35xxsp

BOOTSTRAP_PACKAGES := \
	alpine-sdk \
	abuild-rootbld \
	bash \
	bc \
	bison \
	build-base \
	coreutils \
	curl \
	device-mapper \
	dtc \
	dosfstools \
	e2fsprogs \
	elfutils-dev \
	erofs-utils \
	f2fs-tools \
	findutils \
	flex \
	gawk \
	git \
	jq \
	linux-headers \
	openssl-dev \
	pahole \
	parted \
	patch \
	perl \
	perl-dev \
	pkgconf \
	python3 \
	python3-dev \
	rsync \
	swig \
	tar \
	u-boot-tools \
	xz \
	zstd \
	alsa-lib-dev \
	sdl2-dev \
	mesa-dev \
	eudev-dev \
	rust \
	cargo \
	rust-clippy \
	clang-dev \
	clang-libs

.DEFAULT_GOAL := help

.PHONY: help list vm shell image clean clean-image clean-vm
.PHONY: $(PACKAGES)

help:
	@printf '%s\n' \
		'Allium SP firmware build targets:' \
		'  make help              Show this help.' \
		'  make list              List local package targets.' \
		'  make vm                Create/update the OrbStack Alpine build VM.' \
		'  make shell             Open a shell in the build VM at this repo.' \
		'  make image             Build missing packages, rootfs, and loop image.' \
		'  make <package>         Build one package from aports/<package>.' \
		'  make clean             Remove local artifacts, keeping the VM.' \
		'  make clean-image       Remove rootfs/image artifacts, keeping APKs.' \
		'  make clean-vm          Delete the OrbStack VM.' \
		'  make clean-<package>   Clean one package build and APK artifacts.'

list:
	@printf '%s\n' $(PACKAGES)

vm:
	@set -eu; \
	command -v orb >/dev/null 2>&1 || { \
		printf '%s\n' "ERROR: OrbStack CLI 'orb' not found" >&2; \
		exit 1; \
	}; \
	if ! orb list --quiet 2>/dev/null | grep -qx '$(ORB_MACHINE)'; then \
		orb create -a '$(ORB_ARCH)' -u '$(ORB_USER)' '$(ORB_DISTRO)' \
			'$(ORB_MACHINE)'; \
	fi; \
	if ! orb list --running --quiet 2>/dev/null | \
		grep -qx '$(ORB_MACHINE)'; then \
		orb start '$(ORB_MACHINE)' >/dev/null; \
	fi; \
	orb -m '$(ORB_MACHINE)' -u root sh -lc ' \
		set -eu; \
		apk update; \
		apk add $(BOOTSTRAP_PACKAGES); \
		id -nG "$(ORB_USER)" | grep -qw abuild || \
			addgroup "$(ORB_USER)" abuild; \
	'; \
	orb -m '$(ORB_MACHINE)' -u '$(ORB_USER)' -w '$(LINUX_ROOT)' sh -lc ' \
		set -eu; \
		mkdir -p "$$HOME/.abuild"; \
		if ! find "$$HOME/.abuild" -maxdepth 1 -type f -name "*.rsa" | \
			grep -q .; then \
			abuild-keygen -a -n >/dev/null 2>&1; \
		fi; \
		privkey=$$(find "$$HOME/.abuild" -maxdepth 1 -type f \
			-name "*.rsa" | sort | tail -n1); \
		[ -n "$$privkey" ] || { \
			printf "%s\n" "ERROR: no abuild private key was generated" >&2; \
			exit 1; \
		}; \
		printf "%s\n" "PACKAGER_PRIVKEY=\"$$privkey\"" > \
			"$$HOME/.abuild/abuild.conf"; \
	'; \
	orb -m '$(ORB_MACHINE)' -u root sh -lc ' \
		set -eu; \
		user_home=$$(getent passwd "$(ORB_USER)" | cut -d: -f6); \
		[ -n "$$user_home" ] || user_home="/home/$(ORB_USER)"; \
		find "$$user_home/.abuild" -maxdepth 1 -type f -name "*.pub" \
			-exec cp -f {} /etc/apk/keys/ \;; \
	'; \
	printf '%s\n' "OrbStack machine '$(ORB_MACHINE)' is ready"

shell:
	@set -eu; \
	command -v orb >/dev/null 2>&1 || { \
		printf '%s\n' "ERROR: OrbStack CLI 'orb' not found" >&2; \
		exit 1; \
	}; \
	if ! orb list --quiet 2>/dev/null | grep -qx '$(ORB_MACHINE)'; then \
		printf '%s\n' \
			"ERROR: OrbStack machine '$(ORB_MACHINE)' does not exist; run make vm first" \
			>&2; \
			exit 1; \
	fi; \
	if ! orb list --running --quiet 2>/dev/null | \
		grep -qx '$(ORB_MACHINE)'; then \
		orb start '$(ORB_MACHINE)' >/dev/null; \
	fi; \
	orb -m '$(ORB_MACHINE)' -u '$(ORB_USER)' -w '$(LINUX_ROOT)' sh

$(PACKAGES):
	@make _build-aport APORT_NAME='$@'

clean-%:
	@pkg='$*'; \
	case " $(PACKAGES) " in \
		*" $$pkg "*) make _clean-aport APORT_NAME="$$pkg" ;; \
		*) \
			printf '%s\n' "ERROR: unknown package '$$pkg'" >&2; \
			exit 1; \
			;; \
	esac

image:
	@set -eu; \
	for pkg in $(IMAGE_PACKAGES); do \
		if [ -z "$$(find '$(OUT_DIR)/packages' -type f \
			-name "$$pkg-[0-9]*.apk" -print -quit 2>/dev/null)" ]; then \
			make "$$pkg"; \
		else \
			printf '%s\n' "Package $$pkg already staged"; \
		fi; \
	done
	@make _sync-local-repo
	@make _build-image

clean:
	@set -eu; \
	rm -rf '$(OUT_DIR)'; \
	for dir in '$(APORTS_DIR)'/*; do \
		[ -d "$$dir" ] || continue; \
		rm -rf "$$dir/src" "$$dir/pkg" "$$dir/dist"; \
	done

clean-image:
	@set -eu; \
	rm -rf '$(ROOTFS_DIR)'; \
	rm -rf '$(ARTIFACT_DIR)/Image' \
		'$(ARTIFACT_DIR)/sun50i-h700-anbernic-rg35xx-sp.dtb' \
		'$(ARTIFACT_DIR)/u-boot-sunxi-with-spl.bin' \
		'$(ARTIFACT_DIR)/boot.scr' \
		'$(ARTIFACT_DIR)/system.erofs' \
		'$(ARTIFACT_DIR)/uInitrd'
	@if command -v orb >/dev/null 2>&1 && \
		orb list --quiet 2>/dev/null | grep -qx '$(ORB_MACHINE)'; then \
		if orb list --running --quiet 2>/dev/null | grep -qx '$(ORB_MACHINE)'; then \
			orb -m '$(ORB_MACHINE)' -u root sh -lc 'rm -rf /tmp/sp-rootfs-build' >/dev/null 2>&1 || true; \
		fi; \
	fi

clean-vm:
	@set -eu; \
	command -v orb >/dev/null 2>&1 || { \
		printf '%s\n' "ERROR: OrbStack CLI 'orb' not found" >&2; \
		exit 1; \
	}; \
	if orb list --quiet 2>/dev/null | grep -qx '$(ORB_MACHINE)'; then \
		orb delete '$(ORB_MACHINE)'; \
	fi

.PHONY: _build-aport _clean-aport _sync-local-repo _build-image

_build-aport:
	@set -eu; \
	[ -n '$(APORT_NAME)' ] || { \
		printf '%s\n' 'ERROR: APORT_NAME is required' >&2; \
		exit 1; \
	}; \
	[ -d '$(APORTS_DIR)/$(APORT_NAME)' ] || { \
		printf '%s\n' "ERROR: unknown package '$(APORT_NAME)'" >&2; \
		exit 1; \
	}; \
	mkdir -p '$(OUT_DIR)' '$(PACKAGE_DIR)' '$(PACKAGE_NOARCH_DIR)' \
		'$(ARTIFACT_DIR)'; \
	command -v orb >/dev/null 2>&1 || { \
		printf '%s\n' "ERROR: OrbStack CLI 'orb' not found" >&2; \
		exit 1; \
	}; \
	if ! orb list --quiet 2>/dev/null | grep -qx '$(ORB_MACHINE)'; then \
		printf '%s\n' \
			"ERROR: OrbStack machine '$(ORB_MACHINE)' does not exist; run make vm first" \
			>&2; \
		exit 1; \
	fi; \
	if ! orb list --running --quiet 2>/dev/null | \
		grep -qx '$(ORB_MACHINE)'; then \
		orb start '$(ORB_MACHINE)' >/dev/null; \
	fi; \
	orb -m '$(ORB_MACHINE)' -u root sh -lc ' \
		set -eu; \
		user_home=$$(getent passwd "$(ORB_USER)" | cut -d: -f6); \
		[ -n "$$user_home" ] || user_home="/home/$(ORB_USER)"; \
		grep -qF "$$user_home/packages/aports" /etc/apk/repositories || \
			printf "%s\n" "$$user_home/packages/aports" >> /etc/apk/repositories; \
	'; \
	orb -m '$(ORB_MACHINE)' -u '$(ORB_USER)' -w '$(LINUX_ROOT)' sh -lc ' \
		set -eu; \
		[ -f "$$HOME/.abuild/abuild.conf" ] || { \
			printf "%s\n" \
				"ERROR: missing ~/.abuild/abuild.conf; run make vm first" \
				>&2; \
			exit 1; \
		}; \
		user_repo_root="$$HOME/packages/aports"; \
		user_repo_dir="$$user_repo_root/aarch64"; \
		user_noarch_dir="$$user_repo_root/noarch"; \
		mkdir -p "$$user_repo_dir" "$$user_noarch_dir"; \
		find "$$user_repo_dir" "$$user_noarch_dir" -maxdepth 1 -type f \
			\( -name "*.apk" -o -name "APKINDEX.tar.gz*" \) -delete; \
		if [ -d "$(LINUX_ROOT)/out/packages/aarch64" ]; then \
			find "$(LINUX_ROOT)/out/packages/aarch64" -maxdepth 1 \
				-type f -name "*.apk" -exec cp -f {} "$$user_repo_dir/" \;; \
		fi; \
		if [ -d "$(LINUX_ROOT)/out/packages/noarch" ]; then \
			find "$(LINUX_ROOT)/out/packages/noarch" -maxdepth 1 \
				-type f -name "*.apk" -exec cp -f {} "$$user_noarch_dir/" \;; \
			find "$(LINUX_ROOT)/out/packages/noarch" -maxdepth 1 \
				-type f -name "*.apk" -exec cp -f {} "$$user_repo_dir/" \;; \
		fi; \
		privkey=$$(sed -n \
			'\''s/^PACKAGER_PRIVKEY="\(.*\)"$$/\1/p'\'' \
			"$$HOME/.abuild/abuild.conf"); \
		[ -n "$$privkey" ] || { \
			printf "%s\n" \
				"ERROR: missing PACKAGER_PRIVKEY in ~/.abuild/abuild.conf" \
				>&2; \
				exit 1; \
		}; \
		for repo_dir in "$$user_repo_dir" "$$user_noarch_dir"; do \
			cd "$$repo_dir"; \
			set -- *.apk; \
			[ -f "$$1" ] || continue; \
			if [ "$$repo_dir" = "$$user_repo_dir" ]; then \
				apk index --rewrite-arch aarch64 -o APKINDEX.tar.gz "$$@" \
					>/dev/null; \
			else \
				apk index -o APKINDEX.tar.gz "$$@" >/dev/null; \
			fi; \
			abuild-sign -k "$$privkey" APKINDEX.tar.gz >/dev/null; \
		done; \
		cd "$(LINUX_ROOT)/aports/$(APORT_NAME)"; \
		if [ -x ./prepare-sources.sh ]; then \
			./prepare-sources.sh; \
		fi; \
		find . -name ".DS_Store" -delete 2>/dev/null || true; \
		abuild checksum || true; \
		env SP_ARTIFACT_DIR="$(LINUX_ROOT)/out/artifacts" \
			JOBS="$(APORT_JOBS)" abuild -rK; \
		find . -name ".DS_Store" -delete 2>/dev/null || true; \
	'; \
	make _sync-local-repo

_clean-aport:
	@set -eu; \
	[ -n '$(APORT_NAME)' ] || { \
		printf '%s\n' 'ERROR: APORT_NAME is required' >&2; \
		exit 1; \
	}; \
	[ -d '$(APORTS_DIR)/$(APORT_NAME)' ] || { \
		printf '%s\n' "ERROR: unknown package '$(APORT_NAME)'" >&2; \
		exit 1; \
	}; \
	rm -rf '$(APORTS_DIR)/$(APORT_NAME)/src' \
		'$(APORTS_DIR)/$(APORT_NAME)/pkg' \
		'$(APORTS_DIR)/$(APORT_NAME)/dist'; \
	find '$(OUT_DIR)/packages' -type f \
		\( -name '$(APORT_NAME)-[0-9]*.apk' -o \
		-name 'APKINDEX.tar.gz*' \) -delete 2>/dev/null || true; \
	if command -v orb >/dev/null 2>&1 && \
		orb list --quiet 2>/dev/null | grep -qx '$(ORB_MACHINE)'; then \
		if ! orb list --running --quiet 2>/dev/null | \
			grep -qx '$(ORB_MACHINE)'; then \
			orb start '$(ORB_MACHINE)' >/dev/null; \
		fi; \
		orb -m '$(ORB_MACHINE)' -u '$(ORB_USER)' sh -lc ' \
			set -eu; \
			find "$$HOME/packages/aports" -type f \
				\( -name "$(APORT_NAME)-[0-9]*.apk" -o \
				-name "APKINDEX.tar.gz*" \) -delete 2>/dev/null || true; \
		'; \
	fi

_sync-local-repo:
	@set -eu; \
	mkdir -p '$(PACKAGE_DIR)' '$(PACKAGE_NOARCH_DIR)' '$(ARTIFACT_DIR)'; \
	orb -m '$(ORB_MACHINE)' -u '$(ORB_USER)' -w '$(LINUX_ROOT)' sh -lc ' \
		set -eu; \
		package_root="$(LINUX_ROOT)/out/packages"; \
		package_dir="$$package_root/aarch64"; \
		package_noarch_dir="$$package_root/noarch"; \
		artifact_dir="$(LINUX_ROOT)/out/artifacts"; \
		user_repo_root="$$HOME/packages/aports"; \
		user_repo_dir="$$user_repo_root/aarch64"; \
		user_noarch_dir="$$user_repo_root/noarch"; \
		tmp_dir="$$artifact_dir/.extract"; \
		mkdir -p "$$package_dir" "$$package_noarch_dir" "$$artifact_dir"; \
		apkbuild_field() { \
			awk -F= -v key="$$2" '\'' \
				$$1 == key { \
					val = substr($$0, length(key) + 2); \
					gsub(/^"/, "", val); \
					gsub(/"$$/, "", val); \
					print val; \
					exit; \
				} \
			'\'' "$$1"; \
		}; \
		rebuild_index() { \
			repo_dir="$$1"; \
			find "$$repo_dir" -maxdepth 1 -type f \
				-name "APKINDEX.tar.gz*" -delete; \
			cd "$$repo_dir"; \
			set -- *.apk; \
			[ -f "$$1" ] || return 0; \
			apk index -o APKINDEX.tar.gz "$$@" >/dev/null; \
			privkey=$$(sed -n \
				'\''s/^PACKAGER_PRIVKEY="\(.*\)"$$/\1/p'\'' \
				"$$HOME/.abuild/abuild.conf"); \
			[ -n "$$privkey" ] || { \
				printf "%s\n" \
					"ERROR: missing PACKAGER_PRIVKEY in ~/.abuild/abuild.conf" \
					>&2; \
				return 1; \
			}; \
			abuild-sign -k "$$privkey" APKINDEX.tar.gz >/dev/null; \
		}; \
		if [ -d "$$user_repo_root" ]; then \
			find "$$package_dir" "$$package_noarch_dir" -maxdepth 1 \
				-type f -name "APKINDEX.tar.gz*" -delete; \
			find "$(LINUX_ROOT)/aports" -mindepth 2 -maxdepth 2 \
				-type f -name APKBUILD | sort | \
				while IFS= read -r apkbuild; do \
					pkg_name=$$(apkbuild_field "$$apkbuild" pkgname); \
					pkg_ver=$$(apkbuild_field "$$apkbuild" pkgver); \
					pkg_rel=$$(apkbuild_field "$$apkbuild" pkgrel); \
					pkg_arch=$$(apkbuild_field "$$apkbuild" arch); \
					[ -n "$$pkg_name" ] || continue; \
					apk_name="$$pkg_name-$$pkg_ver-r$$pkg_rel.apk"; \
					if [ "$$pkg_arch" = noarch ]; then \
						src_repo_dir="$$user_noarch_dir"; \
						dest_repo_dir="$$package_noarch_dir"; \
					else \
						src_repo_dir="$$user_repo_dir"; \
						dest_repo_dir="$$package_dir"; \
					fi; \
					if [ "$$pkg_arch" = noarch ]; then \
						if [ -f "$$src_repo_dir/$$apk_name" ]; then \
							apk_path="$$src_repo_dir/$$apk_name"; \
						elif [ -f "$$user_repo_dir/$$apk_name" ]; then \
							apk_path="$$user_repo_dir/$$apk_name"; \
						else \
							continue; \
						fi; \
						cp -f "$$apk_path" "$$package_noarch_dir/"; \
						cp -f "$$apk_path" "$$package_dir/"; \
					else \
						[ -f "$$src_repo_dir/$$apk_name" ] || continue; \
						cp -f "$$src_repo_dir/$$apk_name" \
							"$$dest_repo_dir/"; \
					fi; \
				done; \
			rebuild_index "$$package_dir"; \
			rebuild_index "$$package_noarch_dir"; \
		fi; \
		extract_latest_pkg_artifacts() { \
			pkg="$$1"; \
			dest_name="$$2"; \
			apk_path=$$(find "$$package_dir" -maxdepth 1 -type f \
				-name "$$pkg-*.apk" | sort | tail -n1); \
			[ -n "$$apk_path" ] || return 0; \
			rm -rf "$$tmp_dir/$$dest_name"; \
			mkdir -p "$$tmp_dir/$$dest_name"; \
			tar -xzf "$$apk_path" -C "$$tmp_dir/$$dest_name" \
				>/dev/null 2>&1 || return 1; \
			if [ -d "$$tmp_dir/$$dest_name/usr/lib/sp/artifacts/$$dest_name" ]; then \
				rm -rf "$$artifact_dir/$$dest_name"; \
				cp -a "$$tmp_dir/$$dest_name/usr/lib/sp/artifacts/$$dest_name" \
					"$$artifact_dir/"; \
			fi; \
		}; \
		rm -rf "$$tmp_dir" "$$artifact_dir/linux" "$$artifact_dir/u-boot" \
			"$$artifact_dir/Image" \
			"$$artifact_dir/sun50i-h700-anbernic-rg35xx-sp.dtb" \
			"$$artifact_dir/u-boot-sunxi-with-spl.bin" \
			"$$artifact_dir/boot.scr"; \
		mkdir -p "$$tmp_dir"; \
		extract_latest_pkg_artifacts linux-rg35xxsp linux; \
		extract_latest_pkg_artifacts u-boot-rg35xxsp u-boot; \
		[ -f "$$artifact_dir/linux/Image" ] && \
			cp -f "$$artifact_dir/linux/Image" "$$artifact_dir/Image"; \
		[ -f "$$artifact_dir/linux/sun50i-h700-anbernic-rg35xx-sp.dtb" ] && \
			cp -f "$$artifact_dir/linux/sun50i-h700-anbernic-rg35xx-sp.dtb" \
				"$$artifact_dir/sun50i-h700-anbernic-rg35xx-sp.dtb"; \
		[ -f "$$artifact_dir/u-boot/u-boot-sunxi-with-spl.bin" ] && \
			cp -f "$$artifact_dir/u-boot/u-boot-sunxi-with-spl.bin" \
				"$$artifact_dir/u-boot-sunxi-with-spl.bin"; \
		[ -f "$$artifact_dir/u-boot/boot.scr" ] && \
			cp -f "$$artifact_dir/u-boot/boot.scr" "$$artifact_dir/boot.scr"; \
		rm -rf "$$tmp_dir"; \
	'

_build-image:
	@set -eu; \
	mkdir -p '$(OUT_DIR)' '$(ARTIFACT_DIR)'; \
	[ -f '$(PACKAGE_DIR)/APKINDEX.tar.gz' ] || { \
		printf '%s\n' \
			'ERROR: local APK repository is missing; run make image again after packages build' \
			>&2; \
		exit 1; \
	}; \
	orb -m '$(ORB_MACHINE)' -u root sh -lc ' \
		set -eu; \
		workdir="/tmp/sp-rootfs-build"; \
		root="$$workdir/rootfs"; \
		mkdir -p "$$workdir"; \
		rm -rf "$$root"; \
		mkdir -p "$$root"; \
		cat "$(LINUX_ROOT)/world/base.world" "$(LINUX_ROOT)/world/rg35xxsp.world" > "$$workdir/world"; \
		mkdir -p "$$root/etc/apk/keys"; \
		cp -f /etc/apk/keys/* "$$root/etc/apk/keys/"; \
		user_home=$$(getent passwd "$(ORB_USER)" | cut -d: -f6); \
		[ -n "$$user_home" ] || user_home="/home/$(ORB_USER)"; \
		find "$$user_home/.abuild" -type f -name "*.pub" \
			-exec cp -f {} "$$root/etc/apk/keys/" \;; \
		cp "$(LINUX_ROOT)/board/rg35xxsp/repositories" \
			"$$workdir/repositories"; \
		printf "%s\n" "$(LINUX_ROOT)/out/packages" >> \
			"$$workdir/repositories"; \
		apk --root "$$root" --initdb \
			--repositories-file "$$workdir/repositories" \
			--update-cache add \
			$$(grep -v "^[[:space:]]*#" "$$workdir/world" | tr "\n" " "); \
		install -D -m 0644 "$(LINUX_ROOT)/board/rg35xxsp/overlay/etc/fstab" \
			"$$root/etc/fstab"; \
		install -D -m 0644 "$(LINUX_ROOT)/board/rg35xxsp/overlay/etc/inittab" \
			"$$root/etc/inittab"; \
		install -D -m 0644 "$(LINUX_ROOT)/board/rg35xxsp/overlay/etc/hostname" \
			"$$root/etc/hostname"; \
		install -D -m 0644 "$(LINUX_ROOT)/board/rg35xxsp/overlay/etc/conf.d/dropbear" \
			"$$root/etc/conf.d/dropbear"; \
		install -D -m 0644 "$(LINUX_ROOT)/board/rg35xxsp/overlay/etc/iwd/main.conf" \
			"$$root/etc/iwd/main.conf"; \
		if [ -f "$$root/lib/firmware/rtw88/rtw8821c_fw.bin.zst" ]; then \
			zstd -d -f -c "$$root/lib/firmware/rtw88/rtw8821c_fw.bin.zst" > \
				"$$root/lib/firmware/rtw88/rtw8821c_fw.bin"; \
		fi; \
		if [ -f "$$root/lib/firmware/rtl_bt/rtl8821cs_fw.bin.zst" ]; then \
			zstd -d -f -c "$$root/lib/firmware/rtl_bt/rtl8821cs_fw.bin.zst" > \
				"$$root/lib/firmware/rtl_bt/rtl8821cs_fw.bin"; \
		fi; \
		mkdir -p "$$root/etc/runlevels/boot" \
			"$$root/etc/runlevels/default" \
			"$$root/etc/runlevels/shutdown"; \
		while IFS="|" read -r level svc; do \
			[ -n "$$level" ] || continue; \
			svc_target="$${svc%%.*}"; \
			if [ "$$svc" != "$$svc_target" ]; then \
				ln -snf "$$svc_target" "$$root/etc/init.d/$$svc"; \
				svc_target="$$svc"; \
			fi; \
			ln -snf "../../init.d/$$svc_target" \
				"$$root/etc/runlevels/$$level/$$svc"; \
		done < "$(LINUX_ROOT)/board/rg35xxsp/openrc-services.conf"; \
	'; \
	orb -m '$(ORB_MACHINE)' -u '$(ORB_USER)' sh -lc ' \
		set -eu; \
		mkdir -p "$(LINUX_ROOT)/out/artifacts"; \
		mkimage -A arm64 -T script -C none -n "Allium Boot" -d "$(LINUX_ROOT)/board/rg35xxsp/boot.cmd" "$(LINUX_ROOT)/out/artifacts/boot.scr"; \
	'; \
	for required_file in \
		'$(ARTIFACT_DIR)/Image' \
		'$(ARTIFACT_DIR)/sun50i-h700-anbernic-rg35xx-sp.dtb' \
		'$(ARTIFACT_DIR)/boot.scr'; do \
		[ -f "$$required_file" ] || { \
			printf '%s\n' \
				"ERROR: missing required board artifact: $$required_file" >&2; \
			exit 1; \
		}; \
	done; \
	orb -m '$(ORB_MACHINE)' -u root sh -lc ' \
		set -eu; \
		root="/tmp/sp-rootfs-build/rootfs"; \
		art="$(LINUX_ROOT)/out/artifacts"; \
		stage=$$(mktemp -d /tmp/sp-image.XXXXXX); \
		trap '\''rm -rf "$$stage"'\'' EXIT INT TERM; \
		mkdir -p "$$art"; \
		rm -f "$$art/system.erofs" "$$art/uInitrd"; \
		mkdir -p "$$stage/rootfs"; \
		cp -a "$$root/." "$$stage/rootfs/"; \
		rm -rf "$$stage/rootfs/usr/lib/sp/preloaded-roms"; \
		if [ -f "$$stage/rootfs/bin/bbsuid" ]; then \
			chmod 4111 "$$stage/rootfs/bin/bbsuid"; \
		fi; \
		printf "%s\n" "Compiling lz4-compressed system.erofs image..."; \
		mkfs.erofs -zlz4hc "$$art/system.erofs" "$$stage/rootfs"; \
		printf "%s\n" "Executing ramdisk build script..."; \
		sh "$(LINUX_ROOT)/build-initrd.sh"; \
	'; \
	printf '%s\n' 'Firmware loop image and boot helper staged in $(ARTIFACT_DIR)'
