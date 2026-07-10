#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd "$SCRIPT_DIR/.." && pwd)
PACKAGER=$REPO_ROOT/scripts/release/package_archive.sh
VERSION=1.2.3-test
TMP_ROOT=
FOREIGN_CWD=

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

assert_equal() {
    expected=$1
    actual=$2
    context=$3

    if [ "$actual" != "$expected" ]; then
        printf 'Expected: %s\n' "$expected" >&2
        printf 'Actual:   %s\n' "$actual" >&2
        fail "$context"
    fi
}

assert_same_file() {
    expected_file=$1
    actual_file=$2

    if ! cmp "$expected_file" "$actual_file" >/dev/null 2>&1; then
        fail "$actual_file does not match $expected_file"
    fi
}

assert_mode_0755() {
    mode_path=$1

    if mode=$(stat -f '%Lp' "$mode_path" 2>/dev/null); then
        :
    elif mode=$(stat -c '%a' "$mode_path" 2>/dev/null); then
        :
    else
        fail "could not inspect mode for $mode_path"
    fi

    assert_equal '755' "$mode" "$mode_path must have mode 0755"
}

assert_archive_members() {
    members_archive=$1
    shift
    expected_members=$TMP_ROOT/expected-members.txt
    actual_members=$TMP_ROOT/actual-members.txt

    printf '%s\n' "$@" | LC_ALL=C sort > "$expected_members"
    tar -tzf "$members_archive" | LC_ALL=C sort > "$actual_members"

    if ! cmp "$expected_members" "$actual_members" >/dev/null 2>&1; then
        printf 'Expected archive members:\n' >&2
        sed 's/^/  /' "$expected_members" >&2
        printf 'Actual archive members:\n' >&2
        sed 's/^/  /' "$actual_members" >&2
        fail "$members_archive has an unexpected member list"
    fi
}

assert_only_archive_output() {
    output_dir=$1
    archive_name=$2
    output_listing=$(LC_ALL=C ls -1A "$output_dir")

    assert_equal "$archive_name" "$output_listing" \
        "$output_dir must contain only $archive_name"
}

assert_directory_empty() {
    empty_dir=$1

    if [ -d "$empty_dir" ] && [ -n "$(LC_ALL=C ls -1A "$empty_dir")" ]; then
        fail "$empty_dir must be empty after a packaging failure"
    fi
}

make_unix_build() {
    fixture_dir=$1

    mkdir -p "$fixture_dir/icons/hicolor/48x48/apps" "$fixture_dir/private"
    printf 'fake cli for %s\n' "$fixture_dir" > "$fixture_dir/wakezilla"
    printf 'fake tray for %s\n' "$fixture_dir" > "$fixture_dir/wakezilla-tray"
    chmod 0600 "$fixture_dir/wakezilla" "$fixture_dir/wakezilla-tray"
    printf 'must not be packaged\n' > "$fixture_dir/sentinel-not-for-release.txt"
    printf 'must not be packaged either\n' > "$fixture_dir/private/secret.txt"
    printf 'wrong build-local icon\n' \
        > "$fixture_dir/icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png"
    printf 'wrong build-local macOS icon\n' > "$fixture_dir/Wakezilla.icns"
}

make_windows_build() {
    fixture_dir=$1

    mkdir -p "$fixture_dir/private"
    printf 'fake Windows cli for %s\n' "$fixture_dir" > "$fixture_dir/wakezilla.exe"
    printf 'fake Windows tray for %s\n' "$fixture_dir" > "$fixture_dir/wakezilla-tray.exe"
    printf 'must not be packaged\n' > "$fixture_dir/sentinel-not-for-release.txt"
    printf 'must not be packaged either\n' > "$fixture_dir/private/secret.txt"
    printf 'wrong build-local Windows icon\n' > "$fixture_dir/wakezilla.ico"
    printf 'wrong build-local uninstaller\n' > "$fixture_dir/uninstall-wakezilla.ps1"
}

run_linux_case() (
    target=$1
    label=$2
    build_dir=$TMP_ROOT/$label\ build\ fixtures
    output_dir=$TMP_ROOT/$label\ archive\ output
    extract_dir=$TMP_ROOT/$label\ extracted\ archive
    archive_name=wakezilla-$VERSION-$target.tar.gz
    archive_path=$output_dir/$archive_name

    make_unix_build "$build_dir"
    mkdir -p "$output_dir" "$extract_dir"

    (
        cd "$FOREIGN_CWD"
        sh "$PACKAGER" "$VERSION" "$target" "$build_dir" "$output_dir"
    )

    [ -f "$archive_path" ] || fail "expected archive was not created: $archive_path"
    assert_only_archive_output "$output_dir" "$archive_name"
    assert_archive_members "$archive_path" \
        wakezilla \
        wakezilla-tray \
        icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png \
        icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png \
        icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png

    tar -xzf "$archive_path" -C "$extract_dir"
    assert_same_file "$build_dir/wakezilla" "$extract_dir/wakezilla"
    assert_same_file "$build_dir/wakezilla-tray" "$extract_dir/wakezilla-tray"
    assert_mode_0755 "$extract_dir/wakezilla"
    assert_mode_0755 "$extract_dir/wakezilla-tray"
    assert_same_file \
        "$REPO_ROOT/assets/desktop/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png" \
        "$extract_dir/icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png"
    assert_same_file \
        "$REPO_ROOT/assets/desktop/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png" \
        "$extract_dir/icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png"
    assert_same_file \
        "$REPO_ROOT/assets/desktop/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png" \
        "$extract_dir/icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png"
    [ ! -f "$extract_dir/sentinel-not-for-release.txt" ] || \
        fail "build sentinel leaked into $archive_path"
    [ ! -f "$extract_dir/private/secret.txt" ] || \
        fail "nested build sentinel leaked into $archive_path"

    printf 'ok - %s\n' "$target"
)

run_macos_case() (
    target=$1
    label=$2
    build_dir=$TMP_ROOT/$label\ build\ fixtures
    output_dir=$TMP_ROOT/$label\ archive\ output
    extract_dir=$TMP_ROOT/$label\ extracted\ archive
    archive_name=wakezilla-$VERSION-$target.tar.gz
    archive_path=$output_dir/$archive_name

    make_unix_build "$build_dir"
    mkdir -p "$output_dir" "$extract_dir"

    (
        cd "$FOREIGN_CWD"
        sh "$PACKAGER" "$VERSION" "$target" "$build_dir" "$output_dir"
    )

    [ -f "$archive_path" ] || fail "expected archive was not created: $archive_path"
    assert_only_archive_output "$output_dir" "$archive_name"
    assert_archive_members "$archive_path" wakezilla wakezilla-tray Wakezilla.icns

    tar -xzf "$archive_path" -C "$extract_dir"
    assert_same_file "$build_dir/wakezilla" "$extract_dir/wakezilla"
    assert_same_file "$build_dir/wakezilla-tray" "$extract_dir/wakezilla-tray"
    assert_mode_0755 "$extract_dir/wakezilla"
    assert_mode_0755 "$extract_dir/wakezilla-tray"
    assert_same_file "$REPO_ROOT/assets/desktop/Wakezilla.icns" \
        "$extract_dir/Wakezilla.icns"
    [ ! -f "$extract_dir/sentinel-not-for-release.txt" ] || \
        fail "build sentinel leaked into $archive_path"
    [ ! -f "$extract_dir/private/secret.txt" ] || \
        fail "nested build sentinel leaked into $archive_path"

    printf 'ok - %s\n' "$target"
)

run_windows_case() (
    target=x86_64-pc-windows-msvc
    label=windows\ x86_64
    build_dir=$TMP_ROOT/$label\ build\ fixtures
    output_dir=$TMP_ROOT/$label\ archive\ output
    extract_dir=$TMP_ROOT/$label\ extracted\ archive
    archive_name=wakezilla-$VERSION-$target.tar.gz
    archive_path=$output_dir/$archive_name

    make_windows_build "$build_dir"
    mkdir -p "$output_dir" "$extract_dir"

    (
        cd "$FOREIGN_CWD"
        sh "$PACKAGER" "$VERSION" "$target" "$build_dir" "$output_dir"
    )

    [ -f "$archive_path" ] || fail "expected archive was not created: $archive_path"
    assert_only_archive_output "$output_dir" "$archive_name"
    assert_archive_members "$archive_path" \
        wakezilla.exe \
        wakezilla-tray.exe \
        wakezilla.ico \
        uninstall-wakezilla.ps1

    tar -xzf "$archive_path" -C "$extract_dir"
    assert_same_file "$build_dir/wakezilla.exe" "$extract_dir/wakezilla.exe"
    assert_same_file "$build_dir/wakezilla-tray.exe" "$extract_dir/wakezilla-tray.exe"
    assert_same_file "$REPO_ROOT/assets/desktop/wakezilla.ico" \
        "$extract_dir/wakezilla.ico"
    assert_same_file "$REPO_ROOT/scripts/windows/uninstall-wakezilla.ps1" \
        "$extract_dir/uninstall-wakezilla.ps1"
    [ ! -f "$extract_dir/sentinel-not-for-release.txt" ] || \
        fail "build sentinel leaked into $archive_path"
    [ ! -f "$extract_dir/private/secret.txt" ] || \
        fail "nested build sentinel leaked into $archive_path"

    printf 'ok - %s\n' "$target"
)

cleanup() {
    if [ -n "$TMP_ROOT" ] && [ -d "$TMP_ROOT" ]; then
        rm -rf "$TMP_ROOT"
    fi
}

TMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/wakezilla release package tests.XXXXXX") || \
    fail 'could not create temporary test directory'
FOREIGN_CWD=$TMP_ROOT/unrelated\ working\ directory
mkdir -p "$FOREIGN_CWD"
trap cleanup 0
trap 'exit 1' HUP INT TERM

[ -f "$PACKAGER" ] || fail "packager not found: $PACKAGER"

run_linux_case x86_64-unknown-linux-gnu linux\ gnu\ x86_64
run_linux_case aarch64-unknown-linux-gnu linux\ gnu\ aarch64
run_linux_case x86_64-unknown-linux-musl linux\ musl\ x86_64
run_linux_case aarch64-unknown-linux-musl linux\ musl\ aarch64
run_macos_case x86_64-apple-darwin macos\ x86_64
run_macos_case aarch64-apple-darwin macos\ aarch64
run_windows_case

unsupported_build=$TMP_ROOT/unsupported\ target\ build
unsupported_output=$TMP_ROOT/unsupported\ target\ output
mkdir -p "$unsupported_build" "$unsupported_output"
if unsupported_message=$(
    sh "$PACKAGER" "$VERSION" riscv64-unknown-none \
        "$unsupported_build" "$unsupported_output" 2>&1
); then
    fail 'unsupported target unexpectedly succeeded'
fi
case $unsupported_message in
    *'unsupported target:'*'riscv64-unknown-none'*) ;;
    *) fail "unsupported target error was not clear: $unsupported_message" ;;
esac
assert_directory_empty "$unsupported_output"
printf 'ok - unsupported target is rejected\n'

missing_build=$TMP_ROOT/missing\ artifact\ build
missing_output=$TMP_ROOT/missing\ artifact\ output
make_unix_build "$missing_build"
rm "$missing_build/wakezilla-tray"
mkdir -p "$missing_output"
if missing_message=$(
    sh "$PACKAGER" "$VERSION" x86_64-unknown-linux-gnu \
        "$missing_build" "$missing_output" 2>&1
); then
    fail 'missing build artifact unexpectedly succeeded'
fi
case $missing_message in
    *'missing required artifact:'*'wakezilla-tray'*) ;;
    *) fail "missing artifact error was not clear: $missing_message" ;;
esac
assert_directory_empty "$missing_output"
printf 'ok - missing artifacts are rejected\n'

printf 'All release package tests passed.\n'
