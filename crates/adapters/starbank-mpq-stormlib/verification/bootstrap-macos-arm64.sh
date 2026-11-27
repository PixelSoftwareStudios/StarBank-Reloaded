#!/bin/sh
set -eu
export LC_ALL=C

source_sha256=f80b08bae168888702a4251312d6c2279f487673df611c2b5ef4f395e810c3b1
target=aarch64-apple-darwin
stamp_version=starbank-stormlib-bootstrap-v1

if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
	printf '%s\n' 'Usage: bootstrap-macos-arm64.sh /path/to/StormLib-v9.40.tar.gz' >&2
	exit 2
fi
if [ "$(uname -s)/$(uname -m)" != 'Darwin/arm64' ]; then
	printf '%s\n' 'The qualified bootstrap requires native arm64 macOS.' >&2
	exit 1
fi
if [ "$(cmake --version | head -n 1)" != 'cmake version 4.4.4' ] ||
	[ "$(xcrun --sdk macosx --show-sdk-version)" != '15.2' ] ||
	[ "$(xcodebuild -version)" != "$(printf 'Xcode 16.2\nBuild version 16C5032a')" ]; then
	printf '%s\n' 'The qualified bootstrap requires CMake 4.4.4 and Xcode 16.2 (SDK 15.2).' >&2
	exit 1
fi

umask 077
stage=$(mktemp -d /private/tmp/starbank-stormlib-build.XXXXXXXX)
cleanup() {
	rm -rf -- "$stage"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

cp -- "$1" "$stage/StormLib-v9.40.tar.gz"
actual_sha256=$(shasum -a 256 <"$stage/StormLib-v9.40.tar.gz")
if [ "${actual_sha256%% *}" != "$source_sha256" ]; then
	printf '%s\n' 'StormLib v9.40 source checksum mismatch.' >&2
	exit 1
fi
tar -xzf "$stage/StormLib-v9.40.tar.gz" -C "$stage"
env -u CC -u CXX -u CFLAGS -u CXXFLAGS -u CPPFLAGS -u LDFLAGS -u CMAKE_PREFIX_PATH \
	cmake -S "$stage/StormLib-9.40" -B "$stage/build" -G 'Unix Makefiles' \
		-DCMAKE_BUILD_TYPE=Release \
		-DCMAKE_C_COMPILER="$(xcrun --sdk macosx --find clang)" \
		-DCMAKE_CXX_COMPILER="$(xcrun --sdk macosx --find clang++)" \
		-DCMAKE_MAKE_PROGRAM=/usr/bin/make \
		-DCMAKE_C_FLAGS="-ffile-prefix-map=$stage=/starbank-stormlib" \
		-DCMAKE_CXX_FLAGS="-ffile-prefix-map=$stage=/starbank-stormlib" \
		-DCMAKE_OSX_SYSROOT="$(xcrun --sdk macosx --show-sdk-path)" \
		-DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 \
		-DBUILD_SHARED_LIBS=OFF -DSTORM_SKIP_INSTALL=ON -DSTORM_BUILD_TESTS=OFF \
		-DSTORM_USE_BUNDLED_LIBRARIES=ON -DWITH_LIBTOMCRYPT=OFF \
		-DCMAKE_DISABLE_FIND_PACKAGE_ZLIB=ON -DCMAKE_DISABLE_FIND_PACKAGE_BZip2=ON >&2
ZERO_AR_DATE=1 cmake --build "$stage/build" --target storm --parallel 2 >&2

archive="$stage/build/libstorm.a"
if [ "$(xcrun lipo -archs "$archive")" != arm64 ]; then
	printf '%s\n' 'StormLib archive is not exclusively arm64.' >&2
	exit 1
fi
mkdir "$stage/artifact"
cp "$archive" "$stage/artifact/libstorm.a"
archive_sha256=$(shasum -a 256 <"$stage/artifact/libstorm.a")
printf '%s\n' "$stamp_version" "source=$source_sha256" "target=$target" \
	"archive=${archive_sha256%% *}" >"$stage/artifact/provenance"
rm -rf -- "$stage/StormLib-9.40" "$stage/StormLib-v9.40.tar.gz" "$stage/build"
trap - EXIT
printf '%s\n' "$stage/artifact"
