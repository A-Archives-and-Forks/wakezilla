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

assert_regular_files() {
    for regular_path in "$@"; do
        if [ ! -f "$regular_path" ] || [ -L "$regular_path" ]; then
            fail "archive member must be a regular file: $regular_path"
        fi
    done
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

assert_directory_members() {
    members_dir=$1
    shift
    expected_directory_members=$TMP_ROOT/expected-directory-members.txt
    actual_directory_members=$TMP_ROOT/actual-directory-members.txt

    printf '%s\n' "$@" | LC_ALL=C sort > "$expected_directory_members"
    LC_ALL=C ls -1A "$members_dir" | LC_ALL=C sort > "$actual_directory_members"

    if ! cmp "$expected_directory_members" "$actual_directory_members" \
        >/dev/null 2>&1; then
        printf 'Expected directory members:\n' >&2
        sed 's/^/  /' "$expected_directory_members" >&2
        printf 'Actual directory members:\n' >&2
        sed 's/^/  /' "$actual_directory_members" >&2
        fail "$members_dir has an unexpected member list"
    fi
}

assert_directory_empty() {
    empty_dir=$1

    if [ -d "$empty_dir" ] && [ -n "$(LC_ALL=C ls -1A "$empty_dir")" ]; then
        fail "$empty_dir must be empty after a packaging failure"
    fi
}

assert_no_package_temps() {
    temp_output_dir=$1

    for temp_path in "$temp_output_dir"/.wakezilla-package.*; do
        if [ -d "$temp_path" ] || [ -f "$temp_path" ]; then
            fail "temporary package path remains after failure: $temp_path"
        fi
    done
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
    assert_regular_files \
        "$extract_dir/wakezilla" \
        "$extract_dir/wakezilla-tray" \
        "$extract_dir/icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png" \
        "$extract_dir/icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png" \
        "$extract_dir/icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png"
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
    assert_regular_files \
        "$extract_dir/wakezilla" \
        "$extract_dir/wakezilla-tray" \
        "$extract_dir/Wakezilla.icns"
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
    assert_regular_files \
        "$extract_dir/wakezilla.exe" \
        "$extract_dir/wakezilla-tray.exe" \
        "$extract_dir/wakezilla.ico" \
        "$extract_dir/uninstall-wakezilla.ps1"
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

directory_build=$TMP_ROOT/archive\ destination\ build
directory_output=$TMP_ROOT/archive\ destination\ output
directory_target=x86_64-unknown-linux-gnu
directory_archive_name=wakezilla-$VERSION-$directory_target.tar.gz
make_unix_build "$directory_build"
mkdir -p "$directory_output"
directory_output=$(CDPATH= cd "$directory_output" && pwd)
directory_archive=$directory_output/$directory_archive_name
directory_sentinel=$directory_archive/preexisting\ sentinel.txt
mkdir "$directory_archive"
printf 'preserve this directory\n' > "$directory_sentinel"
if directory_message=$(
    cd "$FOREIGN_CWD"
    sh "$PACKAGER" "$VERSION" "$directory_target" \
        "$directory_build" "$directory_output" 2>&1
); then
    fail 'archive destination directory unexpectedly succeeded'
fi
case $directory_message in
    *'archive destination is a directory:'*"$directory_archive"*) ;;
    *) fail "archive destination error was not clear: $directory_message" ;;
esac
[ -d "$directory_archive" ] || fail 'preexisting archive directory was removed'
assert_equal 'preserve this directory' "$(sed -n '1p' "$directory_sentinel")" \
    'preexisting archive directory sentinel was changed'
[ ! -f "$directory_archive/$directory_archive_name" ] || \
    fail 'archive was nested inside the preexisting destination directory'
assert_no_package_temps "$directory_output"
assert_only_archive_output "$directory_output" "$directory_archive_name"
printf 'ok - archive destination directories are rejected\n'

symlink_build=$TMP_ROOT/symlink\ artifact\ build
symlink_output=$TMP_ROOT/symlink\ artifact\ output
symlink_external_dir=$TMP_ROOT/symlink\ external\ source
symlink_target=x86_64-unknown-linux-gnu
symlink_archive_name=wakezilla-$VERSION-$symlink_target.tar.gz
symlink_expected_archive=$TMP_ROOT/symlink\ expected\ archive
symlink_expected_sentinel=$TMP_ROOT/symlink\ expected\ sentinel
make_unix_build "$symlink_build"
mkdir -p "$symlink_output" "$symlink_external_dir"
symlink_build=$(CDPATH= cd "$symlink_build" && pwd)
symlink_output=$(CDPATH= cd "$symlink_output" && pwd)
symlink_archive=$symlink_output/$symlink_archive_name
symlink_sentinel=$symlink_output/unrelated\ sentinel.txt
printf 'external executable bytes\n' > "$symlink_external_dir/wakezilla"
rm "$symlink_build/wakezilla"
ln -s "$symlink_external_dir/wakezilla" "$symlink_build/wakezilla"
printf 'previous release archive\n' > "$symlink_expected_archive"
printf 'preserve unrelated output\n' > "$symlink_expected_sentinel"
cp "$symlink_expected_archive" "$symlink_archive"
cp "$symlink_expected_sentinel" "$symlink_sentinel"
if symlink_message=$(
    cd "$FOREIGN_CWD"
    sh "$PACKAGER" "$VERSION" "$symlink_target" \
        "$symlink_build" "$symlink_output" 2>&1
); then
    fail 'symbolic-link build artifact unexpectedly succeeded'
fi
case $symlink_message in
    *'required artifact must not be a symbolic link:'*"$symlink_build/wakezilla"*) ;;
    *) fail "symbolic-link artifact error was not clear: $symlink_message" ;;
esac
assert_same_file "$symlink_expected_archive" "$symlink_archive"
assert_same_file "$symlink_expected_sentinel" "$symlink_sentinel"
assert_no_package_temps "$symlink_output"
assert_directory_members "$symlink_output" \
    "$symlink_archive_name" \
    'unrelated sentinel.txt'
printf 'ok - symbolic-link artifacts are rejected\n'

replace_build=$TMP_ROOT/replace\ existing\ build
replace_output=$TMP_ROOT/replace\ existing\ output
replace_target=x86_64-apple-darwin
replace_archive_name=wakezilla-$VERSION-$replace_target.tar.gz
replace_archive=$replace_output/$replace_archive_name
replace_sentinel=$replace_output/unrelated\ sentinel.txt
replace_expected_old=$TMP_ROOT/replace\ expected\ old\ archive
replace_expected_sentinel=$TMP_ROOT/replace\ expected\ sentinel
make_unix_build "$replace_build"
mkdir -p "$replace_output"
printf 'previous release archive\n' > "$replace_expected_old"
printf 'preserve unrelated output\n' > "$replace_expected_sentinel"
cp "$replace_expected_old" "$replace_archive"
cp "$replace_expected_sentinel" "$replace_sentinel"
(
    cd "$FOREIGN_CWD"
    sh "$PACKAGER" "$VERSION" "$replace_target" \
        "$replace_build" "$replace_output"
)
if cmp "$replace_expected_old" "$replace_archive" >/dev/null 2>&1; then
    fail 'successful packaging did not replace the previous archive'
fi
assert_archive_members "$replace_archive" wakezilla wakezilla-tray Wakezilla.icns
assert_same_file "$replace_expected_sentinel" "$replace_sentinel"
assert_no_package_temps "$replace_output"
assert_directory_members "$replace_output" \
    "$replace_archive_name" \
    'unrelated sentinel.txt'
printf 'ok - existing archives are atomically replaced\n'

tar_failure_build=$TMP_ROOT/tar\ failure\ build
tar_failure_output=$TMP_ROOT/tar\ failure\ output
tar_failure_bin=$TMP_ROOT/tar\ failure\ tools
tar_failure_target=x86_64-apple-darwin
tar_failure_archive_name=wakezilla-$VERSION-$tar_failure_target.tar.gz
tar_failure_archive=$tar_failure_output/$tar_failure_archive_name
tar_failure_sentinel=$tar_failure_output/unrelated\ sentinel.txt
tar_failure_expected_archive=$TMP_ROOT/tar\ failure\ expected\ archive
tar_failure_expected_sentinel=$TMP_ROOT/tar\ failure\ expected\ sentinel
make_unix_build "$tar_failure_build"
mkdir -p "$tar_failure_output" "$tar_failure_bin"
printf 'previous release archive\n' > "$tar_failure_expected_archive"
printf 'preserve unrelated output\n' > "$tar_failure_expected_sentinel"
cp "$tar_failure_expected_archive" "$tar_failure_archive"
cp "$tar_failure_expected_sentinel" "$tar_failure_sentinel"
printf '%s\n' \
    '#!/bin/sh' \
    "printf '%s\\n' 'intentional fake tar failure' >&2" \
    'exit 42' \
    > "$tar_failure_bin/tar"
chmod 0755 "$tar_failure_bin/tar"
if tar_failure_message=$(
    cd "$FOREIGN_CWD"
    PATH="$tar_failure_bin:$PATH" sh "$PACKAGER" "$VERSION" \
        "$tar_failure_target" "$tar_failure_build" "$tar_failure_output" 2>&1
); then
    fail 'injected tar failure unexpectedly succeeded'
fi
case $tar_failure_message in
    *'intentional fake tar failure'*) ;;
    *) fail "injected tar error was not clear: $tar_failure_message" ;;
esac
assert_same_file "$tar_failure_expected_archive" "$tar_failure_archive"
assert_same_file "$tar_failure_expected_sentinel" "$tar_failure_sentinel"
assert_no_package_temps "$tar_failure_output"
assert_directory_members "$tar_failure_output" \
    "$tar_failure_archive_name" \
    'unrelated sentinel.txt'
printf 'ok - tar failures preserve existing output\n'

printf 'All release package tests passed.\n'
