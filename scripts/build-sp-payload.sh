#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=${ROOT_DIR:-$(pwd)}
DIST_DIR=${DIST_DIR:-"${ROOT_DIR}/dist/sp"}
SP_BUILDROOT_DIR=${SP_BUILDROOT_DIR:-"${DIST_DIR}/buildroot"}
SP_EXTERNAL_DIR=${SP_EXTERNAL_DIR:-"${ROOT_DIR}/buildroot-external"}
PAYLOAD_DIR=${SP_PAYLOAD_DIR:-"${DIST_DIR}/payload"}
CORES_BUILD_DIR=${SP_CORES_BUILD_DIR:-"${DIST_DIR}/cores-build"}
RETROARCH_DIR=${RETROARCH_DIR:-"${ROOT_DIR}/third-party/RetroArch-patch"}
TARGET_TRIPLE=aarch64-unknown-linux-gnu
HOST_DIR="${SP_BUILDROOT_DIR}/output/host"
SYSROOT_DIR="${HOST_DIR}/aarch64-buildroot-linux-gnu/sysroot"
CROSS_PREFIX="${HOST_DIR}/bin/aarch64-linux-"
MAKE_CMD=${MAKE:-make}

ALLIUM_BINS=(
	alliumd
	allium-launcher
	allium-menu
	activity-tracker
	screenshot-viewer
	screenshot
	say
	show
	play
)

prepare_payload() {
	rm -rf "${PAYLOAD_DIR}"
	mkdir -p "${PAYLOAD_DIR}"
	rsync -a --exclude='.gitkeep' --exclude='.DS_Store' "${ROOT_DIR}/static/." "${PAYLOAD_DIR}/"
	rm -rf "${PAYLOAD_DIR}/Apps" "${PAYLOAD_DIR}/RetroArch" "${PAYLOAD_DIR}/.tmp_update"
	rm -rf "${PAYLOAD_DIR}/.allium/cores"
	mkdir -p "${PAYLOAD_DIR}/.allium/bin" "${PAYLOAD_DIR}/.allium/cores/retroarch"
	mkdir -p "${PAYLOAD_DIR}/RetroArch/.retroarch/cores"
}

write_version() {
	local version
	version=$(git -C "${ROOT_DIR}" describe --tags --dirty --always 2>/dev/null || git -C "${ROOT_DIR}" rev-parse --short HEAD)
	printf 'rg35xxsp-%s\n' "${version}" > "${PAYLOAD_DIR}/.allium/version.txt"
}

install_board_payload() {
	rsync -a "${SP_EXTERNAL_DIR}/board/rg35xxsp/payload/." "${PAYLOAD_DIR}/"
	find "${PAYLOAD_DIR}" -name .DS_Store -delete
	chmod +x "${PAYLOAD_DIR}/.allium/cores/retroarch/"*.sh
}

build_allium() {
	local cargo_args=()
	local rustflags="-C target-feature=-crt-static --cfg tokio_unstable"
	local bin
	if command -v rustup >/dev/null 2>&1; then
		rustup target add "${TARGET_TRIPLE}" >/dev/null
	elif ! command -v cargo >/dev/null 2>&1; then
		echo "cargo or rustup is required to build the SP Allium payload" >&2
		return 1
	fi
	for bin in "${ALLIUM_BINS[@]}"; do
		cargo_args+=(--bin "${bin}")
	done
	cd "${ROOT_DIR}"
	env -u CARGO_ENCODED_RUSTFLAGS \
	CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="${CROSS_PREFIX}gcc" \
	CC_aarch64_unknown_linux_gnu="${CROSS_PREFIX}gcc" \
	AR_aarch64_unknown_linux_gnu="${CROSS_PREFIX}gcc-ar" \
	CFLAGS_aarch64_unknown_linux_gnu="--sysroot=${SYSROOT_DIR}" \
	RUSTFLAGS="${rustflags}" \
	PKG_CONFIG="${HOST_DIR}/bin/pkg-config" \
	PKG_CONFIG_ALLOW_CROSS=1 \
	PKG_CONFIG_SYSROOT_DIR="${SYSROOT_DIR}" \
	PKG_CONFIG_PATH="${SYSROOT_DIR}/usr/lib/pkgconfig:${SYSROOT_DIR}/usr/share/pkgconfig" \
	CARGO_TARGET_DIR="${DIST_DIR}/cargo-target" \
	cargo build --release --target "${TARGET_TRIPLE}" --features rg35xxsp \
		"${cargo_args[@]}"
	for bin in "${ALLIUM_BINS[@]}"; do
		install -D -m 0755 "${DIST_DIR}/cargo-target/${TARGET_TRIPLE}/release/${bin}" "${PAYLOAD_DIR}/.allium/bin/${bin}"
	done
}

configure_retroarch() {
	cd "${RETROARCH_DIR}/build"
		PKG_CONFIG="${HOST_DIR}/bin/pkg-config" \
		PKG_CONF_PATH="${HOST_DIR}/bin/pkg-config" \
		PKG_CONFIG_SYSROOT_DIR="${SYSROOT_DIR}" \
		PKG_CONFIG_LIBDIR="${SYSROOT_DIR}/usr/lib/pkgconfig:${SYSROOT_DIR}/usr/share/pkgconfig" \
		CROSS_COMPILE="${CROSS_PREFIX}" \
		CC="${CROSS_PREFIX}gcc" \
	CXX="${CROSS_PREFIX}g++" \
	AR="${CROSS_PREFIX}gcc-ar" \
	STRIP="${CROSS_PREFIX}strip" \
	CFLAGS="-O3 -flto --sysroot=${SYSROOT_DIR}" \
	CXXFLAGS="-O3 -flto --sysroot=${SYSROOT_DIR}" \
	LDFLAGS="-fuse-ld=mold -flto --sysroot=${SYSROOT_DIR}" \
	./configure --host=aarch64-linux --prefix=/usr \
		--disable-x11 --disable-wayland --disable-kms --disable-egl \
		--disable-opengl --disable-opengl1 --disable-opengl_core \
		--disable-opengles --disable-vulkan --disable-ffmpeg \
		--disable-qt --disable-pulse --disable-jack --disable-oss \
		--disable-tinyalsa --disable-flac --disable-ssl \
		--enable-sdl2 --enable-alsa --enable-udev \
		--enable-command --enable-rgui \
		--disable-materialui --disable-xmb --disable-ozone
}

build_retroarch() {
	local jobs
	jobs=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
	"${MAKE_CMD}" -C "${RETROARCH_DIR}" assemble
	patch -d "${RETROARCH_DIR}/build" -p1 < "${SP_EXTERNAL_DIR}/board/rg35xxsp/patches/retroarch/10001_rg35xxsp_root_fallback.patch"
	rm -rf "${RETROARCH_DIR}/build/obj-unix"
	configure_retroarch
	"${MAKE_CMD}" -C "${RETROARCH_DIR}/build" -j"${jobs}"
	install -D -m 0755 "${RETROARCH_DIR}/build/retroarch" "${PAYLOAD_DIR}/RetroArch/retroarch"
}

copy_core_info() {
	local core=$1
	local source="${ROOT_DIR}/static/RetroArch/.retroarch/cores/${core}_libretro.info"
	test ! -f "${source}" || cp -a "${source}" "${PAYLOAD_DIR}/RetroArch/.retroarch/cores/"
}

build_core() {
	local name=$1 repo=$2 makefile=$3 path=$4 output=$5 platform=$6 extra=$7 install_name=$8 package=$9 extra_include=${10:-}
	local workdir="${CORES_BUILD_DIR}/${name}"
	local patch_dir="${SP_EXTERNAL_DIR}/package/${package}/patches"
	local cflags="-O3 -flto${extra_include:+ -I${extra_include}}"
	local cxxflags="-O3 -flto${extra_include:+ -I${extra_include}}"
	test "${makefile}" != "-" || makefile=
	test "${path}" != "-" || path=
	test "${extra}" != "-" || extra=
	if [ ! -f "${workdir}/output/${name}_libretro.so" ]; then
		"${MAKE_CMD}" -f "${SP_EXTERNAL_DIR}/package/allium-support/libretro-core.mkfrag" \
			WORKDIR="${workdir}" PATCH_DIR="${patch_dir}" CORE_NAME="${name}" \
			CORE_REPO="${repo}" CORE_MAKEFILE="${makefile}" CORE_BUILD_PATH="${path}" \
				CORE_OUTPUT_NAME="${output}" CORE_PLATFORM="${platform}" CORE_EXTRA_VARS="${extra}" \
				CC="${CROSS_PREFIX}gcc" CXX="${CROSS_PREFIX}g++" AR="${CROSS_PREFIX}gcc-ar" \
				RANLIB="${CROSS_PREFIX}gcc-ranlib" CROSS_COMPILE="${CROSS_PREFIX}" \
				CORE_CFLAGS="${cflags}" \
				CORE_CXXFLAGS="${cxxflags}" \
				CORE_LDFLAGS="-fuse-ld=mold -flto" build
	else
		echo "Reusing ${name}_libretro.so"
	fi
	install -D -m 0755 "${workdir}/output/${name}_libretro.so" \
		"${PAYLOAD_DIR}/RetroArch/.retroarch/cores/${install_name}_libretro.so"
	copy_core_info "${install_name}"
}

build_cores() {
	build_core fake-08 https://github.com/jtothebell/fake-08 - fake-08/platform/libretro fake08_libretro.so unix - fake08 libretro-fake08 "${RETROARCH_DIR}/build/libretro-common/include"
	build_core fceumm https://github.com/libretro/libretro-fceumm - - fceumm_libretro.so unix WANT_32BPP=0 fceumm libretro-fceumm
	build_core gambatte https://github.com/libretro/gambatte-libretro Makefile.libretro - gambatte_libretro.so unix - gambatte libretro-gambatte
	build_core gpsp https://github.com/libretro/gpsp - - gpsp_libretro.so arm64 - gpsp libretro-gpsp
	build_core mednafen_pce_fast https://github.com/libretro/beetle-pce-fast-libretro - - mednafen_pce_fast_libretro.so unix - mednafen_pce_fast libretro-mednafen-pce-fast
	build_core mednafen_supafaust https://github.com/libretro/supafaust - - mednafen_supafaust_libretro.so unix - mednafen_supafaust libretro-mednafen-supafaust
	build_core mednafen_vb https://github.com/libretro/beetle-vb-libretro - - mednafen_vb_libretro.so unix - mednafen_vb libretro-mednafen-vb
	build_core mgba https://github.com/libretro/mgba - - mgba_libretro.so unix - mgba libretro-mgba
	build_core pcsx_rearmed https://github.com/libretro/pcsx_rearmed Makefile.libretro - pcsx_rearmed_libretro.so h5 - pcsx_rearmed libretro-pcsx-rearmed
	build_core picodrive https://github.com/irixxxx/picodrive Makefile.libretro - picodrive_libretro.so aarch64 - picodrive libretro-picodrive
	build_core pokemini https://github.com/libretro/PokeMini - - pokemini_libretro.so unix - pokemini libretro-pokemini
	build_core race https://github.com/libretro/race - - race_libretro.so unix - race libretro-race
	build_core snes9x2005_plus https://github.com/libretro/snes9x2005 - - snes9x2005_plus_libretro.so unix USE_BLARGG_APU=1 snes9x2005_plus libretro-snes9x2005-plus
}

prepare_payload
write_version
install_board_payload
build_allium
build_retroarch
build_cores
