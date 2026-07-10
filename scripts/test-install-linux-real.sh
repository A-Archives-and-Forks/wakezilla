#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

skip_or_fail() {
  reason="$1"
  if [ "${WAKEZILLA_CI_REQUIRED:-}" = 1 ]; then
    printf 'FAIL: real Linux installer integration prerequisite: %s\n' "$reason" >&2
    exit 1
  fi
  printf 'SKIP: real Linux installer integration: %s\n' "$reason"
  exit 0
}

[ "$(uname -s 2>/dev/null || printf unknown)" = Linux ] || \
  skip_or_fail "Linux is required"
[ "$(id -u)" = 0 ] || skip_or_fail "root is required to create a secondary user"

for required_command in \
  desktop-file-validate \
  env \
  getent \
  gio \
  runuser \
  stat \
  useradd \
  userdel; do
  command -v "$required_command" >/dev/null 2>&1 || \
    skip_or_fail "$required_command is unavailable"
done

temp_dir=$(mktemp -d)
chmod 0755 "$temp_dir"
test_user="wakezillaci$$"
created_user=no
cleanup() {
  if [ "$created_user" = yes ]; then
    userdel "$test_user" >/dev/null 2>&1 || true
  fi
  rm -rf "$temp_dir"
}
trap cleanup 0
trap 'exit 1' 1 2 15

while getent passwd "$test_user" >/dev/null 2>&1; do
  test_user="${test_user}x"
done
user_home="$temp_dir/home"
useradd --create-home --home-dir "$user_home" --shell /bin/sh "$test_user"
created_user=yes
test_uid=$(id -u "$test_user")
test_gid=$(id -g "$test_user")
[ "$test_uid" != 0 ] || {
  printf 'FAIL: secondary integration user unexpectedly has UID 0\n' >&2
  exit 1
}

extract_dir="$temp_dir/extract"
bin_dir="$temp_dir/system-bin"
launch_log="$user_home/gio-launch.log"
mkdir -p "$extract_dir/icons/hicolor/48x48/apps" \
  "$extract_dir/icons/hicolor/128x128/apps" \
  "$extract_dir/icons/hicolor/256x256/apps" \
  "$bin_dir"
cat > "$extract_dir/wakezilla-tray" <<EOF
#!/usr/bin/env sh
printf '%s|%s\n' "\$(id -u)" "\$#" > '$launch_log'
EOF
chmod 0755 "$extract_dir/wakezilla-tray"
cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
chmod 0755 "$bin_dir/wakezilla-tray"
for icon_size in 48 128 256; do
  printf 'real-linux-icon-%s\n' "$icon_size" > \
    "$extract_dir/icons/hicolor/${icon_size}x${icon_size}/apps/dev.wakezilla.Wakezilla.png"
done

export WAKEZILLA_INSTALL_SH_TEST_MODE=1
. "$ROOT_DIR/install.sh"

HOME=/root \
SUDO_USER="$test_user" \
SUDO_UID="$test_uid" \
SUDO_GID="$test_gid" \
XDG_DATA_HOME="$temp_dir/root-data-must-not-be-used" \
XDG_CONFIG_HOME="$temp_dir/root-config-must-not-be-used" \
DISPLAY= \
WAYLAND_DISPLAY= \
  install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null

data_home="$user_home/.local/share"
config_home="$user_home/.config"
application_entry="$data_home/applications/dev.wakezilla.Wakezilla.desktop"
autostart_entry="$config_home/autostart/dev.wakezilla.tray.desktop"

for installed_file in \
  "$application_entry" \
  "$autostart_entry" \
  "$data_home/icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png" \
  "$data_home/icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png" \
  "$data_home/icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png"; do
  [ -f "$installed_file" ] || {
    printf 'FAIL: expected installed profile file: %s\n' "$installed_file" >&2
    exit 1
  }
  [ ! -L "$installed_file" ] || {
    printf 'FAIL: installed profile file is a symlink: %s\n' "$installed_file" >&2
    exit 1
  }
  installed_uid=$(stat -c '%u' "$installed_file")
  [ "$installed_uid" = "$test_uid" ] || {
    printf 'FAIL: %s is owned by UID %s, expected %s\n' \
      "$installed_file" "$installed_uid" "$test_uid" >&2
    exit 1
  }
done

[ ! -e "$temp_dir/root-data-must-not-be-used" ] || {
  printf 'FAIL: installer wrote to root XDG data path\n' >&2
  exit 1
}
[ ! -e "$temp_dir/root-config-must-not-be-used" ] || {
  printf 'FAIL: installer wrote to root XDG config path\n' >&2
  exit 1
}

expected_exec="Exec=\"$bin_dir/wakezilla-tray\""
grep -Fqx "$expected_exec" "$application_entry" || {
  printf 'FAIL: application launcher does not directly execute the tray helper\n' >&2
  exit 1
}
grep -Fqx "$expected_exec" "$autostart_entry" || {
  printf 'FAIL: autostart launcher does not directly execute the tray helper\n' >&2
  exit 1
}
desktop-file-validate "$application_entry"
desktop-file-validate "$autostart_entry"

runuser -u "$test_user" -- \
  env -i \
  HOME="$user_home" \
  XDG_DATA_HOME="$data_home" \
  XDG_CONFIG_HOME="$config_home" \
  PATH=/usr/local/bin:/usr/bin:/bin \
  gio launch "$application_entry"

launch_wait=0
while [ ! -f "$launch_log" ] && [ "$launch_wait" -lt 100 ]; do
  sleep 0.05
  launch_wait=$((launch_wait + 1))
done
[ -f "$launch_log" ] || {
  printf 'FAIL: GLib launch did not execute the tray helper\n' >&2
  exit 1
}
[ "$(cat "$launch_log")" = "$test_uid|0" ] || {
  printf 'FAIL: GLib launch did not run as the secondary user with zero arguments\n' >&2
  exit 1
}

printf 'real Linux installer integration passed\n'
