#!/bin/sh

set -eu

# macOS build helper. Keep this separate from build.sh so the default script
# stays untouched.

if [ "${BUILD_MAC_GIT_PULL:-0}" = "1" ] && [ -d ".git" ]; then
	if ! git pull --recurse-submodules=yes; then
		git pull --no-recurse-submodules
	fi
fi

BUILD_MODE=Release
BUILD_ROOT="${BUILD_ROOT:-build-ninja-m1}"
QT6_DIR="${QT6_DIR:-/opt/homebrew/opt/qt/lib/cmake/Qt6}"
ENABLE_OGLRENDERER="${ENABLE_OGLRENDERER:-OFF}"
ENABLE_LTO_RELEASE="${ENABLE_LTO_RELEASE:-ON}"
MELONDS_SRC_DIR="$(pwd)/melonds-rs/melonDS/src"

if command -v ninja >/dev/null 2>&1; then
	CORES_GENERATOR="Ninja"
else
	CORES_GENERATOR="Unix Makefiles"
fi

jobs() {
	if command -v nproc >/dev/null 2>&1; then
		nproc
	elif command -v sysctl >/dev/null 2>&1; then
		sysctl -n hw.physicalcpu
	else
		echo 4
	fi
}

cmake_launcher_args() {
	if command -v ccache >/dev/null 2>&1; then
		printf '%s\n' \
			-DCMAKE_C_COMPILER_LAUNCHER=ccache \
			-DCMAKE_CXX_COMPILER_LAUNCHER=ccache
	fi
}

find_macdeployqt() {
	if [ -n "${MACDEPLOYQT:-}" ] && [ -x "${MACDEPLOYQT}" ]; then
		printf '%s\n' "${MACDEPLOYQT}"
		return 0
	fi

	if command -v macdeployqt >/dev/null 2>&1; then
		command -v macdeployqt
		return 0
	fi

	qt_prefix=$(cd "${QT6_DIR}/../../.." && pwd)
	if [ -x "${qt_prefix}/bin/macdeployqt" ]; then
		printf '%s\n' "${qt_prefix}/bin/macdeployqt"
		return 0
	fi

	return 1
}

bundle_sdl3() {
	app="$1"
	app_bin="${app}/Contents/MacOS/SuperShuckie"
	frameworks_dir="${app}/Contents/Frameworks"
	sdl_src=$(otool -L "${app_bin}" | awk '/libSDL3\.0\.dylib/{print $1; exit}')

	if [ -z "${sdl_src}" ]; then
		printf 'failed to locate linked SDL3 dylib in %s\n' "${app_bin}" >&2
		exit 1
	fi

	case "${sdl_src}" in
		@executable_path/*|@rpath/*)
			return 0
			;;
	esac

	mkdir -p "${frameworks_dir}"
	cp -f "${sdl_src}" "${frameworks_dir}/libSDL3.0.dylib"
	chmod u+w "${frameworks_dir}/libSDL3.0.dylib"
	install_name_tool -id "@rpath/libSDL3.0.dylib" "${frameworks_dir}/libSDL3.0.dylib"
	install_name_tool -change "${sdl_src}" "@executable_path/../Frameworks/libSDL3.0.dylib" "${app_bin}"
}

write_qt_conf() {
	app="$1"
	mkdir -p "${app}/Contents/Resources"
	cat > "${app}/Contents/Resources/qt.conf" <<'EOF'
[Paths]
Plugins = PlugIns
EOF
}

ad_hoc_sign_app() {
	app="$1"
	frameworks_dir="${app}/Contents/Frameworks"
	plugins_dir="${app}/Contents/PlugIns"
	app_bin="${app}/Contents/MacOS/SuperShuckie"

	if ! command -v codesign >/dev/null 2>&1; then
		return 0
	fi

	if [ -d "${frameworks_dir}" ]; then
		find "${frameworks_dir}" -maxdepth 1 -type d -name '*.framework' -print | while IFS= read -r framework; do
			codesign --force --sign - --timestamp=none "${framework}"
		done
		find "${frameworks_dir}" -maxdepth 1 -type f -name '*.dylib' -print | while IFS= read -r dylib; do
			codesign --force --sign - --timestamp=none "${dylib}"
		done
	fi

	if [ -d "${plugins_dir}" ]; then
		find "${plugins_dir}" -type f -name '*.dylib' -print | while IFS= read -r plugin; do
			codesign --force --sign - --timestamp=none "${plugin}"
		done
	fi

	codesign --force --sign - --timestamp=none "${app_bin}"
	codesign --force --sign - --timestamp=none "${app}"
}

verify_no_homebrew_refs() {
	app="$1"
	bad_refs=$(
		find "${app}/Contents" -type f -print | while IFS= read -r file; do
			if file "${file}" | grep -q 'Mach-O'; then
				otool -L "${file}" 2>/dev/null | grep '/opt/homebrew' || true
			fi
		done
	)

	if [ -n "${bad_refs}" ]; then
		printf 'unbundled Homebrew references remain:\n%s\n' "${bad_refs}" >&2
		exit 1
	fi
}

deploy_app_bundle() {
	app="$1"
	macdeployqt_bin=$(find_macdeployqt) || {
		printf 'macdeployqt not found; install Qt deployment tools or set MACDEPLOYQT\n' >&2
		exit 1
	}

	write_qt_conf "${app}"
	"${macdeployqt_bin}" "${app}" -always-overwrite
	bundle_sdl3 "${app}"
	ad_hoc_sign_app "${app}"
	verify_no_homebrew_refs "${app}"
}

if command -v ccache >/dev/null 2>&1; then
	export CCACHE_DIR="${CCACHE_DIR:-/tmp/ccache-supershuckie}"
	export CCACHE_TEMPDIR="${CCACHE_TEMPDIR:-$CCACHE_DIR/tmp}"
	mkdir -p "$CCACHE_DIR" "$CCACHE_TEMPDIR"
fi

export RUSTFLAGS="${RUSTFLAGS:--C target-cpu=apple-m1}"

CMAKE_C_FLAGS_RELEASE="-O3 -mcpu=apple-m1"
CMAKE_CXX_FLAGS_RELEASE="-O3 -mcpu=apple-m1"

cmake -S ./melonds-rs/melonDS -B "${BUILD_ROOT}/melonDS" -G "$CORES_GENERATOR" \
	-DENABLE_JIT=ON \
	-DENABLE_OGLRENDERER="$ENABLE_OGLRENDERER" \
	-DENABLE_GDBSTUB=OFF \
	-DBUILD_QT_SDL=OFF \
	-DENABLE_LTO_RELEASE="$ENABLE_LTO_RELEASE" \
	-DCMAKE_BUILD_TYPE="$BUILD_MODE" \
	-DCMAKE_OSX_ARCHITECTURES=arm64 \
	-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON \
	$(cmake_launcher_args) \
	"-DCMAKE_CXX_FLAGS=-I${MELONDS_SRC_DIR}" \
	"-DCMAKE_C_FLAGS_RELEASE=$CMAKE_C_FLAGS_RELEASE" \
	"-DCMAKE_CXX_FLAGS_RELEASE=$CMAKE_CXX_FLAGS_RELEASE"
cmake --build "${BUILD_ROOT}/melonDS" -j"$(jobs)"

cmake -S ./mgba-rs/mgba -B "${BUILD_ROOT}/mgba" -G "$CORES_GENERATOR" \
	-DLIBMGBA_ONLY=ON \
	-DDISABLE_FRONTENDS=ON \
	-DCMAKE_BUILD_TYPE="$BUILD_MODE" \
	-DCMAKE_OSX_ARCHITECTURES=arm64 \
	-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON \
	$(cmake_launcher_args) \
	"-DCMAKE_C_FLAGS_RELEASE=$CMAKE_C_FLAGS_RELEASE" \
	"-DCMAKE_CXX_FLAGS_RELEASE=$CMAKE_CXX_FLAGS_RELEASE"
cmake --build "${BUILD_ROOT}/mgba" -j"$(jobs)"

cmake -S ./supershuckie-qt -B "${BUILD_ROOT}" -G "$CORES_GENERATOR" \
	-DCMAKE_BUILD_TYPE="$BUILD_MODE" \
	-DSCRIPT_BUILD=ON \
	-DQt6_DIR="$QT6_DIR" \
	-DCMAKE_IGNORE_PATH=/usr/local \
	-DCMAKE_OSX_ARCHITECTURES=arm64 \
	-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON \
	$(cmake_launcher_args) \
	"-DCMAKE_C_FLAGS_RELEASE=$CMAKE_C_FLAGS_RELEASE" \
	"-DCMAKE_CXX_FLAGS_RELEASE=$CMAKE_CXX_FLAGS_RELEASE"
cmake --build "${BUILD_ROOT}" -j"$(jobs)"

deploy_app_bundle "${BUILD_ROOT}/SuperShuckie.app"

mkdir -p dist/macos
ditto "${BUILD_ROOT}/SuperShuckie.app" dist/macos/SuperShuckie.app
