#!/bin/sh

# Build a canonical desktop release archive from already-built artifacts.
#
# Usage:
#   package_archive.sh VERSION TARGET BUILD_DIR OUTPUT_DIR
#
# BUILD_DIR is the target's release directory (for example,
# target/x86_64-unknown-linux-gnu/release). The script copies only the
# target-specific allowlist below and prints the completed archive path.

set -eu

LC_ALL=C
COPYFILE_DISABLE=1
export LC_ALL COPYFILE_DISABLE
umask 022

usage() {
    printf 'Usage: %s VERSION TARGET BUILD_DIR OUTPUT_DIR\n' "$0"
}

die() {
    printf 'package_archive.sh: %s\n' "$*" >&2
    exit 1
}

require_file() {
    required_file=$1

    if [ ! -f "$required_file" ] || [ ! -s "$required_file" ]; then
        die "missing required artifact: $required_file"
    fi
}

if [ "$#" -ne 4 ]; then
    usage >&2
    exit 2
fi

VERSION=$1
TARGET=$2
BUILD_DIR=$3
OUTPUT_DIR=$4

case $VERSION in
    ''|*[!A-Za-z0-9._+-]*)
        die "invalid version: $VERSION"
        ;;
esac

case $TARGET in
    x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|\
    x86_64-unknown-linux-musl|aarch64-unknown-linux-musl)
        PLATFORM=linux
        ;;
    x86_64-apple-darwin|aarch64-apple-darwin)
        PLATFORM=macos
        ;;
    x86_64-pc-windows-msvc)
        PLATFORM=windows
        ;;
    *)
        die "unsupported target: $TARGET"
        ;;
esac

[ -n "$BUILD_DIR" ] || die 'build directory must not be empty'
[ -n "$OUTPUT_DIR" ] || die 'output directory must not be empty'
[ -d "$BUILD_DIR" ] || die "build directory does not exist: $BUILD_DIR"

BUILD_DIR=$(CDPATH= cd "$BUILD_DIR" && pwd)
SCRIPT_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd "$SCRIPT_DIR/../.." && pwd)

ICON_48=$REPO_ROOT/assets/desktop/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png
ICON_128=$REPO_ROOT/assets/desktop/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png
ICON_256=$REPO_ROOT/assets/desktop/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png
MACOS_ICON=$REPO_ROOT/assets/desktop/Wakezilla.icns
WINDOWS_ICON=$REPO_ROOT/assets/desktop/wakezilla.ico
WINDOWS_UNINSTALLER=$REPO_ROOT/scripts/windows/uninstall-wakezilla.ps1

case $PLATFORM in
    linux)
        require_file "$BUILD_DIR/wakezilla"
        require_file "$BUILD_DIR/wakezilla-tray"
        require_file "$ICON_48"
        require_file "$ICON_128"
        require_file "$ICON_256"
        ;;
    macos)
        require_file "$BUILD_DIR/wakezilla"
        require_file "$BUILD_DIR/wakezilla-tray"
        require_file "$MACOS_ICON"
        ;;
    windows)
        require_file "$BUILD_DIR/wakezilla.exe"
        require_file "$BUILD_DIR/wakezilla-tray.exe"
        require_file "$WINDOWS_ICON"
        require_file "$WINDOWS_UNINSTALLER"
        ;;
esac

mkdir -p "$OUTPUT_DIR" || die "could not create output directory: $OUTPUT_DIR"
OUTPUT_DIR=$(CDPATH= cd "$OUTPUT_DIR" && pwd)
ARCHIVE_NAME=wakezilla-$VERSION-$TARGET.tar.gz
FINAL_ARCHIVE=$OUTPUT_DIR/$ARCHIVE_NAME
[ ! -d "$FINAL_ARCHIVE" ] || \
    die "archive destination is a directory: $FINAL_ARCHIVE"
TEMP_DIR=

cleanup() {
    if [ -n "$TEMP_DIR" ] && [ -d "$TEMP_DIR" ]; then
        rm -rf "$TEMP_DIR"
    fi
}

trap cleanup 0
trap 'exit 1' HUP INT TERM

TEMP_DIR=$(mktemp -d "$OUTPUT_DIR/.wakezilla-package.XXXXXX") || \
    die "could not create temporary package directory in: $OUTPUT_DIR"
STAGE_DIR=$TEMP_DIR/stage
TEMP_ARCHIVE=$TEMP_DIR/$ARCHIVE_NAME
mkdir -p "$STAGE_DIR"

case $PLATFORM in
    linux)
        mkdir -p \
            "$STAGE_DIR/icons/hicolor/48x48/apps" \
            "$STAGE_DIR/icons/hicolor/128x128/apps" \
            "$STAGE_DIR/icons/hicolor/256x256/apps"
        cp "$BUILD_DIR/wakezilla" "$STAGE_DIR/wakezilla"
        cp "$BUILD_DIR/wakezilla-tray" "$STAGE_DIR/wakezilla-tray"
        cp "$ICON_48" \
            "$STAGE_DIR/icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png"
        cp "$ICON_128" \
            "$STAGE_DIR/icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png"
        cp "$ICON_256" \
            "$STAGE_DIR/icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png"
        chmod 0755 "$STAGE_DIR/wakezilla" "$STAGE_DIR/wakezilla-tray"
        chmod 0644 \
            "$STAGE_DIR/icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png" \
            "$STAGE_DIR/icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png" \
            "$STAGE_DIR/icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png"
        tar -czf "$TEMP_ARCHIVE" -C "$STAGE_DIR" \
            wakezilla \
            wakezilla-tray \
            icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png \
            icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png \
            icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png
        ;;
    macos)
        cp "$BUILD_DIR/wakezilla" "$STAGE_DIR/wakezilla"
        cp "$BUILD_DIR/wakezilla-tray" "$STAGE_DIR/wakezilla-tray"
        cp "$MACOS_ICON" "$STAGE_DIR/Wakezilla.icns"
        chmod 0755 "$STAGE_DIR/wakezilla" "$STAGE_DIR/wakezilla-tray"
        chmod 0644 "$STAGE_DIR/Wakezilla.icns"
        tar -czf "$TEMP_ARCHIVE" -C "$STAGE_DIR" \
            wakezilla wakezilla-tray Wakezilla.icns
        ;;
    windows)
        cp "$BUILD_DIR/wakezilla.exe" "$STAGE_DIR/wakezilla.exe"
        cp "$BUILD_DIR/wakezilla-tray.exe" "$STAGE_DIR/wakezilla-tray.exe"
        cp "$WINDOWS_ICON" "$STAGE_DIR/wakezilla.ico"
        cp "$WINDOWS_UNINSTALLER" "$STAGE_DIR/uninstall-wakezilla.ps1"
        chmod 0755 "$STAGE_DIR/wakezilla.exe" "$STAGE_DIR/wakezilla-tray.exe"
        chmod 0644 "$STAGE_DIR/wakezilla.ico" \
            "$STAGE_DIR/uninstall-wakezilla.ps1"
        tar -czf "$TEMP_ARCHIVE" -C "$STAGE_DIR" \
            wakezilla.exe \
            wakezilla-tray.exe \
            wakezilla.ico \
            uninstall-wakezilla.ps1
        ;;
esac

mv "$TEMP_ARCHIVE" "$FINAL_ARCHIVE"
printf '%s\n' "$FINAL_ARCHIVE"
