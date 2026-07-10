#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
SCRIPT="$ROOT_DIR/install.sh"

failures=0

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  failures=$((failures + 1))
}

assert_contains() {
  haystack="$1"
  needle="$2"
  label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label: expected output to contain '$needle'" ;;
  esac
}

assert_not_contains() {
  haystack="$1"
  needle="$2"
  label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label: expected output not to contain '$needle'" ;;
    *) ;;
  esac
}

assert_eq() {
  expected="$1"
  actual="$2"
  label="$3"
  if [ "$expected" != "$actual" ]; then
    fail "$label: expected '$expected', got '$actual'"
  fi
}

assert_command_exists() {
  command_name="$1"
  label="$2"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    fail "$label: expected command '$command_name' to be defined"
    return 1
  fi
}

run_script() {
  output_file=$(mktemp)
  set +e
  "$SCRIPT" "$@" >"$output_file" 2>&1
  status=$?
  set -e
  output=$(cat "$output_file")
  rm -f "$output_file"
}

write_stub_command() {
  command_path="$1"
  printf '#!/usr/bin/env sh\nexit 0\n' > "$command_path"
  chmod +x "$command_path"
}

write_exec_wrapper() {
  command_path="$1"
  real_command="$2"
  printf '#!/bin/sh\nexec "%s" "$@"\n' "$real_command" > "$command_path"
  chmod +x "$command_path"
}

write_install_dependency_stubs() {
  bin_dir="$1"
  real_tar=$(command -v tar)
  real_sha256sum=$(command -v sha256sum 2>/dev/null || true)
  real_shasum=$(command -v shasum 2>/dev/null || true)
  mkdir -p "$bin_dir"
  write_stub_command "$bin_dir/curl"
  write_exec_wrapper "$bin_dir/tar" "$real_tar"
  cat > "$bin_dir/sha256sum" <<SH
#!/usr/bin/env sh
if [ -n "$real_sha256sum" ]; then
  exec "$real_sha256sum" "\$@"
fi
exec "$real_shasum" -a 256 "\$@"
SH
  chmod +x "$bin_dir/sha256sum"
}

write_fixture_curl() {
  command_path="$1"
  cat > "$command_path" <<'SH'
#!/usr/bin/env sh
output=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      shift
      output="$1"
      ;;
    -*)
      ;;
    *)
      url="$1"
      ;;
  esac
  shift
done

if [ -n "$output" ]; then
  case "$url" in
    */SHA256SUMS)
      archive_dir=$(dirname "$output")
      archive="$archive_dir/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"
      sha256sum "$archive" | awk '{print $1 "  wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"}' > "$output"
      ;;
    *)
      temp_dir=$(mktemp -d)
      mkdir -p "$temp_dir/archive"
      if [ "${WAKEZILLA_FAKE_HISTORICAL_VERSION_ONLY:-}" = "1" ]; then
        printf '#!/usr/bin/env sh\nif [ "$#" -eq 1 ] && [ "${1:-}" = "--version" ]; then\n  printf "wakezilla 0.1.49\\n"\n  exit 0\nfi\nexit 97\n' > "$temp_dir/archive/wakezilla"
      elif [ "${WAKEZILLA_FAKE_VERSION_MISMATCH:-}" = "1" ]; then
        printf '#!/usr/bin/env sh\nif [ "$#" -eq 1 ] && [ "${1:-}" = "--version" ]; then\n  printf "wakezilla 0.1.50\\n"\n  exit 0\nfi\nexit 97\n' > "$temp_dir/archive/wakezilla"
      elif [ "${WAKEZILLA_FAKE_VERSION_EXITS_NONZERO:-}" = "1" ]; then
        printf '#!/usr/bin/env sh\nif [ "$#" -eq 1 ] && [ "${1:-}" = "--version" ]; then\n  printf "wakezilla 0.1.49\\n"\n  exit 1\nfi\nexit 97\n' > "$temp_dir/archive/wakezilla"
      elif [ "${WAKEZILLA_FAKE_VERSION_EMPTY:-}" = "1" ]; then
        printf '#!/usr/bin/env sh\nif [ "$#" -eq 1 ] && [ "${1:-}" = "--version" ]; then\n  exit 0\nfi\nexit 97\n' > "$temp_dir/archive/wakezilla"
      else
        printf '#!/usr/bin/env sh\nif [ "$#" -eq 1 ] && [ "${1:-}" = "--version" ]; then\n  printf "wakezilla 0.1.49\\n"\n  exit 0\nfi\nexit 97\n' > "$temp_dir/archive/wakezilla"
      fi
      chmod +x "$temp_dir/archive/wakezilla"
      printf '#!/usr/bin/env sh\nexit 0\n' > "$temp_dir/archive/wakezilla-tray"
      chmod +x "$temp_dir/archive/wakezilla-tray"
      for size in 48 128 256; do
        mkdir -p "$temp_dir/archive/icons/hicolor/${size}x${size}/apps"
        printf 'fixture-icon-%s\n' "$size" > \
          "$temp_dir/archive/icons/hicolor/${size}x${size}/apps/dev.wakezilla.Wakezilla.png"
      done
      tar -C "$temp_dir/archive" -czf "$output" wakezilla wakezilla-tray icons
      rm -rf "$temp_dir"
      ;;
  esac
  exit 0
fi

cat "$WAKEZILLA_FAKE_CURL_FIXTURE"
SH
  chmod +x "$command_path"
}

write_recording_fixture_curl() {
  command_path="$1"
  cat > "$command_path" <<'SH'
#!/usr/bin/env sh
: > "$WAKEZILLA_FAKE_CURL_ARGS"
for arg do
  printf '%s\n' "$arg" >> "$WAKEZILLA_FAKE_CURL_ARGS"
done
cat "$WAKEZILLA_FAKE_CURL_FIXTURE"
SH
  chmod +x "$command_path"
}

sha256_file() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    return 1
  fi
}

portable_file_mode() {
  mode_path="$1"
  if stat -f '%Lp' "$mode_path" >/dev/null 2>&1; then
    stat -f '%Lp' "$mode_path"
  else
    stat -c '%a' "$mode_path"
  fi
}

write_linux_integration_fixture() {
  extract_dir="$1"
  mkdir -p "$extract_dir/icons/hicolor/48x48/apps" \
    "$extract_dir/icons/hicolor/128x128/apps" \
    "$extract_dir/icons/hicolor/256x256/apps"
  printf '#!/usr/bin/env sh\nexit 0\n' > "$extract_dir/wakezilla-tray"
  chmod 755 "$extract_dir/wakezilla-tray"
  for size in 48 128 256; do
    printf 'wakezilla-icon-%s\n' "$size" > \
      "$extract_dir/icons/hicolor/${size}x${size}/apps/dev.wakezilla.Wakezilla.png"
  done
}

write_macos_integration_fixture() {
  extract_dir="$1"
  mkdir -p "$extract_dir"
  printf 'macos cli bytes\n' > "$extract_dir/wakezilla"
  printf 'macos tray bytes\n' > "$extract_dir/wakezilla-tray"
  printf 'macos icon bytes\n' > "$extract_dir/Wakezilla.icns"
  chmod 0755 "$extract_dir/wakezilla" "$extract_dir/wakezilla-tray"
  chmod 0644 "$extract_dir/Wakezilla.icns"
}

write_fake_plutil() {
  command_path="$1"
  cat > "$command_path" <<'SH'
#!/usr/bin/env sh
set -eu
if [ -n "${WAKEZILLA_MACOS_PLUTIL_LOG:-}" ]; then
  first=yes
  for argument do
    if [ "$first" = yes ]; then
      printf '%s' "$argument" >> "$WAKEZILLA_MACOS_PLUTIL_LOG"
      first=no
    else
      printf '|%s' "$argument" >> "$WAKEZILLA_MACOS_PLUTIL_LOG"
    fi
  done
  printf '\n' >> "$WAKEZILLA_MACOS_PLUTIL_LOG"
fi
case "${1:-}" in
  -lint)
    plist_path="$2"
    grep -F '<plist version="1.0">' "$plist_path" >/dev/null
    grep -F '</plist>' "$plist_path" >/dev/null
    ;;
  -extract)
    key="$2"
    shift 5
    plist_path="$1"
    awk -v wanted="<key>$key</key>" '
      index($0, wanted) {
        if (getline <= 0) exit 1
        value = $0
        sub(/^.*<string>/, "", value)
        sub(/<\/string>.*$/, "", value)
        print value
        found = 1
        exit 0
      }
      END { if (!found) exit 1 }
    ' "$plist_path"
    ;;
  *) exit 91 ;;
esac
SH
  chmod 0755 "$command_path"
}

write_fake_launchctl() {
  command_path="$1"
  cat > "$command_path" <<'SH'
#!/usr/bin/env sh
set -eu
action="${1:-}"
if [ -n "${WAKEZILLA_MACOS_LAUNCHCTL_LOG:-}" ]; then
  printf '%s' "$action" >> "$WAKEZILLA_MACOS_LAUNCHCTL_LOG"
  shift
  for argument do
    printf '|%s' "$argument" >> "$WAKEZILLA_MACOS_LAUNCHCTL_LOG"
  done
  printf '\n' >> "$WAKEZILLA_MACOS_LAUNCHCTL_LOG"
  set -- "$action" "$@"
fi
case "$action" in
  print)
    case "${2:-}" in
      */dev.wakezilla.tray)
        if [ "${WAKEZILLA_MACOS_OLD_AGENT_LOADED:-no}" = yes ]; then
          exit 0
        fi
        printf '%s\n' 'Could not find service' >&2
        exit 113
        ;;
      gui/*)
        case "${WAKEZILLA_MACOS_GUI_DOMAIN:-present}" in
          present) exit 0 ;;
          absent) printf '%s\n' 'Could not find domain' >&2; exit 113 ;;
          *) printf '%s\n' 'unexpected launchctl domain failure' >&2; exit 70 ;;
        esac
        ;;
    esac
    ;;
  bootout|bootstrap|kickstart)
    if [ "${WAKEZILLA_MACOS_FAIL_LAUNCHCTL:-}" = "$action" ]; then
      printf 'injected %s failure\n' "$action" >&2
      exit 72
    fi
    exit 0
    ;;
esac
exit 90
SH
  chmod 0755 "$command_path"
}

write_linux_release_archive() {
  archive_path="$1"
  version_status="$2"
  icon_marker="$3"
  archive_stage=$(mktemp -d)
  write_linux_integration_fixture "$archive_stage"
  cat > "$archive_stage/wakezilla" <<SH
#!/usr/bin/env sh
if [ "\$#" -eq 1 ] && [ "\${1:-}" = "--version" ]; then
  printf 'wakezilla 0.1.49\\n'
  exit $version_status
fi
exit 97
SH
  chmod 755 "$archive_stage/wakezilla"
  for archive_size in 48 128 256; do
    printf '%s-%s\n' "$icon_marker" "$archive_size" > \
      "$archive_stage/icons/hicolor/${archive_size}x${archive_size}/apps/dev.wakezilla.Wakezilla.png"
  done
  tar -C "$archive_stage" -czf "$archive_path" wakezilla wakezilla-tray icons
  rm -rf "$archive_stage"
}

write_fake_sudo() {
  command_path="$1"
  cat > "$command_path" <<'SH'
#!/usr/bin/env sh
printf '%s\n' "$@" >> "$WAKEZILLA_PRIVILEGE_LOG"
[ "${1:-}" = "-u" ] || exit 91
[ "${2:-}" != "root" ] || exit 92
shift 2
if [ "${1:-}" = "--" ]; then
  shift
fi
exec "$@"
SH
  chmod 755 "$command_path"
}

write_fake_chown() {
  command_path="$1"
  cat > "$command_path" <<'SH'
#!/usr/bin/env sh
printf '%s\n' "$@" >> "$WAKEZILLA_CHOWN_LOG"
exit 0
SH
  chmod 755 "$command_path"
}

test_help_includes_required_docs() {
  run_script --help
  assert_eq "0" "$status" "help exit status"
  assert_contains "$output" "Usage: install.sh" "help usage"
  assert_contains "$output" "VERSION" "help VERSION"
  assert_contains "$output" "BIN_DIR" "help BIN_DIR"
  assert_contains "$output" "PREFIX" "help PREFIX"
  assert_contains "$output" "TARGET" "help TARGET"
  assert_contains "$output" "REPO" "help REPO"
  assert_contains "$output" "GITHUB_TOKEN" "help GITHUB_TOKEN"
  assert_contains "$output" "curl -fsSL https://raw.githubusercontent.com/guibeira/wakezilla/main/install.sh | sh" "help curl example"
  assert_contains "$output" "curl -fsSL https://raw.githubusercontent.com/guibeira/wakezilla/main/install.sh | sh -s -- 0.1.49" "help version curl example"
  assert_contains "$output" "VERSION=0.1.49 BIN_DIR=/usr/local/bin sh install.sh" "help local install example"
}

test_no_args_resolves_release_metadata() {
  temp_dir=$(mktemp -d)
  old_path="$PATH"
  write_install_dependency_stubs "$temp_dir/bin"
  write_fixture_curl "$temp_dir/bin/curl"
  TARGET=x86_64-unknown-linux-gnu
  BIN_DIR="$temp_dir/install-bin"
  GITHUB_TOKEN=secret-token
  WAKEZILLA_FAKE_CURL_FIXTURE="$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json"
  PATH="$temp_dir/bin:$PATH"
  export TARGET BIN_DIR GITHUB_TOKEN WAKEZILLA_FAKE_CURL_FIXTURE PATH
  mkdir -p "$temp_dir/home"
  HOME="$temp_dir/home" \
  XDG_DATA_HOME="$temp_dir/data" \
  XDG_CONFIG_HOME="$temp_dir/config" \
  DISPLAY= WAYLAND_DISPLAY= \
    run_script
  unset TARGET BIN_DIR GITHUB_TOKEN WAKEZILLA_FAKE_CURL_FIXTURE
  PATH="$old_path"
  export PATH
  assert_eq "0" "$status" "release metadata exit status"
  assert_contains "$output" "installing wakezilla for x86_64-unknown-linux-gnu" "release metadata target"
  assert_contains "$output" "resolved wakezilla v0.1.49" "release metadata version"
  assert_contains "$output" "asset: https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz" "release metadata asset"
  canonical_install_dir=$(CDPATH= cd -- "$temp_dir/install-bin" && pwd -P)
  assert_contains "$output" "install dir: $canonical_install_dir" "release metadata install dir"
  assert_contains "$output" "installed wakezilla v0.1.49 to $canonical_install_dir/wakezilla" "release metadata installed"
  assert_not_contains "$output" "secret-token" "release metadata output token"
  if [ ! -x "$temp_dir/install-bin/wakezilla" ]; then
    fail "release metadata install: expected executable in temp BIN_DIR"
  fi
  if [ ! -f "$temp_dir/data/applications/dev.wakezilla.Wakezilla.desktop" ]; then
    fail "release metadata install: expected Linux application integration"
  fi
  if [ ! -f "$temp_dir/config/autostart/dev.wakezilla.tray.desktop" ]; then
    fail "release metadata install: expected Linux autostart integration"
  fi
  assert_contains "$output" "next graphical login" "release metadata headless integration message"
  rm -rf "$temp_dir"
}

test_end_to_end_install_with_fake_curl() {
  temp_dir=$(mktemp -d)
  old_path="$PATH"
  mkdir -p "$temp_dir/bin" "$temp_dir/archive" "$temp_dir/install"

  write_install_dependency_stubs "$temp_dir/bin"

  printf '#!/usr/bin/env sh\nif [ "$#" -eq 1 ] && [ "${1:-}" = "--version" ]; then\n  printf "wakezilla 0.1.49\\n"\n  exit 0\nfi\nexit 97\n' > "$temp_dir/archive/wakezilla"
  write_linux_integration_fixture "$temp_dir/archive"
  chmod +x "$temp_dir/archive/wakezilla"
  chmod +x "$temp_dir/archive/wakezilla-tray"
  tar -C "$temp_dir/archive" -czf "$temp_dir/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz" wakezilla wakezilla-tray icons
  if ! sha=$(sha256_file "$temp_dir/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"); then
    printf 'SKIP: end-to-end fake release test requires sha256sum or shasum\n'
    rm -rf "$temp_dir"
    return 0
  fi
  printf '%s  wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz\n' "$sha" > "$temp_dir/SHA256SUMS"

  cat > "$temp_dir/release.json" <<EOF
{
  "tag_name": "v0.1.49",
  "prerelease": false,
  "assets": [
    {
      "name": "wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz",
      "browser_download_url": "https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"
    }
  ]
}
EOF

  cat > "$temp_dir/bin/curl" <<EOF
#!/usr/bin/env sh
set -eu
out=
url=
while [ "\$#" -gt 0 ]; do
  case "\$1" in
    -o)
      out="\$2"
      shift 2
      ;;
    -H)
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      url="\$1"
      shift
      ;;
  esac
done

case "\$url" in
  https://api.github.com/repos/guibeira/wakezilla/releases/tags/v0.1.49)
    cat "$temp_dir/release.json"
    ;;
  https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz)
    cp "$temp_dir/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz" "\$out"
    ;;
  https://github.com/guibeira/wakezilla/releases/download/v0.1.49/SHA256SUMS)
    cp "$temp_dir/SHA256SUMS" "\$out"
    ;;
  *)
    printf 'unexpected url: %s\n' "\$url" >&2
    exit 1
    ;;
esac
EOF
  chmod +x "$temp_dir/bin/curl"

  output_file=$(mktemp)
  mkdir -p "$temp_dir/home"
  set +e
  PATH="$temp_dir/bin:$temp_dir/install:$PATH" \
    BIN_DIR="$temp_dir/install" \
    HOME="$temp_dir/home" \
    XDG_DATA_HOME="$temp_dir/data" \
    XDG_CONFIG_HOME="$temp_dir/config" \
    DISPLAY= \
    WAYLAND_DISPLAY= \
    TARGET=x86_64-unknown-linux-gnu \
    "$SCRIPT" 0.1.49 >"$output_file" 2>&1
  status=$?
  set -e
  output=$(cat "$output_file")
  rm -f "$output_file"
  PATH="$old_path"
  export PATH

  assert_eq "0" "$status" "end-to-end fake release exit status"
  assert_contains "$output" "installed wakezilla v0.1.49" "end-to-end installed version"
  assert_contains "$output" "resolved wakezilla v0.1.49" "end-to-end resolved version"
  assert_contains "$output" "asset: https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz" "end-to-end asset"
  canonical_install_dir=$(CDPATH= cd -- "$temp_dir/install" && pwd -P)
  assert_contains "$output" "install dir: $canonical_install_dir" "end-to-end install dir"
  assert_not_contains "$output" "unexpected url" "end-to-end no unexpected network"
  if [ ! -x "$temp_dir/install/wakezilla" ]; then
    fail "end-to-end install: expected installed binary"
  fi
  if [ ! -x "$temp_dir/install/wakezilla-tray" ]; then
    fail "end-to-end install: expected installed tray helper"
  fi
  if [ ! -f "$temp_dir/data/applications/dev.wakezilla.Wakezilla.desktop" ]; then
    fail "end-to-end install: expected Linux application entry"
  fi
  for size in 48 128 256; do
    if ! cmp -s \
      "$temp_dir/archive/icons/hicolor/${size}x${size}/apps/dev.wakezilla.Wakezilla.png" \
      "$temp_dir/data/icons/hicolor/${size}x${size}/apps/dev.wakezilla.Wakezilla.png"; then
      fail "end-to-end install: expected byte-identical ${size}x${size} icon"
    fi
  done

  rm -rf "$temp_dir"
}

test_end_to_end_macos_main_publishes_bundle_transaction_only() {
  temp_dir=$(mktemp -d)
  tools_dir="$temp_dir/tools"
  archive_dir="$temp_dir/archive"
  home_dir="$temp_dir/home"
  bin_dir="$temp_dir/install-bin"
  curl_log="$temp_dir/curl.log"
  launchctl_log="$temp_dir/launchctl.log"
  plutil_log="$temp_dir/plutil.log"
  mkdir -p "$tools_dir" "$archive_dir" "$home_dir" "$bin_dir"
  test_uid=$(id -u)
  write_install_dependency_stubs "$tools_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"

  cat > "$archive_dir/wakezilla" <<'SH'
#!/usr/bin/env sh
if [ "$#" -eq 1 ] && [ "${1:-}" = "--version" ]; then
  printf 'wakezilla 0.1.49\n'
  exit 0
fi
exit 97
SH
  printf '#!/usr/bin/env sh\nexit 0\n' > "$archive_dir/wakezilla-tray"
  printf 'macos e2e icon bytes\n' > "$archive_dir/Wakezilla.icns"
  chmod 0755 "$archive_dir/wakezilla" "$archive_dir/wakezilla-tray"
  archive="$temp_dir/wakezilla-0.1.49-aarch64-apple-darwin.tar.gz"
  tar -C "$archive_dir" -czf "$archive" wakezilla wakezilla-tray Wakezilla.icns
  checksum=$(sha256_file "$archive") || {
    printf 'SKIP: macOS end-to-end fixture requires sha256sum or shasum\n'
    rm -rf "$temp_dir"
    return 0
  }
  printf '%s  %s\n' "$checksum" "${archive##*/}" > "$temp_dir/SHA256SUMS"
  cat > "$temp_dir/release.json" <<'EOF'
{
  "tag_name": "v0.1.49",
  "assets": [
    {
      "name": "wakezilla-0.1.49-aarch64-apple-darwin.tar.gz",
      "browser_download_url": "https://example.test/wakezilla-0.1.49-aarch64-apple-darwin.tar.gz"
    }
  ]
}
EOF
  cat > "$tools_dir/curl" <<SH
#!/usr/bin/env sh
set -eu
out=
url=
while [ "\$#" -gt 0 ]; do
  case "\$1" in
    -o) out="\$2"; shift 2 ;;
    -H) shift 2 ;;
    -*) shift ;;
    *) url="\$1"; shift ;;
  esac
done
printf '%s\n' "\$url" >> '$curl_log'
case "\$url" in
  https://api.github.com/repos/guibeira/wakezilla/releases/tags/v0.1.49)
    cat '$temp_dir/release.json' ;;
  https://example.test/wakezilla-0.1.49-aarch64-apple-darwin.tar.gz)
    cp '$archive' "\$out" ;;
  https://github.com/guibeira/wakezilla/releases/download/v0.1.49/SHA256SUMS)
    cp '$temp_dir/SHA256SUMS' "\$out" ;;
  *) exit 91 ;;
esac
SH
  chmod 0755 "$tools_dir/curl"

  : > "$curl_log"
  root_output="$temp_dir/root-output"
  set +e
  PATH="$tools_dir:$PATH" HOME="$home_dir" BIN_DIR="$temp_dir/root-bin" \
    TARGET=aarch64-apple-darwin WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    sh -c '. "$1"; run_installer_main_at "$2" "$3" "$4" "$5"' \
      sh "$SCRIPT" 0 "$tools_dir/plutil" "$tools_dir/launchctl" 0.1.49 \
      > "$root_output" 2>&1
  root_status=$?
  set -e
  if [ "$root_status" -eq 0 ]; then
    fail "macOS main root refusal: expected fatal status"
  fi
  assert_contains "$(cat "$root_output")" 'without sudo' \
    "macOS main root refusal guidance"
  assert_eq "" "$(cat "$curl_log")" "macOS main root refusal happens before download"
  if [ -e "$temp_dir/root-bin" ]; then
    fail "macOS main root refusal: created BIN_DIR before rejection"
  fi

  real_home="$temp_dir/real-home"
  linked_home="$temp_dir/linked-home"
  mkdir -p "$real_home"
  ln -s "$real_home" "$linked_home"
  : > "$curl_log"
  set +e
  PATH="$tools_dir:$PATH" HOME="$linked_home" BIN_DIR= \
    TARGET=aarch64-apple-darwin WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    sh -c '. "$1"; run_installer_main_at "$2" "$3" "$4" "$5"' \
      sh "$SCRIPT" "$(id -u)" "$tools_dir/plutil" "$tools_dir/launchctl" 0.1.49 \
      > "$temp_dir/symlink-home-output" 2>&1
  unsafe_home_status=$?
  set -e
  if [ "$unsafe_home_status" -eq 0 ]; then
    fail "macOS main symlink HOME: expected fatal status"
  fi
  if [ -e "$real_home/.local" ]; then
    fail "macOS main symlink HOME: wrote default BIN_DIR before rejection"
  fi
  assert_eq "" "$(cat "$curl_log")" "macOS main symlink HOME rejected before download"

  : > "$curl_log"
  wrong_uid=$((test_uid + 1))
  set +e
  PATH="$tools_dir:$PATH" HOME="$home_dir" BIN_DIR= \
    TARGET=aarch64-apple-darwin WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    sh -c '. "$1"; run_installer_main_at "$2" "$3" "$4" "$5"' \
      sh "$SCRIPT" "$wrong_uid" "$tools_dir/plutil" "$tools_dir/launchctl" 0.1.49 \
      > "$temp_dir/owner-home-output" 2>&1
  unsafe_home_status=$?
  set -e
  if [ "$unsafe_home_status" -eq 0 ]; then
    fail "macOS main unowned HOME: expected fatal status"
  fi
  if [ -e "$home_dir/.local" ]; then
    fail "macOS main unowned HOME: wrote default BIN_DIR before rejection"
  fi
  assert_eq "" "$(cat "$curl_log")" "macOS main unowned HOME rejected before download"

  : > "$curl_log"
  : > "$launchctl_log"
  : > "$plutil_log"
  output_file="$temp_dir/install-output"
  set +e
  PATH="$tools_dir:$PATH" HOME="$home_dir" BIN_DIR="$bin_dir" \
    TARGET=aarch64-apple-darwin WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_MACOS_PLUTIL_LOG="$plutil_log" \
    WAKEZILLA_MACOS_LAUNCHCTL_LOG="$launchctl_log" \
    WAKEZILLA_MACOS_GUI_DOMAIN=present WAKEZILLA_SUDO_SYMLINK=no \
    sh -c '. "$1"; run_installer_main_at "$2" "$3" "$4" "$5"' \
      sh "$SCRIPT" "$test_uid" "$tools_dir/plutil" "$tools_dir/launchctl" 0.1.49 \
      > "$output_file" 2>&1
  install_status=$?
  set -e
  assert_eq "0" "$install_status" "macOS main end-to-end exit status"
  home_physical=$(CDPATH= cd -- "$home_dir" && pwd -P)
  bin_physical=$(CDPATH= cd -- "$bin_dir" && pwd -P)
  app="$home_physical/Applications/Wakezilla.app"
  agent="$home_physical/Library/LaunchAgents/dev.wakezilla.tray.plist"
  if [ ! -d "$app" ] || [ ! -f "$agent" ]; then
    fail "macOS main end-to-end: expected bundle and LaunchAgent"
  fi
  if [ ! -L "$bin_physical/wakezilla" ]; then
    fail "macOS main end-to-end: CLI endpoint must be a symlink, not a loose binary"
  else
    assert_eq "$app/Contents/MacOS/wakezilla" "$(readlink "$bin_physical/wakezilla")" \
      "macOS main end-to-end bundle CLI endpoint"
  fi
  if [ -e "$bin_physical/wakezilla-tray" ] || [ -L "$bin_physical/wakezilla-tray" ]; then
    fail "macOS main end-to-end: loose tray helper remained"
  fi
  launch_contents=$(cat "$agent" 2>/dev/null || printf '')
  assert_contains "$launch_contents" '<string>/usr/bin/open</string>' \
    "macOS main end-to-end graphical opener"
  assert_contains "$launch_contents" '<string>-g</string>' \
    "macOS main end-to-end background launch"
  assert_not_contains "$launch_contents" 'Terminal' \
    "macOS main end-to-end never opens Terminal"
  assert_contains "$(cat "$output_file")" 'installed wakezilla v0.1.49' \
    "macOS main end-to-end installed version"

  reinstall_output="$temp_dir/reinstall-output"
  set +e
  PATH="$tools_dir:$PATH" HOME="$home_dir" BIN_DIR="$bin_dir" \
    TARGET=aarch64-apple-darwin WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_MACOS_PLUTIL_LOG="$plutil_log" \
    WAKEZILLA_MACOS_LAUNCHCTL_LOG="$launchctl_log" \
    WAKEZILLA_MACOS_GUI_DOMAIN=present WAKEZILLA_MACOS_OLD_AGENT_LOADED=yes \
    WAKEZILLA_SUDO_SYMLINK=no \
    sh -c '. "$1"; run_installer_main_at "$2" "$3" "$4" "$5"' \
      sh "$SCRIPT" "$test_uid" "$tools_dir/plutil" "$tools_dir/launchctl" 0.1.49 \
      > "$reinstall_output" 2>&1
  reinstall_status=$?
  set -e
  assert_eq "0" "$reinstall_status" "macOS full-main reinstall accepts bundle CLI symlink"
  assert_eq "$app/Contents/MacOS/wakezilla" "$(readlink "$bin_physical/wakezilla")" \
    "macOS full-main reinstall preserves bundle CLI endpoint"
  if [ -e "$bin_physical/wakezilla-tray" ] || [ -L "$bin_physical/wakezilla-tray" ]; then
    fail "macOS full-main reinstall: loose tray helper remained"
  fi
  assert_contains "$(cat "$launchctl_log")" \
    "bootout|gui/$test_uid|$agent" "macOS full-main reinstall unloads prior agent"
  rm -rf "$temp_dir"
}

test_linux_integration_uses_final_musl_fallback_extract_once() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin" "$temp_dir/install" "$temp_dir/home"
  write_install_dependency_stubs "$temp_dir/bin"
  gnu_archive="$temp_dir/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"
  musl_archive="$temp_dir/wakezilla-0.1.49-x86_64-unknown-linux-musl.tar.gz"
  write_linux_release_archive "$gnu_archive" 1 gnu
  write_linux_release_archive "$musl_archive" 0 musl
  {
    printf '%s  %s\n' "$(sha256_file "$gnu_archive")" "${gnu_archive##*/}"
    printf '%s  %s\n' "$(sha256_file "$musl_archive")" "${musl_archive##*/}"
  } > "$temp_dir/SHA256SUMS"
  cat > "$temp_dir/release.json" <<'EOF'
{
  "tag_name": "v0.1.49",
  "assets": [
    {"name":"wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz","browser_download_url":"https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"},
    {"name":"wakezilla-0.1.49-x86_64-unknown-linux-musl.tar.gz","browser_download_url":"https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-musl.tar.gz"}
  ]
}
EOF
  cat > "$temp_dir/bin/curl" <<SH
#!/usr/bin/env sh
set -eu
out=
url=
while [ "\$#" -gt 0 ]; do
  case "\$1" in
    -o) out="\$2"; shift 2 ;;
    -H) shift 2 ;;
    -*) shift ;;
    *) url="\$1"; shift ;;
  esac
done
case "\$url" in
  https://api.github.com/repos/guibeira/wakezilla/releases/tags/v0.1.49) cat '$temp_dir/release.json' ;;
  https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz) cp '$gnu_archive' "\$out" ;;
  https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-musl.tar.gz) cp '$musl_archive' "\$out" ;;
  https://github.com/guibeira/wakezilla/releases/download/v0.1.49/SHA256SUMS) cp '$temp_dir/SHA256SUMS' "\$out" ;;
  *) printf 'unexpected url: %s\n' "\$url" >&2; exit 1 ;;
esac
SH
  chmod 755 "$temp_dir/bin/curl"

  output_file=$(mktemp)
  set +e
  PATH="$temp_dir/bin:$PATH" \
    HOME="$temp_dir/home" \
    BIN_DIR="$temp_dir/install" \
    XDG_DATA_HOME="$temp_dir/data" \
    XDG_CONFIG_HOME="$temp_dir/config" \
    TARGET=x86_64-unknown-linux-gnu \
    DISPLAY= WAYLAND_DISPLAY= \
    "$SCRIPT" 0.1.49 > "$output_file" 2>&1
  status=$?
  set -e
  output=$(cat "$output_file")
  rm -f "$output_file"

  assert_eq "0" "$status" "musl fallback integration status"
  assert_contains "$output" "retrying with x86_64-unknown-linux-musl" "musl fallback attempted"
  assert_contains "$output" "asset: https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-musl.tar.gz" "musl fallback final asset"
  integration_messages=$(printf '%s\n' "$output" | grep -c 'Linux desktop integration installed' || true)
  assert_eq "1" "$integration_messages" "musl fallback integration count"
  installed_icon="$temp_dir/data/icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png"
  assert_eq "musl-256" "$(cat "$installed_icon" 2>/dev/null || true)" "musl fallback final extract icon"
  rm -rf "$temp_dir"
}

test_linux_fallback_rejects_unrunnable_musl_without_publication() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin" "$temp_dir/install" "$temp_dir/home"
  write_install_dependency_stubs "$temp_dir/bin"
  gnu_archive="$temp_dir/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"
  musl_archive="$temp_dir/wakezilla-0.1.49-x86_64-unknown-linux-musl.tar.gz"
  write_linux_release_archive "$gnu_archive" 1 gnu-broken
  write_linux_release_archive "$musl_archive" 1 musl-broken
  {
    printf '%s  %s\n' "$(sha256_file "$gnu_archive")" "${gnu_archive##*/}"
    printf '%s  %s\n' "$(sha256_file "$musl_archive")" "${musl_archive##*/}"
  } > "$temp_dir/SHA256SUMS"
  cat > "$temp_dir/release.json" <<'EOF'
{
  "tag_name": "v0.1.49",
  "assets": [
    {"name":"wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz","browser_download_url":"https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"},
    {"name":"wakezilla-0.1.49-x86_64-unknown-linux-musl.tar.gz","browser_download_url":"https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-musl.tar.gz"}
  ]
}
EOF
  cat > "$temp_dir/bin/curl" <<SH
#!/usr/bin/env sh
set -eu
out=
url=
while [ "\$#" -gt 0 ]; do
  case "\$1" in
    -o) out="\$2"; shift 2 ;;
    -H) shift 2 ;;
    -*) shift ;;
    *) url="\$1"; shift ;;
  esac
done
case "\$url" in
  https://api.github.com/repos/guibeira/wakezilla/releases/tags/v0.1.49) cat '$temp_dir/release.json' ;;
  https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz) cp '$gnu_archive' "\$out" ;;
  https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-musl.tar.gz) cp '$musl_archive' "\$out" ;;
  https://github.com/guibeira/wakezilla/releases/download/v0.1.49/SHA256SUMS) cp '$temp_dir/SHA256SUMS' "\$out" ;;
  *) exit 1 ;;
esac
SH
  chmod 755 "$temp_dir/bin/curl"
  printf 'prior cli\n' > "$temp_dir/install/wakezilla"
  printf 'prior helper\n' > "$temp_dir/install/wakezilla-tray"

  output_file=$(mktemp)
  set +e
  PATH="$temp_dir/bin:$PATH" HOME="$temp_dir/home" BIN_DIR="$temp_dir/install" \
    XDG_DATA_HOME="$temp_dir/data" XDG_CONFIG_HOME="$temp_dir/config" \
    TARGET=x86_64-unknown-linux-gnu DISPLAY= WAYLAND_DISPLAY= \
    "$SCRIPT" 0.1.49 > "$output_file" 2>&1
  status=$?
  set -e
  output=$(cat "$output_file")
  rm -f "$output_file"

  if [ "$status" -eq 0 ]; then
    fail "unrunnable musl fallback: expected fatal status"
  fi
  assert_contains "$output" "retrying with x86_64-unknown-linux-musl" \
    "unrunnable musl fallback attempted"
  assert_contains "$output" "no runnable wakezilla binary" \
    "unrunnable musl fallback fatal error"
  assert_eq "prior cli" "$(cat "$temp_dir/install/wakezilla")" \
    "unrunnable musl fallback preserves CLI"
  assert_eq "prior helper" "$(cat "$temp_dir/install/wakezilla-tray")" \
    "unrunnable musl fallback preserves helper"
  if [ -d "$temp_dir/data/applications" ]; then
    fail "unrunnable musl fallback: expected no integration"
  fi
  rm -rf "$temp_dir"
}

test_end_to_end_rejects_malicious_archive_before_extracting() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin" "$temp_dir/archive" "$temp_dir/install" "$temp_dir/home"
  write_install_dependency_stubs "$temp_dir/bin"
  sentinel="$temp_dir/outside-sentinel"
  printf 'outside-sentinel-content\n' > "$sentinel"
  chmod 0640 "$sentinel"
  sentinel_mode=$(portable_file_mode "$sentinel")
  sentinel_contents=$(cat "$sentinel")

  ln -s "$sentinel" "$temp_dir/archive/wakezilla"
  write_linux_integration_fixture "$temp_dir/archive"
  malicious_archive="$temp_dir/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"
  tar -C "$temp_dir/archive" -czf "$malicious_archive" wakezilla wakezilla-tray icons
  printf '%s  %s\n' "$(sha256_file "$malicious_archive")" "${malicious_archive##*/}" > "$temp_dir/SHA256SUMS"
  cat > "$temp_dir/release.json" <<'EOF'
{
  "tag_name": "v0.1.49",
  "assets": [
    {"name":"wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz","browser_download_url":"https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"}
  ]
}
EOF
  cat > "$temp_dir/bin/curl" <<SH
#!/usr/bin/env sh
set -eu
out=
url=
while [ "\$#" -gt 0 ]; do
  case "\$1" in
    -o) out="\$2"; shift 2 ;;
    -H) shift 2 ;;
    -*) shift ;;
    *) url="\$1"; shift ;;
  esac
done
case "\$url" in
  https://api.github.com/repos/guibeira/wakezilla/releases/tags/v0.1.49) cat '$temp_dir/release.json' ;;
  https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz) cp '$malicious_archive' "\$out" ;;
  https://github.com/guibeira/wakezilla/releases/download/v0.1.49/SHA256SUMS) cp '$temp_dir/SHA256SUMS' "\$out" ;;
  *) exit 1 ;;
esac
SH
  chmod 755 "$temp_dir/bin/curl"

  output_file=$(mktemp)
  set +e
  PATH="$temp_dir/bin:$PATH" HOME="$temp_dir/home" BIN_DIR="$temp_dir/install" \
    XDG_DATA_HOME="$temp_dir/data" XDG_CONFIG_HOME="$temp_dir/config" \
    TARGET=x86_64-unknown-linux-gnu DISPLAY= WAYLAND_DISPLAY= \
    "$SCRIPT" 0.1.49 > "$output_file" 2>&1
  status=$?
  set -e
  output=$(cat "$output_file")
  rm -f "$output_file"

  if [ "$status" -eq 0 ]; then
    fail "malicious release archive: expected installer failure"
  fi
  assert_contains "$output" "unsafe release archive" "malicious release archive error"
  assert_eq "$sentinel_contents" "$(cat "$sentinel")" "malicious release archive sentinel contents"
  assert_eq "$sentinel_mode" "$(portable_file_mode "$sentinel")" "malicious release archive sentinel mode"
  if [ -e "$temp_dir/install/wakezilla" ]; then
    fail "malicious release archive: binary was installed"
  fi
  rm -rf "$temp_dir"
}

test_unrunnable_candidate_is_fatal_without_publication() {
  temp_dir=$(mktemp -d)
  old_path="$PATH"
  write_install_dependency_stubs "$temp_dir/bin"
  write_fixture_curl "$temp_dir/bin/curl"
  TARGET=x86_64-unknown-linux-gnu
  BIN_DIR="$temp_dir/install-bin"
  install_dir=$BIN_DIR
  WAKEZILLA_FAKE_CURL_FIXTURE="$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json"
  WAKEZILLA_FAKE_VERSION_EXITS_NONZERO=1
  PATH="$temp_dir/bin:$PATH"
  export TARGET BIN_DIR WAKEZILLA_FAKE_CURL_FIXTURE WAKEZILLA_FAKE_VERSION_EXITS_NONZERO PATH
  mkdir -p "$temp_dir/home" "$BIN_DIR"
  printf 'existing cli\n' > "$BIN_DIR/wakezilla"
  printf 'existing helper\n' > "$BIN_DIR/wakezilla-tray"
  chmod 0640 "$BIN_DIR/wakezilla"
  chmod 0600 "$BIN_DIR/wakezilla-tray"
  HOME="$temp_dir/home" \
  XDG_DATA_HOME="$temp_dir/data" \
  XDG_CONFIG_HOME="$temp_dir/config" \
  DISPLAY= WAYLAND_DISPLAY= \
    run_script
  unset TARGET BIN_DIR WAKEZILLA_FAKE_CURL_FIXTURE WAKEZILLA_FAKE_VERSION_EXITS_NONZERO
  PATH="$old_path"
  export PATH

  if [ "$status" -eq 0 ]; then
    fail "unrunnable candidate: expected fatal install status"
  fi
  assert_contains "$output" "no runnable wakezilla binary" "unrunnable candidate fatal error"
  assert_eq "existing cli" "$(cat "$install_dir/wakezilla")" \
    "unrunnable candidate preserves existing CLI"
  assert_eq "existing helper" "$(cat "$install_dir/wakezilla-tray")" \
    "unrunnable candidate preserves existing helper"
  assert_eq "640" "$(portable_file_mode "$install_dir/wakezilla")" \
    "unrunnable candidate preserves existing CLI mode"
  assert_eq "600" "$(portable_file_mode "$install_dir/wakezilla-tray")" \
    "unrunnable candidate preserves existing helper mode"
  if [ -d "$temp_dir/data/applications" ] || [ -d "$temp_dir/config/autostart" ]; then
    fail "unrunnable candidate: expected no profile integration"
  fi

  rm -rf "$temp_dir"
}

test_historical_version_only_candidate_installs() {
  temp_dir=$(mktemp -d)
  old_path="$PATH"
  write_install_dependency_stubs "$temp_dir/bin"
  write_fixture_curl "$temp_dir/bin/curl"
  TARGET=x86_64-unknown-linux-gnu
  BIN_DIR="$temp_dir/install-bin"
  WAKEZILLA_FAKE_CURL_FIXTURE="$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json"
  WAKEZILLA_FAKE_HISTORICAL_VERSION_ONLY=1
  PATH="$temp_dir/bin:$PATH"
  export TARGET BIN_DIR WAKEZILLA_FAKE_CURL_FIXTURE \
    WAKEZILLA_FAKE_HISTORICAL_VERSION_ONLY PATH
  mkdir -p "$temp_dir/home"
  HOME="$temp_dir/home" XDG_DATA_HOME="$temp_dir/data" \
    XDG_CONFIG_HOME="$temp_dir/config" DISPLAY= WAYLAND_DISPLAY= run_script
  unset TARGET BIN_DIR WAKEZILLA_FAKE_CURL_FIXTURE \
    WAKEZILLA_FAKE_HISTORICAL_VERSION_ONLY
  PATH="$old_path"
  export PATH

  assert_eq "0" "$status" "historical --version-only candidate status"
  assert_contains "$output" "installed wakezilla v0.1.49" \
    "historical --version-only candidate installed version"
  if [ ! -x "$temp_dir/install-bin/wakezilla" ]; then
    fail "historical --version-only candidate: expected binary publication"
  fi
  rm -rf "$temp_dir"
}

test_empty_version_candidate_is_fatal_without_publication() {
  temp_dir=$(mktemp -d)
  old_path="$PATH"
  write_install_dependency_stubs "$temp_dir/bin"
  write_fixture_curl "$temp_dir/bin/curl"
  TARGET=x86_64-unknown-linux-gnu
  BIN_DIR="$temp_dir/install-bin"
  WAKEZILLA_FAKE_CURL_FIXTURE="$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json"
  WAKEZILLA_FAKE_VERSION_EMPTY=1
  PATH="$temp_dir/bin:$PATH"
  export TARGET BIN_DIR WAKEZILLA_FAKE_CURL_FIXTURE WAKEZILLA_FAKE_VERSION_EMPTY PATH
  mkdir -p "$temp_dir/home"
  HOME="$temp_dir/home" XDG_DATA_HOME="$temp_dir/data" \
    XDG_CONFIG_HOME="$temp_dir/config" DISPLAY= WAYLAND_DISPLAY= run_script
  unset TARGET BIN_DIR WAKEZILLA_FAKE_CURL_FIXTURE WAKEZILLA_FAKE_VERSION_EMPTY
  PATH="$old_path"
  export PATH

  if [ "$status" -eq 0 ]; then
    fail "empty version candidate: expected fatal install status"
  fi
  assert_contains "$output" "no runnable wakezilla binary" \
    "empty version candidate fatal error"
  if [ -e "$temp_dir/install-bin/wakezilla" ] || \
     [ -e "$temp_dir/install-bin/wakezilla-tray" ]; then
    fail "empty version candidate: expected no binary publication"
  fi
  rm -rf "$temp_dir"
}

test_mismatched_version_candidate_is_fatal_without_publication() {
  temp_dir=$(mktemp -d)
  old_path="$PATH"
  write_install_dependency_stubs "$temp_dir/bin"
  write_fixture_curl "$temp_dir/bin/curl"
  TARGET=x86_64-unknown-linux-gnu
  BIN_DIR="$temp_dir/install-bin"
  install_dir=$BIN_DIR
  WAKEZILLA_FAKE_CURL_FIXTURE="$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json"
  WAKEZILLA_FAKE_VERSION_MISMATCH=1
  PATH="$temp_dir/bin:$PATH"
  export TARGET BIN_DIR WAKEZILLA_FAKE_CURL_FIXTURE \
    WAKEZILLA_FAKE_VERSION_MISMATCH PATH
  mkdir -p "$temp_dir/home" "$BIN_DIR"
  printf 'existing cli\n' > "$BIN_DIR/wakezilla"
  printf 'existing helper\n' > "$BIN_DIR/wakezilla-tray"
  chmod 0640 "$BIN_DIR/wakezilla"
  chmod 0600 "$BIN_DIR/wakezilla-tray"
  HOME="$temp_dir/home" XDG_DATA_HOME="$temp_dir/data" \
    XDG_CONFIG_HOME="$temp_dir/config" DISPLAY= WAYLAND_DISPLAY= run_script
  unset TARGET BIN_DIR WAKEZILLA_FAKE_CURL_FIXTURE \
    WAKEZILLA_FAKE_VERSION_MISMATCH
  PATH="$old_path"
  export PATH

  if [ "$status" -eq 0 ]; then
    fail "mismatched version candidate: expected fatal install status"
  fi
  assert_contains "$output" \
    "candidate version 0.1.50 does not match release version 0.1.49" \
    "mismatched version candidate error"
  assert_not_contains "$output" "retrying with" \
    "mismatched version candidate does not use compatibility fallback"
  assert_eq "existing cli" "$(cat "$install_dir/wakezilla")" \
    "mismatched version candidate preserves existing CLI"
  assert_eq "existing helper" "$(cat "$install_dir/wakezilla-tray")" \
    "mismatched version candidate preserves existing helper"
  assert_eq "640" "$(portable_file_mode "$install_dir/wakezilla")" \
    "mismatched version candidate preserves existing CLI mode"
  assert_eq "600" "$(portable_file_mode "$install_dir/wakezilla-tray")" \
    "mismatched version candidate preserves existing helper mode"
  if [ -d "$temp_dir/data/applications" ] || [ -d "$temp_dir/config/autostart" ]; then
    fail "mismatched version candidate: expected no profile integration"
  fi
  rm -rf "$temp_dir"
}

test_integration_failure_rolls_back_installed_binaries() {
  for prior_state in existing missing; do
    temp_dir=$(mktemp -d)
    old_path="$PATH"
    write_install_dependency_stubs "$temp_dir/bin"
    write_fixture_curl "$temp_dir/bin/curl"
    install_dir="$temp_dir/install-bin"
    mkdir -p "$temp_dir/home" "$temp_dir/data" "$install_dir"
    printf 'blocks applications directory\n' > "$temp_dir/data/applications"
    if [ "$prior_state" = existing ]; then
      printf 'prior cli\n' > "$install_dir/wakezilla"
      printf 'prior helper\n' > "$install_dir/wakezilla-tray"
      chmod 0640 "$install_dir/wakezilla"
      chmod 0600 "$install_dir/wakezilla-tray"
    fi

    TARGET=x86_64-unknown-linux-gnu \
    BIN_DIR="$install_dir" \
    WAKEZILLA_FAKE_CURL_FIXTURE="$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json" \
    PATH="$temp_dir/bin:$PATH" \
    HOME="$temp_dir/home" \
    XDG_DATA_HOME="$temp_dir/data" \
    XDG_CONFIG_HOME="$temp_dir/config" \
    DISPLAY= WAYLAND_DISPLAY= \
      run_script

    if [ "$status" -eq 0 ]; then
      fail "integration failure $prior_state binaries: expected nonzero status"
    fi
    assert_contains "$output" "error[integration]" \
      "integration failure $prior_state binaries error"
    if [ "$prior_state" = existing ]; then
      assert_eq "prior cli" "$(cat "$install_dir/wakezilla")" \
        "integration failure restores prior CLI"
      assert_eq "prior helper" "$(cat "$install_dir/wakezilla-tray")" \
        "integration failure restores prior helper"
      assert_eq "640" "$(portable_file_mode "$install_dir/wakezilla")" \
        "integration failure restores prior CLI mode"
      assert_eq "600" "$(portable_file_mode "$install_dir/wakezilla-tray")" \
        "integration failure restores prior helper mode"
    else
      if [ -e "$install_dir/wakezilla" ] || [ -L "$install_dir/wakezilla" ] || \
         [ -e "$install_dir/wakezilla-tray" ] || [ -L "$install_dir/wakezilla-tray" ]; then
        fail "integration failure removes newly installed binaries"
      fi
    fi
    temp_count=$(find "$install_dir" -name '.*.install.*' -print | wc -l | tr -d ' ')
    assert_eq "0" "$temp_count" "integration failure binary temporary cleanup"
    PATH="$old_path"
    export PATH
    rm -rf "$temp_dir"
  done
}

test_missing_dependency_reports_hint() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin"
  write_stub_command "$temp_dir/bin/curl"
  write_stub_command "$temp_dir/bin/tar"
  write_stub_command "$temp_dir/bin/sha256sum"
  write_stub_command "$temp_dir/bin/apt-get"

  output_file=$(mktemp)
  set +e
  PATH="$temp_dir/bin" /bin/sh "$SCRIPT" >"$output_file" 2>&1
  status=$?
  set -e
  output=$(cat "$output_file")
  rm -f "$output_file"
  rm -rf "$temp_dir"

  if [ "$status" -eq 0 ]; then
    fail "missing dependency exit status: expected nonzero, got 0"
  fi
  assert_contains "$output" "error[dependency]: jq is required" "missing dependency error"
  assert_contains "$output" "apt-get install -y jq" "missing dependency hint"
}

test_unknown_args_fail_with_parser_error() {
  run_script --unknown
  if [ "$status" -eq 0 ]; then
    fail "unknown args exit status: expected nonzero, got 0"
  fi
  assert_contains "$output" "error[args]: unknown option: --unknown (use --help for usage)" "unknown args parser error"
}

test_mode_executes_cleanly() {
  output_file=$(mktemp)
  set +e
  WAKEZILLA_INSTALL_SH_TEST_MODE=1 "$SCRIPT" >"$output_file" 2>&1
  status=$?
  set -e
  output=$(cat "$output_file")
  rm -f "$output_file"

  assert_eq "0" "$status" "test mode execute exit status"
  assert_eq "" "$output" "test mode execute output"
}

test_mode_sources_cleanly() {
  output_file=$(mktemp)
  set +e
  WAKEZILLA_INSTALL_SH_TEST_MODE=1 sh -c '. "$1"; printf sourced' sh "$SCRIPT" >"$output_file" 2>&1
  status=$?
  set -e
  output=$(cat "$output_file")
  rm -f "$output_file"

  assert_eq "0" "$status" "test mode source exit status"
  assert_eq "sourced" "$output" "test mode source output"
}

test_help_includes_required_docs
test_no_args_resolves_release_metadata
test_end_to_end_install_with_fake_curl
test_end_to_end_macos_main_publishes_bundle_transaction_only
test_linux_integration_uses_final_musl_fallback_extract_once
test_linux_fallback_rejects_unrunnable_musl_without_publication
test_end_to_end_rejects_malicious_archive_before_extracting
test_historical_version_only_candidate_installs
test_unrunnable_candidate_is_fatal_without_publication
test_empty_version_candidate_is_fatal_without_publication
test_mismatched_version_candidate_is_fatal_without_publication
test_integration_failure_rolls_back_installed_binaries
test_missing_dependency_reports_hint
test_unknown_args_fail_with_parser_error
test_mode_executes_cleanly
test_mode_sources_cleanly

load_install_helpers() {
  WAKEZILLA_INSTALL_SH_TEST_MODE=1 . "$SCRIPT"
}

test_detect_target_linux_x86_64() {
  target=$(WAKEZILLA_UNAME_S=Linux WAKEZILLA_UNAME_M=x86_64 WAKEZILLA_LIBC=gnu detect_target)
  assert_eq "x86_64-unknown-linux-gnu" "$target" "linux x86_64 target"
}

test_detect_target_linux_x86_64_musl() {
  target=$(WAKEZILLA_UNAME_S=Linux WAKEZILLA_UNAME_M=x86_64 WAKEZILLA_LIBC=musl detect_target)
  assert_eq "x86_64-unknown-linux-musl" "$target" "linux x86_64 musl target"
}

test_detect_target_macos_x86_64() {
  target=$(WAKEZILLA_UNAME_S=Darwin WAKEZILLA_UNAME_M=x86_64 detect_target)
  assert_eq "x86_64-apple-darwin" "$target" "macos x86_64 target"
}

test_detect_target_macos_arm64() {
  target=$(WAKEZILLA_UNAME_S=Darwin WAKEZILLA_UNAME_M=arm64 detect_target)
  assert_eq "aarch64-apple-darwin" "$target" "macos arm64 target"
}

test_detect_target_override() {
  target=$(TARGET=custom-target WAKEZILLA_UNAME_S=Other WAKEZILLA_UNAME_M=Other detect_target)
  assert_eq "custom-target" "$target" "target override"
}

test_detect_target_linux_arm64() {
  target=$(WAKEZILLA_UNAME_S=Linux WAKEZILLA_UNAME_M=aarch64 WAKEZILLA_LIBC=gnu detect_target)
  assert_eq "aarch64-unknown-linux-gnu" "$target" "linux arm64 target"
}

test_detect_target_linux_arm64_musl() {
  target=$(WAKEZILLA_UNAME_S=Linux WAKEZILLA_UNAME_M=aarch64 WAKEZILLA_LIBC=musl detect_target)
  assert_eq "aarch64-unknown-linux-musl" "$target" "linux arm64 musl target"
}

test_detect_target_unsupported_platform() {
  if output=$(WAKEZILLA_UNAME_S=FreeBSD WAKEZILLA_UNAME_M=x86_64 detect_target 2>&1); then
    fail "unsupported platform target: expected failure, got '$output'"
  else
    assert_contains "$output" "unsupported platform" "unsupported platform"
  fi
}

test_musl_fallback_target_gnu() {
  assert_command_exists musl_fallback_target "musl fallback helper" || return 0
  assert_eq "aarch64-unknown-linux-musl" "$(musl_fallback_target aarch64-unknown-linux-gnu)" "aarch64 gnu musl fallback"
  assert_eq "x86_64-unknown-linux-musl" "$(musl_fallback_target x86_64-unknown-linux-gnu)" "x86_64 gnu musl fallback"
}

test_musl_fallback_target_none() {
  assert_command_exists musl_fallback_target "musl fallback helper" || return 0
  assert_eq "" "$(musl_fallback_target aarch64-unknown-linux-musl)" "musl target has no fallback"
  assert_eq "" "$(musl_fallback_target aarch64-apple-darwin)" "darwin target has no fallback"
}

test_bin_dir_on_secure_path() {
  assert_command_exists bin_dir_on_secure_path "secure path helper" || return 0
  if bin_dir_on_secure_path /usr/local/bin; then
    :
  else
    fail "secure path: expected /usr/local/bin to be on secure_path"
  fi
  if bin_dir_on_secure_path /home/pi/.local/bin; then
    fail "secure path: expected ~/.local/bin to be off secure_path"
  fi
}

test_offer_sudo_symlink_skips_when_disabled() {
  assert_command_exists offer_sudo_symlink "sudo symlink helper" || return 0
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin"
  cat > "$temp_dir/bin/sudo" <<'SH'
#!/usr/bin/env sh
printf 'sudo should not be invoked\n' >&2
exit 1
SH
  chmod +x "$temp_dir/bin/sudo"
  old_path="$PATH"
  PATH="$temp_dir/bin:$PATH"

  # decision=no must never invoke sudo; it should print the manual hint and
  # return cleanly even when the binary is off secure_path.
  output=$(WAKEZILLA_EUID=1000 WAKEZILLA_SUDO_SYMLINK=no offer_sudo_symlink /tmp/wakezilla-bin 2>&1)
  PATH="$old_path"
  export PATH
  rm -rf "$temp_dir"

  assert_contains "$output" "sudo env" "sudo symlink disabled hint"
  assert_not_contains "$output" "sudo should not be invoked" "sudo symlink disabled does not run sudo"
}

test_offer_sudo_symlink_noop_for_root() {
  assert_command_exists offer_sudo_symlink "sudo symlink helper" || return 0
  output=$(WAKEZILLA_EUID=0 WAKEZILLA_SUDO_SYMLINK=no offer_sudo_symlink /tmp/wakezilla-bin 2>&1)
  assert_eq "" "$output" "root sudo symlink noop"
}

test_offer_sudo_symlink_noop_on_secure_path() {
  assert_command_exists offer_sudo_symlink "sudo symlink helper" || return 0
  output=$(WAKEZILLA_EUID=1000 WAKEZILLA_SUDO_SYMLINK=yes offer_sudo_symlink /usr/local/bin 2>&1)
  assert_eq "" "$output" "secure path sudo symlink noop"
}

test_install_argument_helpers_defined() {
  missing=0
  assert_command_exists parse_args "parse args helper" || missing=1
  assert_command_exists resolve_bin_dir "resolve bin dir helper" || missing=1
  assert_command_exists pkg_manager_hint "package manager hint helper" || missing=1
  assert_command_exists have_checksum_tool "checksum tool helper" || missing=1
  assert_command_exists check_dependencies "dependency check helper" || missing=1
  [ "$missing" -eq 0 ]
}

test_canonical_bin_dir_helper_defined() {
  assert_command_exists canonicalize_bin_dir "canonical bin dir helper"
}

test_canonicalize_bin_dir_makes_relative_path_physical_and_absolute() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/work/nested"
  resolved=$(
    cd "$temp_dir/work"
    canonicalize_bin_dir "nested/../wakezilla bin"
  )
  expected=$(CDPATH= cd -- "$temp_dir/work/wakezilla bin" && pwd -P)
  assert_eq "$expected" "$resolved" "canonical relative bin dir"
  if [ ! -d "$resolved" ]; then
    fail "canonical bin dir: expected created directory"
  fi
  rm -rf "$temp_dir"
}

test_parse_args_positional_version() {
  parsed_version=$(
    VERSION=
    parse_args 0.1.49
    printf '%s\n' "$VERSION"
  )
  assert_eq "0.1.49" "$parsed_version" "positional version"
}

test_parse_args_rejects_two_versions() {
  if output=$(
    VERSION=
    parse_args 0.1.49 0.1.50 2>&1
  ); then
    fail "parse args duplicate version: expected failure, got '$output'"
  else
    assert_contains "$output" "unexpected argument" "duplicate version error"
  fi
}

test_resolve_bin_dir_default() {
  bin_dir=$(
    HOME=/tmp/wakezilla-home
    WAKEZILLA_EUID=1000
    unset BIN_DIR || true
    unset PREFIX || true
    resolve_bin_dir
  )
  assert_eq "/tmp/wakezilla-home/.local/bin" "$bin_dir" "default bin dir"
}

test_resolve_bin_dir_root_default() {
  bin_dir=$(
    HOME=/root
    WAKEZILLA_EUID=0
    unset BIN_DIR || true
    unset PREFIX || true
    resolve_bin_dir
  )
  assert_eq "/usr/local/bin" "$bin_dir" "root default bin dir"
}

test_resolve_bin_dir_root_respects_overrides() {
  bin_dir=$(
    WAKEZILLA_EUID=0
    BIN_DIR=/custom/bin
    resolve_bin_dir
  )
  assert_eq "/custom/bin" "$bin_dir" "root BIN_DIR override"

  bin_dir=$(
    WAKEZILLA_EUID=0
    unset BIN_DIR || true
    PREFIX=/opt/wakezilla
    resolve_bin_dir
  )
  assert_eq "/opt/wakezilla/bin" "$bin_dir" "root PREFIX override"
}

test_resolve_bin_dir_prefix() {
  bin_dir=$(
    unset BIN_DIR || true
    PREFIX=/opt/wakezilla
    resolve_bin_dir
  )
  assert_eq "/opt/wakezilla/bin" "$bin_dir" "prefix bin dir"
}

test_resolve_bin_dir_override() {
  bin_dir=$(
    BIN_DIR=/custom/bin
    PREFIX=/ignored
    resolve_bin_dir
  )
  assert_eq "/custom/bin" "$bin_dir" "BIN_DIR override"
}

test_resolve_bin_dir_requires_home_for_default() {
  if output=$(
    WAKEZILLA_EUID=1000
    unset BIN_DIR || true
    unset PREFIX || true
    unset HOME || true
    resolve_bin_dir 2>&1
  ); then
    fail "missing HOME bin dir: expected failure, got '$output'"
  else
    assert_contains "$output" "HOME is not set" "missing HOME bin dir"
  fi
}

test_validate_tar_archive_rejects_unsafe_member_kinds_and_paths() {
  temp_dir=$(mktemp -d)
  archive_dir="$temp_dir/archive"
  mkdir -p "$archive_dir"
  printf 'regular\n' > "$archive_dir/regular"
  ln -s regular "$archive_dir/symlink"
  ln "$archive_dir/regular" "$archive_dir/hardlink"
  mkfifo "$archive_dir/fifo"

  tar -C "$archive_dir" -czf "$temp_dir/symlink.tar.gz" symlink 2>/dev/null
  tar -C "$archive_dir" -czf "$temp_dir/hardlink.tar.gz" regular hardlink 2>/dev/null
  tar -C "$archive_dir" -czf "$temp_dir/fifo.tar.gz" fifo 2>/dev/null
  for archive_kind in symlink hardlink fifo; do
    if validation_error=$(validate_tar_archive "$temp_dir/$archive_kind.tar.gz"); then
      fail "archive $archive_kind member: expected rejection"
    else
      assert_contains "$validation_error" "unsupported archive member type" \
        "archive $archive_kind member rejection"
    fi
  done

  tar -czPf "$temp_dir/absolute.tar.gz" "$archive_dir/regular" 2>/dev/null
  if validation_error=$(validate_tar_archive "$temp_dir/absolute.tar.gz"); then
    fail "archive absolute member: expected rejection"
  else
    assert_contains "$validation_error" "absolute member path" "archive absolute member rejection"
  fi

  if tar --version 2>/dev/null | grep -q 'GNU tar'; then
    tar -C "$archive_dir" --transform='s|^regular$|../escape|' \
      -czf "$temp_dir/traversal.tar.gz" regular 2>/dev/null
  else
    tar -C "$archive_dir" -s ',^regular$,../escape,' \
      -czf "$temp_dir/traversal.tar.gz" regular 2>/dev/null
  fi
  if validation_error=$(validate_tar_archive "$temp_dir/traversal.tar.gz"); then
    fail "archive traversal member: expected rejection"
  else
    assert_contains "$validation_error" "parent traversal member path" \
      "archive traversal member rejection"
  fi

  rm -rf "$temp_dir"
}

test_validate_tar_archive_rejects_control_character_member_names() {
  temp_dir=$(mktemp -d)
  archive_dir="$temp_dir/archive"
  mkdir -p "$archive_dir"

  for control_name in tab carriage_return escape delete line_feed; do
    case "$control_name" in
      tab) control_character=$(printf '\t_'); control_character=${control_character%_} ;;
      carriage_return) control_character=$(printf '\r_'); control_character=${control_character%_} ;;
      escape) control_character=$(printf '\033_'); control_character=${control_character%_} ;;
      delete) control_character=$(printf '\177_'); control_character=${control_character%_} ;;
      line_feed) control_character=$(printf '\n_'); control_character=${control_character%_} ;;
    esac
    archive_member="unsafe${control_character}member"
    printf 'control fixture\n' > "$archive_dir/$archive_member"
    tar -C "$archive_dir" -czf "$temp_dir/$control_name.tar.gz" \
      "$archive_member" 2>/dev/null

    if validation_error=$(validate_tar_archive "$temp_dir/$control_name.tar.gz"); then
      fail "archive $control_name member name: expected rejection"
    else
      assert_contains "$validation_error" "unsupported archive member name" \
        "archive $control_name member name rejection"
    fi
    rm -f "$archive_dir/$archive_member"
  done

  rm -rf "$temp_dir"
}

test_validate_tar_archive_rejects_nonportable_and_duplicate_names() {
  temp_dir=$(mktemp -d)
  archive_dir="$temp_dir/archive"
  mkdir -p "$archive_dir/first" "$archive_dir/second"
  printf 'backslash fixture\n' > "$archive_dir/unsafe\member"
  printf 'regular fixture\n' > "$archive_dir/regular"
  printf 'first cli\n' > "$archive_dir/first/wakezilla"
  printf 'second cli\n' > "$archive_dir/second/wakezilla"
  printf 'first helper\n' > "$archive_dir/first/wakezilla-tray"
  printf 'second helper\n' > "$archive_dir/second/wakezilla-tray"

  tar -C "$archive_dir" -czf "$temp_dir/backslash.tar.gz" 'unsafe\member' 2>/dev/null
  if validation_error=$(validate_tar_archive "$temp_dir/backslash.tar.gz"); then
    fail "archive backslash member name: expected rejection"
  else
    assert_contains "$validation_error" "unsupported archive member name" \
      "archive backslash member name rejection"
  fi

  tar -C "$archive_dir" -czf "$temp_dir/duplicate.tar.gz" regular regular 2>/dev/null
  if validation_error=$(validate_tar_archive "$temp_dir/duplicate.tar.gz"); then
    fail "archive duplicate member name: expected rejection"
  else
    assert_contains "$validation_error" "duplicate archive member" \
      "archive duplicate member rejection"
  fi

  for executable_name in wakezilla wakezilla-tray; do
    tar -C "$archive_dir" -czf "$temp_dir/ambiguous-$executable_name.tar.gz" \
      "first/$executable_name" "second/$executable_name" 2>/dev/null
    if validation_error=$(validate_tar_archive \
      "$temp_dir/ambiguous-$executable_name.tar.gz" wakezilla wakezilla-tray); then
      fail "archive ambiguous $executable_name basename: expected rejection"
    else
      assert_contains "$validation_error" "ambiguous archive executable" \
        "archive ambiguous $executable_name basename rejection"
    fi
  done

  rm -rf "$temp_dir"
}

test_validate_tar_archive_allows_legacy_directory_named_like_binary() {
  temp_dir=$(mktemp -d)
  archive_dir="$temp_dir/archive"
  mkdir -p "$archive_dir/wakezilla"
  printf 'legacy nested cli\n' > "$archive_dir/wakezilla/wakezilla"
  tar -C "$archive_dir" -czf "$temp_dir/legacy-nested.tar.gz" wakezilla 2>/dev/null

  if validation_error=$(validate_tar_archive \
    "$temp_dir/legacy-nested.tar.gz" wakezilla wakezilla-tray); then
    :
  else
    fail "archive legacy binary directory: expected acceptance, got '$validation_error'"
  fi
  rm -rf "$temp_dir"
}

load_install_helpers
test_validate_tar_archive_rejects_unsafe_member_kinds_and_paths
test_validate_tar_archive_rejects_control_character_member_names
test_validate_tar_archive_rejects_nonportable_and_duplicate_names
test_validate_tar_archive_allows_legacy_directory_named_like_binary
if test_canonical_bin_dir_helper_defined; then
  test_canonicalize_bin_dir_makes_relative_path_physical_and_absolute
fi
test_detect_target_linux_x86_64
test_detect_target_linux_x86_64_musl
test_detect_target_macos_x86_64
test_detect_target_macos_arm64
test_detect_target_override
test_detect_target_linux_arm64
test_detect_target_linux_arm64_musl
test_detect_target_unsupported_platform
test_musl_fallback_target_gnu
test_musl_fallback_target_none
test_bin_dir_on_secure_path
test_offer_sudo_symlink_skips_when_disabled
test_offer_sudo_symlink_noop_for_root
test_offer_sudo_symlink_noop_on_secure_path
if test_install_argument_helpers_defined; then
  test_parse_args_positional_version
  test_parse_args_rejects_two_versions
  test_resolve_bin_dir_default
  test_resolve_bin_dir_root_default
  test_resolve_bin_dir_root_respects_overrides
  test_resolve_bin_dir_prefix
  test_resolve_bin_dir_override
  test_resolve_bin_dir_requires_home_for_default
fi

test_pkg_manager_hint_apt() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin"
  printf '#!/usr/bin/env sh\nexit 0\n' > "$temp_dir/bin/apt-get"
  chmod +x "$temp_dir/bin/apt-get"
  hint=$(PATH="$temp_dir/bin" pkg_manager_hint jq)
  assert_eq "apt-get install -y jq" "$hint" "apt package hint"
  rm -rf "$temp_dir"
}

test_pkg_manager_hint_unknown() {
  temp_dir=$(mktemp -d)
  hint=$(PATH="$temp_dir" pkg_manager_hint jq)
  assert_eq "install jq via your package manager" "$hint" "unknown package hint"
  rm -rf "$temp_dir"
}

if command -v pkg_manager_hint >/dev/null 2>&1; then
  test_pkg_manager_hint_apt
  test_pkg_manager_hint_unknown
fi

test_github_api_helpers_defined() {
  missing=0
  assert_command_exists github_api "github api helper" || missing=1
  assert_command_exists fetch_release_json "fetch release json helper" || missing=1
  [ "$missing" -eq 0 ]
}

test_fetch_release_json_latest_request() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin"
  args_file="$temp_dir/curl-args"
  write_recording_fixture_curl "$temp_dir/bin/curl"

  json=$(
    unset GITHUB_TOKEN || true
    REPO=guibeira/wakezilla
    WAKEZILLA_FAKE_CURL_ARGS="$args_file"
    WAKEZILLA_FAKE_CURL_FIXTURE="$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json"
    PATH="$temp_dir/bin:$PATH"
    export WAKEZILLA_FAKE_CURL_ARGS WAKEZILLA_FAKE_CURL_FIXTURE PATH
    fetch_release_json ""
  )
  curl_args=$(cat "$args_file")

  assert_contains "$json" '"tag_name": "v0.1.49"' "latest request fixture output"
  assert_contains "$curl_args" "https://api.github.com/repos/guibeira/wakezilla/releases/latest" "latest request endpoint"
  assert_contains "$curl_args" "-H" "latest request header flag"
  assert_contains "$curl_args" "Accept: application/vnd.github+json" "latest request accept header"
  assert_contains "$curl_args" "X-GitHub-Api-Version: 2022-11-28" "latest request api version header"
  assert_not_contains "$curl_args" "Authorization:" "latest request without token"

  rm -rf "$temp_dir"
}

test_fetch_release_json_version_request() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin"
  args_file="$temp_dir/curl-args"
  write_recording_fixture_curl "$temp_dir/bin/curl"

  (
    REPO=guibeira/wakezilla
    WAKEZILLA_FAKE_CURL_ARGS="$args_file"
    WAKEZILLA_FAKE_CURL_FIXTURE="$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json"
    PATH="$temp_dir/bin:$PATH"
    export WAKEZILLA_FAKE_CURL_ARGS WAKEZILLA_FAKE_CURL_FIXTURE PATH
    fetch_release_json "0.1.49"
  ) >/dev/null
  curl_args=$(cat "$args_file")

  assert_contains "$curl_args" "https://api.github.com/repos/guibeira/wakezilla/releases/tags/v0.1.49" "version request endpoint"

  rm -rf "$temp_dir"
}

test_fetch_release_json_token_request() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin"
  args_file="$temp_dir/curl-args"
  write_recording_fixture_curl "$temp_dir/bin/curl"

  (
    REPO=guibeira/wakezilla
    GITHUB_TOKEN=secret-token
    WAKEZILLA_FAKE_CURL_ARGS="$args_file"
    WAKEZILLA_FAKE_CURL_FIXTURE="$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json"
    PATH="$temp_dir/bin:$PATH"
    export WAKEZILLA_FAKE_CURL_ARGS WAKEZILLA_FAKE_CURL_FIXTURE PATH
    fetch_release_json ""
  ) >/dev/null
  curl_args=$(cat "$args_file")

  assert_contains "$curl_args" "Authorization: Bearer secret-token" "token request authorization header"

  rm -rf "$temp_dir"
}

if test_github_api_helpers_defined; then
  test_fetch_release_json_latest_request
  test_fetch_release_json_version_request
  test_fetch_release_json_token_request
fi

test_install_release_json_helpers_defined() {
  missing=0
  assert_command_exists release_version_from_json "release version json helper" || missing=1
  assert_command_exists asset_url_from_json "asset url json helper" || missing=1
  assert_command_exists available_targets_from_json "available targets json helper" || missing=1
  assert_command_exists download_file "download helper" || missing=1
  assert_command_exists checksum_url_for_release "checksum url helper" || missing=1
  assert_command_exists verify_checksum "verify checksum helper" || missing=1
  assert_command_exists extract_binary "extract binary helper" || missing=1
  assert_command_exists install_optional_tray_helper "install optional tray helper" || missing=1
  assert_command_exists install_bin "install binary helper" || missing=1
  [ "$missing" -eq 0 ]
}

test_release_version_from_json() {
  version=$(release_version_from_json < "$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json")
  assert_eq "0.1.49" "$version" "release version from json"
}

test_asset_url_from_json() {
  url=$(asset_url_from_json wakezilla 0.1.49 x86_64-unknown-linux-gnu < "$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json")
  assert_eq "https://example.test/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz" "$url" "asset url"
}

test_available_targets_from_json() {
  targets=$(available_targets_from_json wakezilla < "$ROOT_DIR/tests/fixtures/install/release-v0.1.49.json" | tr '\n' ' ')
  assert_contains "$targets" "x86_64-unknown-linux-gnu" "available linux target"
  assert_contains "$targets" "aarch64-apple-darwin" "available mac target"
}

test_verify_checksum_sha256sum() {
  if ! command -v sha256sum >/dev/null 2>&1; then
    printf 'SKIP: sha256sum checksum test\n'
    return 0
  fi

  temp_dir=$(mktemp -d)
  printf 'hello\n' > "$temp_dir/file.txt"
  sha=$(sha256sum "$temp_dir/file.txt" | awk '{print $1}')
  printf '%s  file.txt\n' "$sha" > "$temp_dir/SHA256SUMS"

  verify_checksum "$temp_dir/file.txt" "$temp_dir/SHA256SUMS" "file.txt"
  rm -rf "$temp_dir"
}

test_verify_checksum_rejects_mismatch() {
  if ! command -v sha256sum >/dev/null 2>&1; then
    printf 'SKIP: sha256sum mismatch test\n'
    return 0
  fi

  temp_dir=$(mktemp -d)
  printf 'hello\n' > "$temp_dir/file.txt"
  printf '0000000000000000000000000000000000000000000000000000000000000000  file.txt\n' > "$temp_dir/SHA256SUMS"

  if output=$(verify_checksum "$temp_dir/file.txt" "$temp_dir/SHA256SUMS" "file.txt" 2>&1); then
    fail "checksum mismatch: expected failure, got '$output'"
  else
    assert_contains "$output" "checksum verification failed" "checksum mismatch"
  fi
  rm -rf "$temp_dir"
}

test_extract_binary_from_tarball() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/archive"
  printf '#!/usr/bin/env sh\nif [ "$#" -eq 1 ] && [ "${1:-}" = "--version" ]; then\n  printf "wakezilla 0.1.49\\n"\n  exit 0\nfi\nexit 97\n' > "$temp_dir/archive/wakezilla"
  chmod +x "$temp_dir/archive/wakezilla"
  tar -C "$temp_dir/archive" -czf "$temp_dir/wakezilla.tar.gz" wakezilla

  extracted=$(extract_binary "$temp_dir/wakezilla.tar.gz" "$temp_dir/out" wakezilla)
  if [ ! -x "$extracted" ]; then
    fail "extract binary: expected executable at $extracted"
  fi

  rm -rf "$temp_dir"
}

test_extract_binary_from_nested_tarball() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/archive/nested"
  printf '#!/usr/bin/env sh\nif [ "$#" -eq 1 ] && [ "${1:-}" = "--version" ]; then\n  printf "wakezilla 0.1.49\\n"\n  exit 0\nfi\nexit 97\n' > "$temp_dir/archive/nested/wakezilla"
  chmod +x "$temp_dir/archive/nested/wakezilla"
  tar -C "$temp_dir/archive" -czf "$temp_dir/wakezilla.tar.gz" nested/wakezilla

  extracted=$(extract_binary "$temp_dir/wakezilla.tar.gz" "$temp_dir/out" wakezilla)
  if [ ! -x "$extracted" ]; then
    fail "extract nested binary: expected executable at $extracted"
  fi
  assert_contains "$extracted" "/nested/wakezilla" "extract nested binary path"

  rm -rf "$temp_dir"
}

test_install_optional_tray_helper_installs_when_present() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/extract" "$temp_dir/bin"
  printf '#!/usr/bin/env sh\nexit 0\n' > "$temp_dir/extract/wakezilla-tray"

  install_optional_tray_helper "$temp_dir/extract" "$temp_dir/bin"
  if [ ! -x "$temp_dir/bin/wakezilla-tray" ]; then
    fail "install optional tray helper: expected executable helper"
  fi

  rm -rf "$temp_dir"
}

test_install_optional_tray_helper_removes_stale_helper() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/extract" "$temp_dir/bin"
  printf '#!/usr/bin/env sh\nexit 0\n' > "$temp_dir/bin/wakezilla-tray"
  chmod +x "$temp_dir/bin/wakezilla-tray"

  install_optional_tray_helper "$temp_dir/extract" "$temp_dir/bin"
  if [ -e "$temp_dir/bin/wakezilla-tray" ]; then
    fail "install optional tray helper: expected stale helper removal"
  fi

  rm -rf "$temp_dir"
}

test_install_bin_sets_executable() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin"
  printf '#!/usr/bin/env sh\nexit 0\n' > "$temp_dir/src"

  install_bin "$temp_dir/src" "$temp_dir/bin/wakezilla"
  if [ ! -x "$temp_dir/bin/wakezilla" ]; then
    fail "install bin: expected executable destination"
  fi

  rm -rf "$temp_dir"
}

test_install_bin_fallback_replaces_symlink() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/path" "$temp_dir/bin"
  write_exec_wrapper "$temp_dir/path/dirname" "$(command -v dirname)"
  write_exec_wrapper "$temp_dir/path/rm" "$(command -v rm)"
  write_exec_wrapper "$temp_dir/path/cp" "$(command -v cp)"
  write_exec_wrapper "$temp_dir/path/chmod" "$(command -v chmod)"
  write_exec_wrapper "$temp_dir/path/mv" "$(command -v mv)"
  write_exec_wrapper "$temp_dir/path/mktemp" "$(command -v mktemp)"

  printf '#!/usr/bin/env sh\nexit 0\n' > "$temp_dir/src"
  printf 'outside\n' > "$temp_dir/outside"
  ln -s "$temp_dir/outside" "$temp_dir/bin/wakezilla"

  old_path="$PATH"
  PATH="$temp_dir/path"
  export PATH
  install_bin "$temp_dir/src" "$temp_dir/bin/wakezilla"
  PATH="$old_path"
  export PATH

  if [ -L "$temp_dir/bin/wakezilla" ]; then
    fail "install bin fallback symlink: expected destination symlink to be replaced"
  fi
  outside=$(cat "$temp_dir/outside")
  assert_eq "outside" "$outside" "install bin fallback symlink target unchanged"
  if [ ! -x "$temp_dir/bin/wakezilla" ]; then
    fail "install bin fallback symlink: expected executable replacement"
  fi

  rm -rf "$temp_dir"
}

test_install_bin_atomic_failures_preserve_destination() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/path" "$temp_dir/bin"
  printf '#!/usr/bin/env sh\nexit 0\n' > "$temp_dir/src"

  for failing_command in cp chmod mv; do
    rm -f "$temp_dir/path"/*
    for command_name in dirname rm cp chmod mv mktemp; do
      if [ "$command_name" = "$failing_command" ]; then
        cat > "$temp_dir/path/$command_name" <<'SH'
#!/bin/sh
exit 73
SH
        chmod 755 "$temp_dir/path/$command_name"
      else
        write_exec_wrapper "$temp_dir/path/$command_name" "$(command -v "$command_name")"
      fi
    done
    printf 'existing destination\n' > "$temp_dir/bin/wakezilla"
    chmod 0640 "$temp_dir/bin/wakezilla"

    set +e
    PATH="$temp_dir/path" install_bin "$temp_dir/src" "$temp_dir/bin/wakezilla"
    install_status=$?
    set -e
    assert_eq "73" "$install_status" \
      "install bin $failing_command failure propagates status"
    assert_eq "existing destination" "$(cat "$temp_dir/bin/wakezilla")" \
      "install bin $failing_command failure preserves destination"
    assert_eq "640" "$(portable_file_mode "$temp_dir/bin/wakezilla")" \
      "install bin $failing_command failure preserves mode"
    temp_count=$(find "$temp_dir/bin" -name '.wakezilla.install.*' -print | wc -l | tr -d ' ')
    assert_eq "0" "$temp_count" "install bin $failing_command failure cleans temporary"
  done
  rm -rf "$temp_dir"
}

if test_install_release_json_helpers_defined; then
  test_release_version_from_json
  test_asset_url_from_json
  test_available_targets_from_json
  test_verify_checksum_sha256sum
  test_verify_checksum_rejects_mismatch
  test_extract_binary_from_tarball
  test_extract_binary_from_nested_tarball
  test_install_optional_tray_helper_installs_when_present
  test_install_optional_tray_helper_removes_stale_helper
  test_install_bin_sets_executable
  test_install_bin_fallback_replaces_symlink
  test_install_bin_atomic_failures_preserve_destination
fi

test_path_guidance_helpers_defined() {
  missing=0
  assert_command_exists path_guidance "path guidance helper" || missing=1
  [ "$missing" -eq 0 ]
}

test_path_guidance_when_missing() {
  output=$(PATH=/usr/bin SHELL=/bin/zsh path_guidance /tmp/wakezilla-bin)
  assert_contains "$output" "add /tmp/wakezilla-bin to your PATH" "path guidance missing"
  assert_contains "$output" ".zshrc" "zsh rc guidance"
}

test_path_guidance_when_present() {
  output=$(PATH="/tmp/wakezilla-bin:/usr/bin" SHELL=/bin/zsh path_guidance /tmp/wakezilla-bin)
  assert_eq "" "$output" "no path guidance when present"
}

test_path_guidance_when_home_unset() {
  output=$(
    unset HOME || true
    unset PATH || true
    SHELL=/bin/zsh path_guidance /tmp/wakezilla-bin
  )
  assert_contains "$output" "add /tmp/wakezilla-bin to your PATH" "path guidance without HOME"
  assert_contains "$output" "export PATH=\"/tmp/wakezilla-bin:\$PATH\"" "path guidance without HOME export"
}

test_path_guidance_quotes_zsh_rc_with_spaces() {
  output=$(
    PATH=/usr/bin
    SHELL=/bin/zsh
    ZDOTDIR="/tmp/wakezilla zsh home"
    path_guidance /tmp/wakezilla-bin
  )
  assert_contains "$output" '>> "/tmp/wakezilla zsh home/.zshrc"' "zsh rc redirection quoted"
  assert_contains "$output" 'source "/tmp/wakezilla zsh home/.zshrc"' "zsh rc source quoted"
}

test_path_guidance_quotes_bash_rc_with_spaces() {
  output=$(
    PATH=/usr/bin
    SHELL=/bin/bash
    HOME="/tmp/wakezilla bash home"
    WAKEZILLA_UNAME_S=Linux
    path_guidance /tmp/wakezilla-bin
  )
  assert_contains "$output" '>> "/tmp/wakezilla bash home/.bashrc"' "bash rc redirection quoted"
  assert_contains "$output" 'source "/tmp/wakezilla bash home/.bashrc"' "bash rc source quoted"
}

test_path_guidance_quotes_fish_bin_dir_with_spaces() {
  output=$(PATH=/usr/bin SHELL=/usr/bin/fish path_guidance "/tmp/wakezilla bin")
  assert_contains "$output" 'fish_add_path "/tmp/wakezilla bin"' "fish bin dir quoted"
}

if test_path_guidance_helpers_defined; then
  test_path_guidance_when_missing
  test_path_guidance_when_present
  test_path_guidance_when_home_unset
  test_path_guidance_quotes_zsh_rc_with_spaces
  test_path_guidance_quotes_bash_rc_with_spaces
  test_path_guidance_quotes_fish_bin_dir_with_spaces
fi

file_mode() {
  mode_file="$1"
  if stat -f '%Lp' "$mode_file" >/dev/null 2>&1; then
    stat -f '%Lp' "$mode_file"
  else
    stat -c '%a' "$mode_file"
  fi
}

reference_decode_desktop_exec() {
  awk '
    function desktop_unescape(value,    output, i, current, following) {
      output = ""
      for (i = 1; i <= length(value); i++) {
        current = substr(value, i, 1)
        if (current != "\\") {
          output = output current
          continue
        }
        following = substr(value, i + 1, 1)
        if (following == "\\") output = output "\\"
        else if (following == "s") output = output " "
        else if (following == "n") output = output "\n"
        else if (following == "t") output = output "\t"
        else if (following == "r") output = output "\r"
        else exit 2
        i++
      }
      return output
    }
    {
      value = $0
      sub(/^Exec=/, "", value)
      value = desktop_unescape(value)
      if (substr(value, 1, 1) != "\"" || substr(value, length(value), 1) != "\"") exit 3
      value = substr(value, 2, length(value) - 2)
      output = ""
      for (i = 1; i <= length(value); i++) {
        current = substr(value, i, 1)
        following = substr(value, i + 1, 1)
        if (current == "\\") {
          if (following != "\\" && following != "\"" && following != "`" && following != "$") exit 4
          output = output following
          i++
        } else if (current == "%") {
          if (following != "%") exit 5
          output = output "%"
          i++
        } else {
          output = output current
        }
      }
      print output
    }
  '
}

test_linux_desktop_integration_helpers_defined() {
  missing=0
  assert_command_exists resolve_linux_integration_user "linux integration user resolver" || missing=1
  assert_command_exists desktop_exec_quote "desktop Exec quoting helper" || missing=1
  assert_command_exists atomic_install_file "atomic integration file helper" || missing=1
  assert_command_exists install_linux_desktop_integration "linux desktop integration helper" || missing=1
  [ "$missing" -eq 0 ]
}

test_linux_desktop_resolution_helpers_defined() {
  missing=0
  assert_command_exists resolve_linux_desktop_dir "linux Desktop resolver" || missing=1
  assert_command_exists legacy_linux_autostart_is_owned "legacy autostart ownership helper" || missing=1
  [ "$missing" -eq 0 ]
}

test_linux_desktop_integration_writes_xdg_entries_headless() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  data_dir="$temp_dir/xdg data"
  config_dir="$temp_dir/xdg config"
  launch_log="$temp_dir/launch.log"
  mkdir -p "$home_dir" "$bin_dir"
  write_linux_integration_fixture "$extract_dir"
  cat > "$bin_dir/wakezilla-tray" <<SH
#!/usr/bin/env sh
printf 'unexpected launch\n' > '$launch_log'
SH
  chmod 755 "$bin_dir/wakezilla-tray"

  output=$(
    HOME="$home_dir" \
    XDG_DATA_HOME="$data_dir" \
    XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 \
    DISPLAY= \
    WAYLAND_DISPLAY= \
      install_linux_desktop_integration "$extract_dir" "$bin_dir" 2>&1
  )

  app_entry="$data_dir/applications/dev.wakezilla.Wakezilla.desktop"
  autostart_entry="$config_dir/autostart/dev.wakezilla.tray.desktop"
  if [ ! -f "$app_entry" ]; then
    fail "linux headless integration: expected application entry"
  fi
  if [ ! -f "$autostart_entry" ]; then
    fail "linux headless integration: expected autostart entry"
  fi
  app_contents=$(cat "$app_entry" 2>/dev/null || true)
  autostart_contents=$(cat "$autostart_entry" 2>/dev/null || true)
  expected_exec="Exec=\"$bin_dir/wakezilla-tray\""
  expected_try_exec="TryExec=$bin_dir/wakezilla-tray"
  for launcher_kind in application autostart; do
    case "$launcher_kind" in
      application) launcher_contents=$app_contents ;;
      autostart) launcher_contents=$autostart_contents ;;
    esac
    assert_contains "$launcher_contents" "Type=Application" "linux $launcher_kind type"
    assert_not_contains "$launcher_contents" "Version=" "linux $launcher_kind omits stale desktop spec version"
    assert_contains "$launcher_contents" "Name=Wakezilla" "linux $launcher_kind name"
    assert_contains "$launcher_contents" "Comment=" "linux $launcher_kind comment"
    assert_eq "$expected_exec" "$(printf '%s\n' "$launcher_contents" | awk '/^Exec=/ { print; exit }')" "linux $launcher_kind exact direct helper Exec"
    assert_eq "$expected_try_exec" "$(printf '%s\n' "$launcher_contents" | awk '/^TryExec=/ { print; exit }')" "linux $launcher_kind exact TryExec"
    assert_contains "$launcher_contents" "Terminal=false" "linux $launcher_kind terminal"
    assert_contains "$launcher_contents" "StartupNotify=false" "linux $launcher_kind startup notification"
    assert_contains "$launcher_contents" "Icon=dev.wakezilla.Wakezilla" "linux $launcher_kind icon"
    assert_contains "$launcher_contents" "Categories=Network;Utility;" "linux $launcher_kind categories"
  done
  assert_not_contains "$app_contents$autostart_contents" "wakezilla tray" "linux launchers avoid CLI tray subcommand"
  assert_contains "$output" "next graphical login" "linux headless next-login message"
  if [ -e "$home_dir/Desktop" ]; then
    fail "linux headless integration: nonexistent Desktop was created"
  fi
  if [ ! -f "$autostart_entry" ]; then
    fail "linux headless integration: autostart was not retained"
  fi
  if [ -e "$launch_log" ]; then
    fail "linux headless integration: helper launched without a graphical session"
  fi

  rm -rf "$temp_dir"
}

test_desktop_exec_quote_escapes_reserved_characters() {
  exec_path='/tmp/wakezilla path/back\slash"quote`tick$dollar%percent/wakezilla-tray'
  expected='"/tmp/wakezilla path/back\\\\slash\\"quote\\`tick\\$dollar%%percent/wakezilla-tray"'
  actual=$(desktop_exec_quote "$exec_path")
  assert_eq "$expected" "$actual" "desktop Exec reserved-character escaping"
  decoded=$(printf 'Exec=%s\n' "$actual" | reference_decode_desktop_exec || true)
  assert_eq "$exec_path" "$decoded" "desktop Exec independent reference decoding"
}

test_desktop_exec_gio_launches_hostile_path_without_arguments() {
  command -v gio >/dev/null 2>&1 || return 0
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir=$temp_dir/'bin space\slash"quote`touch wakezilla-backtick-pwned`dollar$(touch wakezilla-dollar-pwned)'
  data_dir="$temp_dir/data"
  config_dir="$temp_dir/config"
  sentinel="$temp_dir/launched"
  argv_log="$temp_dir/argv-count"
  executable_log="$temp_dir/executable"
  mkdir -p "$home_dir" "$bin_dir"
  write_linux_integration_fixture "$extract_dir"
  cat > "$bin_dir/wakezilla-tray" <<SH
#!/usr/bin/env sh
printf '%s\n' "\$#" > '$argv_log'
printf '%s\n' "\$0" > '$executable_log'
: > '$sentinel'
SH
  chmod 755 "$bin_dir/wakezilla-tray"

  HOME="$home_dir" XDG_DATA_HOME="$data_dir" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  app_entry="$data_dir/applications/dev.wakezilla.Wakezilla.desktop"
  generated_exec=$(awk '/^Exec=/ { print; exit }' "$app_entry")
  decoded_exec=$(printf '%s\n' "$generated_exec" | reference_decode_desktop_exec || true)
  assert_eq "$bin_dir/wakezilla-tray" "$decoded_exec" "generated desktop Exec independent reference decoding"
  gio_output="$temp_dir/gio-output"
  set +e
  (
    cd "$temp_dir"
    HOME="$home_dir" XDG_DATA_HOME="$data_dir" XDG_CONFIG_HOME="$config_dir" \
      gio launch "$app_entry" >"$gio_output" 2>&1
  )
  gio_status=$?
  set -e
  if [ "$gio_status" -ne 0 ] && grep -qi 'not.*supported' "$gio_output"; then
    rm -rf "$temp_dir"
    return 0
  fi
  if [ "$gio_status" -ne 0 ]; then
    gio_message=$(tr '\n' ' ' < "$gio_output" 2>/dev/null || true)
    if command -v desktop-file-validate >/dev/null 2>&1; then
      desktop_validation=$(desktop-file-validate "$app_entry" 2>&1 | tr '\n' ' ' || true)
    else
      desktop_validation="desktop-file-validate unavailable"
    fi
    generated_commands=$(awk '/^(TryExec|Exec)=/ { print }' "$app_entry" | tr '\n' ' ')
    fail "desktop Exec gio launch: gio rejected the generated desktop entry: $gio_message; $desktop_validation; $generated_commands"
  fi
  gio_wait=0
  while [ ! -e "$sentinel" ] && [ "$gio_wait" -lt 100 ]; do
    sleep 0.01
    gio_wait=$((gio_wait + 1))
  done
  if [ ! -e "$sentinel" ]; then
    fail "desktop Exec gio launch: helper did not run"
  fi
  assert_eq "0" "$(cat "$argv_log" 2>/dev/null || true)" "desktop Exec gio argv count"
  assert_eq "$bin_dir/wakezilla-tray" "$(cat "$executable_log" 2>/dev/null || true)" "desktop Exec gio executable"
  if [ -e "$temp_dir/wakezilla-backtick-pwned" ] || [ -e "$temp_dir/wakezilla-dollar-pwned" ]; then
    fail "desktop Exec gio launch: command substitution sentinel was created"
  fi
  rm -rf "$temp_dir"
}

test_desktop_exec_quote_rejects_line_breaks() {
  cr=$(printf '\r')
  if desktop_exec_quote "/tmp/wakezilla${cr}tray" >/dev/null 2>&1; then
    fail "desktop Exec CR: expected rejection"
  fi
  if desktop_exec_quote '/tmp/wakezilla
tray' >/dev/null 2>&1; then
    fail "desktop Exec LF: expected rejection"
  fi
}

test_desktop_entry_encoders_reject_ascii_controls() {
  tab=$(printf '\t')
  escape=$(printf '\033')
  delete=$(printf '\177')
  for control in "$tab" "$escape" "$delete"; do
    if desktop_exec_quote "/tmp/wakezilla${control}tray" >/dev/null 2>&1; then
      fail "desktop Exec ASCII control: expected rejection"
    fi
    if desktop_string_escape "/tmp/wakezilla${control}tray" >/dev/null 2>&1; then
      fail "desktop string ASCII control: expected rejection"
    fi
  done
}

test_linux_desktop_integration_launches_graphical_helper_once() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  launch_log="$temp_dir/launch.log"
  nohup_log="$temp_dir/nohup.log"
  mkdir -p "$home_dir" "$bin_dir" "$temp_dir/stub-bin"
  write_linux_integration_fixture "$extract_dir"
  cat > "$extract_dir/wakezilla-tray" <<'SH'
#!/usr/bin/env sh
printf '%s|%s\n' "$0" "$#" >> "$WAKEZILLA_LAUNCH_LOG"
SH
  chmod 755 "$extract_dir/wakezilla-tray"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  cat > "$temp_dir/stub-bin/nohup" <<'SH'
#!/usr/bin/env sh
printf '%s\n' "$@" >> "$WAKEZILLA_NOHUP_LOG"
trap '' HUP
exec "$@"
SH
  chmod 755 "$temp_dir/stub-bin/nohup"

  output=$(
    HOME="$home_dir" \
    WAKEZILLA_EUID=1000 \
    DISPLAY=:99 \
    WAYLAND_DISPLAY= \
    WAKEZILLA_LAUNCH_LOG="$launch_log" \
    WAKEZILLA_NOHUP_LOG="$nohup_log" \
    PATH="$temp_dir/stub-bin:$PATH" \
      install_linux_desktop_integration "$extract_dir" "$bin_dir" 2>&1
  )

  launch_wait=0
  while [ ! -f "$launch_log" ] && [ "$launch_wait" -lt 500 ]; do
    sleep 0.01
    launch_wait=$((launch_wait + 1))
  done
  if [ -f "$launch_log" ]; then
    launch_lines=$(wc -l < "$launch_log")
    launch_record=$(cat "$launch_log")
  else
    launch_lines=0
    launch_record=
  fi
  assert_eq "1" "$(printf '%s' "$launch_lines" | tr -d ' ')" "graphical helper launch count"
  assert_eq "$bin_dir/wakezilla-tray|0" "$launch_record" "graphical direct helper invocation"
  assert_eq "$bin_dir/wakezilla-tray" "$(cat "$nohup_log" 2>/dev/null || true)" "graphical nohup direct helper"
  assert_not_contains "$output" "next graphical login" "graphical install omits next-login message"
  assert_contains "$output" "launch requested" "graphical install launch-request message"

  rm -rf "$temp_dir"
}

test_resolve_linux_desktop_dir_prefers_xdg_user_dir() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  desktop_dir="$temp_dir/XDG Desktop"
  mkdir -p "$home_dir" "$desktop_dir" "$temp_dir/bin"
  cat > "$temp_dir/bin/xdg-user-dir" <<SH
#!/usr/bin/env sh
[ "\${1:-}" = "DESKTOP" ] || exit 1
printf '%s\n' '$desktop_dir'
SH
  chmod 755 "$temp_dir/bin/xdg-user-dir"

  resolved=$(HOME="$home_dir" PATH="$temp_dir/bin:$PATH" resolve_linux_desktop_dir "$home_dir" "$home_dir/.config")
  assert_eq "$desktop_dir" "$resolved" "xdg-user-dir Desktop resolution"
  rm -rf "$temp_dir"
}

test_resolve_linux_desktop_dir_parses_without_execution() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  config_dir="$temp_dir/config"
  desktop_dir="$home_dir/My Desktop"
  mkdir -p "$desktop_dir" "$config_dir" "$temp_dir/empty-bin"
  printf '%s\n' 'XDG_DESKTOP_DIR="$HOME/My Desktop"' > "$config_dir/user-dirs.dirs"

  resolved=$(HOME="$home_dir" PATH="$temp_dir/empty-bin" resolve_linux_desktop_dir "$home_dir" "$config_dir")
  assert_eq "$desktop_dir" "$resolved" "user-dirs.dirs HOME Desktop resolution"

  mkdir -p "$home_dir/Desktop"
  printf '%s\n' 'XDG_DESKTOP_DIR="$HOME/Desktop$(touch wakezilla-user-dirs-pwned)"' > "$config_dir/user-dirs.dirs"
  resolved=$(
    cd "$temp_dir"
    HOME="$home_dir" PATH="$temp_dir/empty-bin" resolve_linux_desktop_dir "$home_dir" "$config_dir"
  )
  assert_eq "$home_dir/Desktop" "$resolved" "malformed user-dirs fallback"
  if [ -e "$temp_dir/wakezilla-user-dirs-pwned" ]; then
    fail "user-dirs parser: command substitution was executed"
  fi
  rm -rf "$temp_dir"
}

test_resolve_linux_desktop_dir_treats_home_as_disabled() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  config_dir="$temp_dir/config"
  mkdir -p "$home_dir/Desktop" "$config_dir" "$temp_dir/xdg-bin" "$temp_dir/empty-bin"
  cat > "$temp_dir/xdg-bin/xdg-user-dir" <<SH
#!/usr/bin/env sh
printf '%s\n' '$home_dir'
SH
  chmod 755 "$temp_dir/xdg-bin/xdg-user-dir"
  resolved=$(HOME="$home_dir" PATH="$temp_dir/xdg-bin:$PATH" \
    resolve_linux_desktop_dir "$home_dir" "$config_dir")
  assert_eq "" "$resolved" "xdg-user-dir HOME disables Desktop shortcut"

  printf '%s\n' 'XDG_DESKTOP_DIR="$HOME"' > "$config_dir/user-dirs.dirs"
  resolved=$(HOME="$home_dir" PATH="$temp_dir/empty-bin" \
    resolve_linux_desktop_dir "$home_dir" "$config_dir")
  assert_eq "" "$resolved" "user-dirs HOME disables Desktop shortcut"
  rm -rf "$temp_dir"
}

test_linux_desktop_copy_and_gio_trust_are_best_effort() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  desktop_dir="$home_dir/Desktop"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  gio_log="$temp_dir/gio.log"
  mkdir -p "$desktop_dir" "$bin_dir" "$temp_dir/stub-bin"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  cat > "$temp_dir/stub-bin/gio" <<'SH'
#!/usr/bin/env sh
printf '%s\n' "$@" >> "$WAKEZILLA_GIO_LOG"
exit 1
SH
  chmod 755 "$temp_dir/stub-bin/gio"

  output=$(
    HOME="$home_dir" \
    XDG_DATA_HOME="$temp_dir/data" \
    XDG_CONFIG_HOME="$temp_dir/config" \
    WAKEZILLA_EUID=1000 \
    WAKEZILLA_GIO_LOG="$gio_log" \
    DISPLAY= \
    PATH="$temp_dir/stub-bin:$PATH" \
      install_linux_desktop_integration "$extract_dir" "$bin_dir" 2>&1
  )

  desktop_entry="$desktop_dir/dev.wakezilla.Wakezilla.desktop"
  app_entry="$temp_dir/data/applications/dev.wakezilla.Wakezilla.desktop"
  if [ ! -f "$desktop_entry" ]; then
    fail "Linux Desktop copy: expected canonical application ID"
  elif ! cmp -s "$app_entry" "$desktop_entry"; then
    fail "Linux Desktop copy: expected application entry bytes"
  fi
  if [ -f "$desktop_entry" ]; then
    assert_eq "755" "$(file_mode "$desktop_entry")" "Linux Desktop copy mode"
  fi
  gio_args=$(tr '\n' ' ' < "$gio_log" 2>/dev/null || true)
  assert_contains "$gio_args" "set $desktop_entry metadata::trusted true" "Linux Desktop gio trust metadata"
  assert_contains "$output" "next graphical login" "gio failure remains successful"
  rm -rf "$temp_dir"
}

test_linux_integration_relative_xdg_falls_back_and_copies_icons() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  mkdir -p "$home_dir" "$bin_dir"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"

  HOME="$home_dir" \
  XDG_DATA_HOME=relative/data \
  XDG_CONFIG_HOME=relative/config \
  WAKEZILLA_EUID=1000 \
  DISPLAY= \
  WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1

  if [ ! -f "$home_dir/.local/share/applications/dev.wakezilla.Wakezilla.desktop" ]; then
    fail "relative XDG data: expected HOME fallback"
  fi
  if [ ! -f "$home_dir/.config/autostart/dev.wakezilla.tray.desktop" ]; then
    fail "relative XDG config: expected HOME fallback"
  fi
  for size in 48 128 256; do
    source_icon="$extract_dir/icons/hicolor/${size}x${size}/apps/dev.wakezilla.Wakezilla.png"
    installed_icon="$home_dir/.local/share/icons/hicolor/${size}x${size}/apps/dev.wakezilla.Wakezilla.png"
    if ! cmp -s "$source_icon" "$installed_icon"; then
      fail "Linux ${size}x${size} icon: expected byte-identical copy"
    fi
  done
  rm -rf "$temp_dir"
}

test_linux_integration_removes_only_owned_legacy_autostart() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  config_dir="$temp_dir/config"
  legacy_entry="$config_dir/autostart/wakezilla-tray.desktop"
  mkdir -p "$home_dir" "$bin_dir" "$config_dir/autostart"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  cat > "$legacy_entry" <<'EOF'
[Desktop Entry]
Type=Application
Name=Wakezilla Tray
Exec=/old/bin/wakezilla-tray
EOF

  HOME="$home_dir" XDG_DATA_HOME="$temp_dir/data" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  if [ -e "$legacy_entry" ]; then
    fail "owned legacy Linux autostart: expected removal"
  fi

  cat > "$legacy_entry" <<'EOF'
[Desktop Entry]
Type=Application
Name=Wakezilla Tray
Exec="/old/bin/wakezilla" tray
Terminal=false
EOF
  HOME="$home_dir" XDG_DATA_HOME="$temp_dir/data" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  if [ -e "$legacy_entry" ]; then
    fail "owned legacy CLI tray autostart: expected removal"
  fi

  printf '%s\n' \
    '[Other Group]' 'Name=Wakezilla Tray' 'Exec=/old/bin/wakezilla-tray' \
    '[Desktop Entry]' 'Type=Application' 'Name=Another App' 'Exec=/other/app' > "$legacy_entry"
  HOME="$home_dir" XDG_DATA_HOME="$temp_dir/data" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  if [ ! -f "$legacy_entry" ]; then
    fail "foreign legacy-named autostart: expected preservation"
  fi

  cat > "$legacy_entry" <<'EOF'
[Desktop Entry]
Type=Application
Name=Wakezilla Tray
Exec=/other/not-wakezilla-tray-helper
EOF
  HOME="$home_dir" XDG_DATA_HOME="$temp_dir/data" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  if [ ! -f "$legacy_entry" ]; then
    fail "legacy substring executable: expected preservation"
  fi

  cat > "$legacy_entry" <<'EOF'
[Desktop Entry]
Type=Application
Name=Wakezilla Tray
[Desktop Entry]
Exec=/old/bin/wakezilla-tray
EOF
  HOME="$home_dir" XDG_DATA_HOME="$temp_dir/data" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  if [ ! -f "$legacy_entry" ]; then
    fail "legacy multiple Desktop Entry groups: expected preservation"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_root_treats_home_desktop_as_disabled() {
  temp_dir=$(mktemp -d)
  root_home="$temp_dir/root-home"
  user_home="$temp_dir/user-home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/system-bin"
  legacy_entry="$user_home/.config/autostart/wakezilla-tray.desktop"
  test_uid=$(id -u)
  test_gid=$(id -g)
  mkdir -p "$root_home" "$user_home/Desktop" "$user_home/.config/autostart" \
    "$bin_dir" "$temp_dir/stub-bin"
  printf '%s\n' 'XDG_DESKTOP_DIR="$HOME"' > "$user_home/.config/user-dirs.dirs"
  cat > "$legacy_entry" <<'EOF'
[Desktop Entry]
Type=Application
Name=Wakezilla Tray
Exec=/other/not-wakezilla-tray-helper
EOF
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  write_fake_sudo "$temp_dir/stub-bin/sudo"
  write_fake_chown "$temp_dir/stub-bin/chown"
  : > "$temp_dir/chown.log"
  : > "$temp_dir/privilege.log"

  HOME="$root_home" WAKEZILLA_EUID=0 \
    WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
    WAKEZILLA_TEST_SUDO_UID="$test_uid" WAKEZILLA_TEST_SUDO_GID="$test_gid" \
    WAKEZILLA_TEST_SUDO_HOME="$user_home" \
    WAKEZILLA_PRIVILEGE_LOG="$temp_dir/privilege.log" \
    WAKEZILLA_CHOWN_LOG="$temp_dir/chown.log" \
    DISPLAY= WAYLAND_DISPLAY= PATH="$temp_dir/stub-bin:$PATH" \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1

  if [ ! -f "$user_home/.local/share/applications/dev.wakezilla.Wakezilla.desktop" ]; then
    fail "sudo HOME Desktop disabled: expected application entry"
  fi
  if [ -e "$user_home/dev.wakezilla.Wakezilla.desktop" ] || \
     [ -e "$user_home/Desktop/dev.wakezilla.Wakezilla.desktop" ]; then
    fail "sudo HOME Desktop disabled: expected no Desktop shortcut"
  fi
  if [ ! -f "$legacy_entry" ]; then
    fail "sudo strict legacy matcher: expected foreign substring entry preserved"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_validates_all_assets_before_writes() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  data_dir="$temp_dir/data"
  config_dir="$temp_dir/config"
  mkdir -p "$home_dir" "$bin_dir" "$data_dir/applications" "$config_dir/autostart"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  rm -f "$extract_dir/icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png"
  printf 'existing-app\n' > "$data_dir/applications/dev.wakezilla.Wakezilla.desktop"
  printf 'legacy-owned\nExec=/old/wakezilla-tray\n' > "$config_dir/autostart/wakezilla-tray.desktop"

  output=$(HOME="$home_dir" XDG_DATA_HOME="$data_dir" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" 2>&1)
  assert_eq "existing-app" "$(cat "$data_dir/applications/dev.wakezilla.Wakezilla.desktop")" "prevalidation preserves application entry"
  if [ -d "$data_dir/icons" ]; then
    fail "prevalidation: expected no icon directories before complete validation"
  fi
  if [ ! -f "$config_dir/autostart/wakezilla-tray.desktop" ]; then
    fail "prevalidation: expected legacy entry untouched"
  fi
  assert_contains "$output" "skipping desktop integration" "legacy archive compatibility warning"
  rm -rf "$temp_dir"
}

test_linux_integration_is_idempotent_without_temp_siblings() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  data_dir="$temp_dir/data"
  config_dir="$temp_dir/config"
  mkdir -p "$home_dir" "$bin_dir"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"

  for reinstall in 1 2; do
    HOME="$home_dir" XDG_DATA_HOME="$data_dir" XDG_CONFIG_HOME="$config_dir" \
      WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
      install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  done
  temp_count=$(find "$data_dir" "$config_dir" -name '*.tmp.*' -print | wc -l | tr -d ' ')
  assert_eq "0" "$temp_count" "idempotent integration temporary siblings"
  assert_eq "1" "$(find "$data_dir/applications" -name 'dev.wakezilla.Wakezilla.desktop' -print | wc -l | tr -d ' ')" "idempotent application entry count"
  assert_eq "1" "$(find "$config_dir/autostart" -name 'dev.wakezilla.tray.desktop' -print | wc -l | tr -d ' ')" "idempotent autostart entry count"
  rm -rf "$temp_dir"
}

test_linux_integration_rolls_back_late_autostart_failure() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  data_dir="$temp_dir/data"
  config_dir="$temp_dir/config"
  app_entry="$data_dir/applications/dev.wakezilla.Wakezilla.desktop"
  autostart_entry="$config_dir/autostart/dev.wakezilla.tray.desktop"
  legacy_entry="$config_dir/autostart/wakezilla-tray.desktop"
  icon_48="$data_dir/icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png"
  icon_128="$data_dir/icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png"
  icon_256="$data_dir/icons/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png"
  mkdir -p "$home_dir" "$bin_dir" "${app_entry%/*}" "${autostart_entry%/*}" \
    "${icon_48%/*}"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  printf 'old application\n' > "$app_entry"
  printf 'old autostart\n' > "$autostart_entry"
  printf 'old icon\n' > "$icon_48"
  cat > "$legacy_entry" <<'EOF'
[Desktop Entry]
Type=Application
Name=Wakezilla Tray
Exec=/old/bin/wakezilla-tray
EOF
  chmod 0600 "$app_entry"
  chmod 0640 "$autostart_entry"
  chmod 0604 "$icon_48"

  set +e
  HOME="$home_dir" XDG_DATA_HOME="$data_dir" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_TEST_FAIL_LINUX_INTEGRATION_AFTER=autostart \
    DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  install_status=$?
  set -e

  if [ "$install_status" -eq 0 ]; then
    fail "late integration failure: expected nonzero status"
  fi
  assert_eq "old application" "$(cat "$app_entry")" "late rollback application bytes"
  assert_eq "old autostart" "$(cat "$autostart_entry")" "late rollback autostart bytes"
  assert_eq "old icon" "$(cat "$icon_48")" "late rollback icon bytes"
  assert_eq "600" "$(file_mode "$app_entry")" "late rollback application mode"
  assert_eq "640" "$(file_mode "$autostart_entry")" "late rollback autostart mode"
  assert_eq "604" "$(file_mode "$icon_48")" "late rollback icon mode"
  if [ ! -f "$legacy_entry" ]; then
    fail "late rollback legacy entry: expected restoration"
  fi
  if [ -e "$icon_128" ] || [ -e "$icon_256" ]; then
    fail "late rollback new icons: expected removal"
  fi
  temp_count=$(find "$data_dir" "$config_dir" -name '*.tmp.*' -print | wc -l | tr -d ' ')
  assert_eq "0" "$temp_count" "late rollback temporary siblings"
  rm -rf "$temp_dir"
}

test_linux_integration_profile_directory_modes() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  data_dir="$temp_dir/data"
  config_dir="$temp_dir/config"
  mkdir -p "$home_dir" "$bin_dir" "$data_dir" "$config_dir"
  chmod 0755 "$data_dir"
  chmod 0711 "$config_dir"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"

  HOME="$home_dir" XDG_DATA_HOME="$data_dir" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1

  assert_eq "755" "$(file_mode "$data_dir")" "pre-existing data directory mode"
  assert_eq "711" "$(file_mode "$config_dir")" "pre-existing config directory mode"
  for private_dir in \
    "$data_dir/applications" \
    "$config_dir/autostart" \
    "$data_dir/icons" \
    "$data_dir/icons/hicolor" \
    "$data_dir/icons/hicolor/48x48" \
    "$data_dir/icons/hicolor/48x48/apps"; do
    assert_eq "700" "$(file_mode "$private_dir")" "new profile directory mode $private_dir"
  done
  rm -rf "$temp_dir"
}

test_linux_integration_reports_incomplete_rollback_and_continues() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  data_dir="$temp_dir/data"
  config_dir="$temp_dir/config"
  app_entry="$data_dir/applications/dev.wakezilla.Wakezilla.desktop"
  icon_entry="$data_dir/icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png"
  mkdir -p "$home_dir" "$bin_dir" "${app_entry%/*}" "${icon_entry%/*}"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  printf 'old application\n' > "$app_entry"
  printf 'old icon\n' > "$icon_entry"
  output_file="$temp_dir/output"

  set +e
  HOME="$home_dir" XDG_DATA_HOME="$data_dir" XDG_CONFIG_HOME="$config_dir" \
    WAKEZILLA_EUID=1000 WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_TEST_FAIL_LINUX_INTEGRATION_AFTER=autostart \
    WAKEZILLA_TEST_FAIL_LINUX_ROLLBACK=application \
    DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" \
      >"$output_file" 2>&1
  install_status=$?
  set -e
  output=$(cat "$output_file")

  if [ "$install_status" -eq 0 ]; then
    fail "incomplete profile rollback: expected nonzero status"
  fi
  assert_contains "$output" "rollback incomplete" \
    "incomplete profile rollback warning"
  assert_not_contains "$(cat "$app_entry")" "old application" \
    "injected application restore failure remains visible"
  assert_eq "old icon" "$(cat "$icon_entry")" \
    "incomplete profile rollback continues restoring icons"
  rm -rf "$temp_dir"
}

test_linux_integration_root_helper_rolls_back_late_failure() {
  temp_dir=$(mktemp -d)
  root_home="$temp_dir/root-home"
  user_home="$temp_dir/user-home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/system-bin"
  data_dir="$user_home/.local/share"
  config_dir="$user_home/.config"
  app_entry="$data_dir/applications/dev.wakezilla.Wakezilla.desktop"
  autostart_entry="$config_dir/autostart/dev.wakezilla.tray.desktop"
  legacy_entry="$config_dir/autostart/wakezilla-tray.desktop"
  icon_48="$data_dir/icons/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png"
  icon_128="$data_dir/icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png"
  test_uid=$(id -u)
  test_gid=$(id -g)
  mkdir -p "$root_home" "$user_home" "$bin_dir" "$temp_dir/stub-bin" \
    "${app_entry%/*}" "${autostart_entry%/*}" "${icon_48%/*}"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  printf 'root old application\n' > "$app_entry"
  printf 'root old autostart\n' > "$autostart_entry"
  printf 'root old icon\n' > "$icon_48"
  cat > "$legacy_entry" <<'EOF'
[Desktop Entry]
Type=Application
Name=Wakezilla Tray
Exec=/old/bin/wakezilla-tray
EOF
  chmod 0600 "$app_entry"
  chmod 0640 "$autostart_entry"
  chmod 0604 "$icon_48"
  write_fake_sudo "$temp_dir/stub-bin/sudo"
  write_fake_chown "$temp_dir/stub-bin/chown"
  : > "$temp_dir/chown.log"
  : > "$temp_dir/privilege.log"

  set +e
  HOME="$root_home" WAKEZILLA_EUID=0 WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
    WAKEZILLA_TEST_SUDO_UID="$test_uid" WAKEZILLA_TEST_SUDO_GID="$test_gid" \
    WAKEZILLA_TEST_SUDO_HOME="$user_home" \
    WAKEZILLA_TEST_FAIL_LINUX_INTEGRATION_AFTER=autostart \
    WAKEZILLA_PRIVILEGE_LOG="$temp_dir/privilege.log" \
    WAKEZILLA_CHOWN_LOG="$temp_dir/chown.log" \
    DISPLAY= WAYLAND_DISPLAY= PATH="$temp_dir/stub-bin:$PATH" \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  install_status=$?
  set -e

  if [ "$install_status" -eq 0 ]; then
    fail "sudo late integration failure: expected nonzero status"
  fi
  assert_eq "root old application" "$(cat "$app_entry")" "sudo late rollback application bytes"
  assert_eq "root old autostart" "$(cat "$autostart_entry")" "sudo late rollback autostart bytes"
  assert_eq "root old icon" "$(cat "$icon_48")" "sudo late rollback icon bytes"
  assert_eq "600" "$(file_mode "$app_entry")" "sudo late rollback application mode"
  assert_eq "640" "$(file_mode "$autostart_entry")" "sudo late rollback autostart mode"
  assert_eq "604" "$(file_mode "$icon_48")" "sudo late rollback icon mode"
  if [ ! -f "$legacy_entry" ]; then
    fail "sudo late rollback legacy entry: expected restoration"
  fi
  if [ -e "$icon_128" ]; then
    fail "sudo late rollback new icon: expected removal"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_root_without_valid_sudo_user_skips_everything() {
  temp_dir=$(mktemp -d)
  root_home="$temp_dir/root-home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  launch_log="$temp_dir/launch.log"
  mkdir -p "$root_home/Desktop" "$bin_dir"
  write_linux_integration_fixture "$extract_dir"
  cat > "$bin_dir/wakezilla-tray" <<'SH'
#!/usr/bin/env sh
printf 'launched\n' >> "$WAKEZILLA_LAUNCH_LOG"
SH
  chmod 755 "$bin_dir/wakezilla-tray"

  output=$(
    cd "$temp_dir"
    HOME="$root_home" \
    XDG_DATA_HOME="$root_home/data" \
    XDG_CONFIG_HOME="$root_home/config" \
    WAKEZILLA_EUID=0 \
    WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    SUDO_USER='bad$(touch root-user-pwned)' \
    SUDO_UID=0 \
    SUDO_GID=0 \
    DISPLAY=:99 \
    WAKEZILLA_LAUNCH_LOG="$launch_log" \
      install_linux_desktop_integration "$extract_dir" "$bin_dir" 2>&1
  )

  assert_contains "$output" "skipping Linux desktop integration" "root direct integration warning"
  if [ -d "$root_home/data" ] || [ -d "$root_home/config" ]; then
    fail "root direct integration: expected no profile writes"
  fi
  if [ -e "$launch_log" ]; then
    fail "root direct integration: expected no graphical launch"
  fi
  if [ -e "$temp_dir/root-user-pwned" ]; then
    fail "root sudo user validation: command substitution was executed"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_euid_override_is_test_mode_only() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin" "$temp_dir/home"
  cat > "$temp_dir/bin/id" <<'SH'
#!/usr/bin/env sh
case "${1:-}" in
  -u) printf '0\n' ;;
  -g) printf '0\n' ;;
  *) exit 1 ;;
esac
SH
  chmod 755 "$temp_dir/bin/id"

  if output=$(HOME="$temp_dir/home" WAKEZILLA_INSTALL_SH_TEST_MODE= \
    WAKEZILLA_EUID=1000 SUDO_USER= SUDO_UID= SUDO_GID= \
    PATH="$temp_dir/bin:$PATH" resolve_linux_integration_user 2>&1); then
    fail "production EUID resolver: honored WAKEZILLA_EUID override as root"
  else
    assert_contains "$output" "skipping Linux desktop integration" "production EUID override warning"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_valid_sudo_user_applies_as_target_user() {
  temp_dir=$(mktemp -d)
  root_home="$temp_dir/root-home"
  user_home="$temp_dir/user-home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/system-bin"
  chown_log="$temp_dir/chown.log"
  privilege_log="$temp_dir/privilege.log"
  launch_log="$temp_dir/launch.log"
  test_uid=$(id -u)
  test_gid=$(id -g)
  mkdir -p "$root_home" "$user_home/Desktop" "$bin_dir" "$temp_dir/stub-bin"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  cat > "$temp_dir/stub-bin/chown" <<'SH'
#!/usr/bin/env sh
printf '%s\n' "$@" >> "$WAKEZILLA_CHOWN_LOG"
exit 0
SH
  chmod 755 "$temp_dir/stub-bin/chown"
  write_fake_sudo "$temp_dir/stub-bin/sudo"
  : > "$privilege_log"

  output=$(
    HOME="$root_home" \
    XDG_DATA_HOME="$root_home/xdg-data-must-not-be-used" \
    XDG_CONFIG_HOME="$root_home/xdg-config-must-not-be-used" \
    WAKEZILLA_EUID=0 \
    WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
    WAKEZILLA_TEST_SUDO_UID="$test_uid" \
    WAKEZILLA_TEST_SUDO_GID="$test_gid" \
    WAKEZILLA_TEST_SUDO_HOME="$user_home" \
    WAKEZILLA_CHOWN_LOG="$chown_log" \
    WAKEZILLA_PRIVILEGE_LOG="$privilege_log" \
    WAKEZILLA_LAUNCH_LOG="$launch_log" \
    DISPLAY=:99 \
    PATH="$temp_dir/stub-bin:$PATH" \
      install_linux_desktop_integration "$extract_dir" "$bin_dir" 2>&1
  )

  if [ ! -f "$user_home/.local/share/applications/dev.wakezilla.Wakezilla.desktop" ]; then
    fail "sudo integration: expected target user's application entry"
  fi
  if [ ! -f "$user_home/.config/autostart/dev.wakezilla.tray.desktop" ]; then
    fail "sudo integration: expected target user's autostart entry"
  fi
  if [ -d "$root_home/xdg-data-must-not-be-used" ] || [ -d "$root_home/xdg-config-must-not-be-used" ]; then
    fail "sudo integration: wrote to root profile XDG paths"
  fi
  if [ -e "$launch_log" ]; then
    fail "sudo integration: must not launch the tray as root"
  fi
  chown_args=$(cat "$chown_log" 2>/dev/null || true)
  assert_contains "$chown_args" "$test_uid:$test_gid" "sudo integration chown owner"
  assert_not_contains "$chown_args" "$user_home" "sudo integration never chowns target home or profile"
  privilege_args=$(tr '\n' ' ' < "$privilege_log" 2>/dev/null || true)
  assert_contains "$privilege_args" "-u wakezilla-test-user --" "sudo integration drops to target user"
  assert_not_contains "$privilege_args" "-u root" "sudo integration never selects root"
  assert_contains "$output" "next graphical login" "sudo integration next-login message"
  rm -rf "$temp_dir"
}

test_linux_integration_privilege_failures_are_fatal() {
  temp_dir=$(mktemp -d)
  user_home="$temp_dir/user-home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  partial_marker="$temp_dir/partial-attempt"
  privilege_log="$temp_dir/privilege.log"
  test_uid=$(id -u)
  test_gid=$(id -g)
  mkdir -p "$user_home" "$bin_dir" "$temp_dir/stub-bin"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  write_fake_chown "$temp_dir/stub-bin/chown"

  for failure_mode in exit_125 no_runner; do
    rm -rf "$user_home/.local" "$user_home/.config"
    : > "$privilege_log"
    rm -f "$partial_marker"
    case "$failure_mode" in
      exit_125)
        cat > "$temp_dir/stub-bin/sudo" <<'SH'
#!/usr/bin/env sh
: > "$WAKEZILLA_PARTIAL_MARKER"
exit 125
SH
        no_runner=
        ;;
      no_runner)
        cat > "$temp_dir/stub-bin/sudo" <<'SH'
#!/usr/bin/env sh
printf 'runner should not be invoked\n' >> "$WAKEZILLA_PRIVILEGE_LOG"
exit 0
SH
        no_runner=1
        ;;
    esac
    chmod 755 "$temp_dir/stub-bin/sudo"

    set +e
    output=$(HOME=/root WAKEZILLA_EUID=0 WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
      WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
      WAKEZILLA_TEST_SUDO_UID="$test_uid" WAKEZILLA_TEST_SUDO_GID="$test_gid" \
      WAKEZILLA_TEST_SUDO_HOME="$user_home" \
      WAKEZILLA_TEST_NO_PRIVILEGE_RUNNER="$no_runner" \
      WAKEZILLA_PARTIAL_MARKER="$partial_marker" \
      WAKEZILLA_PRIVILEGE_LOG="$privilege_log" WAKEZILLA_CHOWN_LOG="$temp_dir/chown.log" \
      DISPLAY= WAYLAND_DISPLAY= PATH="$temp_dir/stub-bin:$PATH" \
      install_linux_desktop_integration "$extract_dir" "$bin_dir" 2>&1)
    install_status=$?
    set -e

    if [ "$install_status" -eq 0 ]; then
      fail "sudo $failure_mode failure: expected nonzero integration status"
    fi
    assert_not_contains "$output" "Linux desktop integration installed" "sudo $failure_mode no success message"
    if [ "$failure_mode" = "exit_125" ] && [ ! -e "$partial_marker" ]; then
      fail "sudo exit 125 failure: expected partial-attempt marker"
    fi
    if [ "$failure_mode" = "no_runner" ]; then
      assert_contains "$output" "sudo or runuser is unavailable" "sudo missing runner warning"
      assert_eq "" "$(cat "$privilege_log")" "sudo missing runner is not invoked"
    fi
  done
  rm -rf "$temp_dir"
}

test_resolve_linux_integration_user_rejects_unsafe_sudo_homes() {
  temp_dir=$(mktemp -d)
  safe_home="$temp_dir/safe-home"
  outside_home="$temp_dir/outside-home"
  missing_home="$temp_dir/missing-home"
  symlink_home="$temp_dir/symlink-home"
  test_uid=$(id -u)
  test_gid=$(id -g)
  mkdir -p "$safe_home" "$outside_home"
  ln -s "$outside_home" "$symlink_home"

  for unsafe_home in / /root "$symlink_home" "$missing_home"; do
    if output=$(HOME=/root WAKEZILLA_EUID=0 WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
      WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
      WAKEZILLA_TEST_SUDO_UID="$test_uid" WAKEZILLA_TEST_SUDO_GID="$test_gid" \
      WAKEZILLA_TEST_SUDO_HOME="$unsafe_home" resolve_linux_integration_user 2>&1); then
      fail "sudo home validation: expected rejection for $unsafe_home"
    else
      assert_contains "$output" "skipping Linux desktop integration" "sudo home rejection warning"
    fi
  done

  mkdir -p "$temp_dir/stat-bin"
  cat > "$temp_dir/stat-bin/stat" <<'SH'
#!/usr/bin/env sh
printf '999999\n'
SH
  chmod 755 "$temp_dir/stat-bin/stat"
  if output=$(HOME=/root WAKEZILLA_EUID=0 WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
    WAKEZILLA_TEST_SUDO_UID="$test_uid" WAKEZILLA_TEST_SUDO_GID="$test_gid" \
    WAKEZILLA_TEST_SUDO_HOME="$safe_home" PATH="$temp_dir/stat-bin:$PATH" \
    resolve_linux_integration_user 2>&1); then
    fail "sudo home validation: expected owner mismatch rejection"
  else
    assert_contains "$output" "skipping Linux desktop integration" "sudo home owner mismatch warning"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_ignores_test_sudo_overrides_outside_test_mode() {
  temp_dir=$(mktemp -d)
  root_home="$temp_dir/root-home"
  user_home="$temp_dir/user-home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  mkdir -p "$root_home" "$user_home" "$bin_dir"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"

  output=$(HOME="$root_home" WAKEZILLA_EUID=0 WAKEZILLA_INSTALL_SH_TEST_MODE= \
    WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
    WAKEZILLA_TEST_SUDO_UID="$(id -u)" WAKEZILLA_TEST_SUDO_GID="$(id -g)" \
    WAKEZILLA_TEST_SUDO_HOME="$user_home" DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" 2>&1)
  if [ -d "$user_home/.local" ] || [ -d "$user_home/.config" ]; then
    fail "production sudo resolver: honored test-only override"
  fi
  if [ ! -f "$root_home/.local/share/applications/dev.wakezilla.Wakezilla.desktop" ]; then
    fail "production sudo resolver: expected actual non-root HOME instead of test override"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_root_discards_desktop_outside_target_home() {
  temp_dir=$(mktemp -d)
  user_home="$temp_dir/user-home"
  outside_desktop="$temp_dir/outside-desktop"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  test_uid=$(id -u)
  test_gid=$(id -g)
  mkdir -p "$user_home" "$outside_desktop" "$bin_dir" "$temp_dir/stub-bin"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  cat > "$temp_dir/stub-bin/xdg-user-dir" <<SH
#!/usr/bin/env sh
printf '%s\n' '$outside_desktop'
SH
  chmod 755 "$temp_dir/stub-bin/xdg-user-dir"
  write_fake_sudo "$temp_dir/stub-bin/sudo"
  write_fake_chown "$temp_dir/stub-bin/chown"
  : > "$temp_dir/chown.log"

  HOME=/root WAKEZILLA_EUID=0 \
    WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
    WAKEZILLA_TEST_SUDO_UID="$test_uid" WAKEZILLA_TEST_SUDO_GID="$test_gid" \
    WAKEZILLA_TEST_SUDO_HOME="$user_home" WAKEZILLA_PRIVILEGE_LOG="$temp_dir/privilege.log" \
    WAKEZILLA_CHOWN_LOG="$temp_dir/chown.log" \
    DISPLAY= WAYLAND_DISPLAY= \
    PATH="$temp_dir/stub-bin:$PATH" \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  if [ -e "$outside_desktop/dev.wakezilla.Wakezilla.desktop" ]; then
    fail "sudo Desktop resolver: wrote outside target profile"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_root_rejects_profile_traversal_and_symlinks() {
  temp_dir=$(mktemp -d)
  user_home="$temp_dir/user-home"
  outside_dir="$temp_dir/outside"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  test_uid=$(id -u)
  test_gid=$(id -g)
  mkdir -p "$user_home" "$outside_dir" "$bin_dir" "$temp_dir/stub-bin"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  write_fake_sudo "$temp_dir/stub-bin/sudo"
  write_fake_chown "$temp_dir/stub-bin/chown"
  : > "$temp_dir/chown.log"

  HOME=/root WAKEZILLA_EUID=0 WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
    WAKEZILLA_TEST_SUDO_UID="$test_uid" WAKEZILLA_TEST_SUDO_GID="$test_gid" \
    WAKEZILLA_TEST_SUDO_HOME="$user_home" \
    XDG_DATA_HOME="$user_home/../outside/data" \
    XDG_CONFIG_HOME="$user_home/../outside/config" WAKEZILLA_PRIVILEGE_LOG="$temp_dir/privilege.log" \
    WAKEZILLA_CHOWN_LOG="$temp_dir/chown.log" DISPLAY= WAYLAND_DISPLAY= PATH="$temp_dir/stub-bin:$PATH" \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1
  if [ -d "$outside_dir/data" ] || [ -d "$outside_dir/config" ]; then
    fail "sudo profile traversal: wrote outside target home"
  fi
  if [ ! -f "$user_home/.local/share/applications/dev.wakezilla.Wakezilla.desktop" ]; then
    fail "sudo profile traversal: expected safe fallback data path"
  fi

  rm -rf "$user_home/.local" "$user_home/.config"
  ln -s "$outside_dir" "$user_home/.local"
  if HOME=/root WAKEZILLA_EUID=0 WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
    WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
    WAKEZILLA_TEST_SUDO_UID="$test_uid" WAKEZILLA_TEST_SUDO_GID="$test_gid" \
    WAKEZILLA_TEST_SUDO_HOME="$user_home" \
    XDG_DATA_HOME= XDG_CONFIG_HOME= WAKEZILLA_PRIVILEGE_LOG="$temp_dir/privilege.log" \
    WAKEZILLA_CHOWN_LOG="$temp_dir/chown.log" DISPLAY= WAYLAND_DISPLAY= PATH="$temp_dir/stub-bin:$PATH" \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1; then
    fail "sudo profile symlink: expected integration failure"
  fi
  if [ -d "$outside_dir/share" ]; then
    fail "sudo profile symlink: wrote outside target home"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_root_dropped_apply_rejects_intermediate_symlinks() {
  temp_dir=$(mktemp -d)
  user_home="$temp_dir/user-home"
  outside_dir="$temp_dir/outside"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  chown_log="$temp_dir/chown.log"
  privilege_log="$temp_dir/privilege.log"
  test_uid=$(id -u)
  test_gid=$(id -g)
  mkdir -p "$user_home" "$outside_dir" "$bin_dir" "$temp_dir/stub-bin"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  write_fake_sudo "$temp_dir/stub-bin/sudo"
  cat > "$temp_dir/stub-bin/chown" <<'SH'
#!/usr/bin/env sh
printf '%s\n' "$@" >> "$WAKEZILLA_CHOWN_LOG"
exit 0
SH
  chmod 755 "$temp_dir/stub-bin/chown"

  for symlink_case in applications icons hicolor autostart; do
    rm -rf "$user_home/.local" "$user_home/.config" "$outside_dir"
    mkdir -p "$outside_dir"
    printf 'outside-sentinel\n' > "$outside_dir/sentinel"
    case "$symlink_case" in
      applications)
        mkdir -p "$user_home/.local/share"
        ln -s "$outside_dir" "$user_home/.local/share/applications"
        ;;
      icons)
        mkdir -p "$user_home/.local/share"
        ln -s "$outside_dir" "$user_home/.local/share/icons"
        ;;
      hicolor)
        mkdir -p "$user_home/.local/share/icons"
        ln -s "$outside_dir" "$user_home/.local/share/icons/hicolor"
        ;;
      autostart)
        mkdir -p "$user_home/.config"
        ln -s "$outside_dir" "$user_home/.config/autostart"
        ;;
    esac
    : > "$chown_log"
    : > "$privilege_log"
    if HOME=/root WAKEZILLA_EUID=0 WAKEZILLA_INSTALL_SH_TEST_MODE=1 \
      WAKEZILLA_TEST_SUDO_USER=wakezilla-test-user \
      WAKEZILLA_TEST_SUDO_UID="$test_uid" WAKEZILLA_TEST_SUDO_GID="$test_gid" \
      WAKEZILLA_TEST_SUDO_HOME="$user_home" WAKEZILLA_CHOWN_LOG="$chown_log" \
      WAKEZILLA_PRIVILEGE_LOG="$privilege_log" DISPLAY= WAYLAND_DISPLAY= \
      PATH="$temp_dir/stub-bin:$PATH" \
      install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1; then
      fail "sudo dropped apply $symlink_case symlink: expected failure"
    fi
    outside_count=$(find "$outside_dir" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')
    assert_eq "1" "$outside_count" "sudo dropped apply $symlink_case symlink outside writes"
    assert_not_contains "$(cat "$chown_log")" "$user_home" "sudo dropped apply $symlink_case never chowns profile"
    runner_args=$(tr '\n' ' ' < "$privilege_log")
    assert_contains "$runner_args" "-u wakezilla-test-user --" "sudo dropped apply $symlink_case target user"
  done
  rm -rf "$temp_dir"
}

test_linux_integration_post_validation_setup_failures_are_fatal() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  mkdir -p "$home_dir" "$bin_dir" "$temp_dir/mktemp-bin" "$temp_dir/mkdir-bin"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  cat > "$temp_dir/mktemp-bin/mktemp" <<'SH'
#!/usr/bin/env sh
exit 1
SH
  chmod 755 "$temp_dir/mktemp-bin/mktemp"
  if HOME="$home_dir" XDG_DATA_HOME="$temp_dir/data" XDG_CONFIG_HOME="$temp_dir/config" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= PATH="$temp_dir/mktemp-bin:$PATH" \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1; then
    fail "post-validation mktemp failure: expected nonzero"
  fi

  real_mkdir=$(command -v mkdir)
  cat > "$temp_dir/mkdir-bin/mkdir" <<SH
#!/usr/bin/env sh
case "\$*" in
  *'$temp_dir/data'*) exit 1 ;;
esac
exec '$real_mkdir' "\$@"
SH
  chmod 755 "$temp_dir/mkdir-bin/mkdir"
  if HOME="$home_dir" XDG_DATA_HOME="$temp_dir/data" XDG_CONFIG_HOME="$temp_dir/config" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= PATH="$temp_dir/mkdir-bin:$PATH" \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1; then
    fail "post-validation mkdir failure: expected nonzero"
  fi
  rm -rf "$temp_dir"
}

test_linux_integration_rejects_symlinked_archive_assets() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  data_dir="$temp_dir/data"
  mkdir -p "$home_dir" "$bin_dir"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"
  icon_path="$extract_dir/icons/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png"
  printf 'outside-icon\n' > "$temp_dir/outside-icon"
  rm -f "$icon_path"
  ln -s "$temp_dir/outside-icon" "$icon_path"

  output=$(HOME="$home_dir" XDG_DATA_HOME="$data_dir" XDG_CONFIG_HOME="$temp_dir/config" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" 2>&1)
  if [ -d "$data_dir" ]; then
    fail "symlinked archive asset: expected validation before writes"
  fi
  assert_contains "$output" "skipping desktop integration" "symlinked archive compatibility warning"
  rm -rf "$temp_dir"
}

test_linux_integration_rejects_line_break_in_helper_path_before_writes() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin
with-line-break"
  data_dir="$temp_dir/data"
  mkdir -p "$home_dir" "$bin_dir"
  write_linux_integration_fixture "$extract_dir"
  cp "$extract_dir/wakezilla-tray" "$bin_dir/wakezilla-tray"

  if HOME="$home_dir" XDG_DATA_HOME="$data_dir" XDG_CONFIG_HOME="$temp_dir/config" \
    WAKEZILLA_EUID=1000 DISPLAY= WAYLAND_DISPLAY= \
    install_linux_desktop_integration "$extract_dir" "$bin_dir" >/dev/null 2>&1; then
    fail "line-break helper path: expected rejection"
  fi
  if [ -d "$data_dir" ]; then
    fail "line-break helper path: expected no integration writes"
  fi
  rm -rf "$temp_dir"
}

test_atomic_install_file_rolls_back_failed_rename() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/bin" "$temp_dir/dest"
  printf 'old\n' > "$temp_dir/dest/entry.desktop"
  printf 'new\n' > "$temp_dir/source.desktop"
  cat > "$temp_dir/bin/mv" <<'SH'
#!/usr/bin/env sh
exit 1
SH
  chmod 755 "$temp_dir/bin/mv"

  if PATH="$temp_dir/bin:$PATH" atomic_install_file \
    "$temp_dir/source.desktop" "$temp_dir/dest/entry.desktop" 0644; then
    fail "atomic integration rename failure: expected failure"
  fi
  assert_eq "old" "$(cat "$temp_dir/dest/entry.desktop")" "atomic integration rollback"
  temp_count=$(find "$temp_dir/dest" -name '*.tmp.*' -print | wc -l | tr -d ' ')
  assert_eq "0" "$temp_count" "atomic integration rollback cleanup"
  rm -rf "$temp_dir"
}

test_atomic_install_file_rejects_directory_destination() {
  temp_dir=$(mktemp -d)
  mkdir -p "$temp_dir/dest/entry.desktop"
  printf 'sentinel\n' > "$temp_dir/dest/entry.desktop/sentinel"
  printf 'new\n' > "$temp_dir/source.desktop"
  if atomic_install_file "$temp_dir/source.desktop" "$temp_dir/dest/entry.desktop" 0644; then
    fail "atomic integration directory destination: expected rejection"
  fi
  if [ ! -f "$temp_dir/dest/entry.desktop/sentinel" ]; then
    fail "atomic integration directory destination: sentinel changed"
  fi
  if [ -f "$temp_dir/dest/entry.desktop/.entry.desktop.tmp.$$" ]; then
    fail "atomic integration directory destination: nested temporary remained"
  fi
  rm -rf "$temp_dir"
}

test_macos_integration_helpers_defined() {
  missing=0
  assert_command_exists macos_bundle_versions "macOS bundle version normalizer" || missing=1
  assert_command_exists install_macos_desktop_integration_at \
    "macOS desktop integration helper" || missing=1
  [ "$missing" -eq 0 ]
}

test_macos_bundle_version_normalization() {
  versions=$(macos_bundle_versions 1.2.3-beta.4+build.9)
  short_version=$(printf '%s\n' "$versions" | sed -n '1p')
  bundle_version=$(printf '%s\n' "$versions" | sed -n '2p')
  assert_eq "1.2.3" "$short_version" "macOS prerelease short version"
  assert_eq "1.2.3b4" "$bundle_version" "macOS prerelease bundle version"

  versions=$(macos_bundle_versions 2.0.0-rc.7)
  assert_eq "2.0.0fc7" "$(printf '%s\n' "$versions" | sed -n '2p')" \
    "macOS release-candidate bundle version"
  versions=$(macos_bundle_versions 3.4.5)
  assert_eq "3.4.5" "$(printf '%s\n' "$versions" | sed -n '2p')" \
    "macOS stable bundle version"
}

test_macos_bundle_contract_and_gui_activation() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/Home's & <Primary>"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin with spaces"
  tools_dir="$temp_dir/tools"
  plutil_log="$temp_dir/plutil.log"
  launchctl_log="$temp_dir/launchctl.log"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  : > "$plutil_log"
  : > "$launchctl_log"
  home_physical=$(CDPATH= cd -- "$home_dir" && pwd -P)
  test_uid=$(id -u)

  output=$(WAKEZILLA_MACOS_PLUTIL_LOG="$plutil_log" \
    WAKEZILLA_MACOS_LAUNCHCTL_LOG="$launchctl_log" \
    WAKEZILLA_MACOS_GUI_DOMAIN=present \
    install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3-beta.4+build.9 \
      "$home_dir" "$test_uid" "$tools_dir/plutil" "$tools_dir/launchctl" 2>&1)

  app="$home_physical/Applications/Wakezilla.app"
  info_plist="$app/Contents/Info.plist"
  launch_agent="$home_physical/Library/LaunchAgents/dev.wakezilla.tray.plist"
  expected_files='Contents/Info.plist
Contents/MacOS/wakezilla
Contents/MacOS/wakezilla-tray
Contents/Resources/Wakezilla.icns'
  actual_files=$(CDPATH= cd -- "$app" && find Contents -type f -print | sort)
  assert_eq "$expected_files" "$actual_files" "macOS exact bundle files"
  assert_eq "755" "$(portable_file_mode "$app/Contents/MacOS/wakezilla")" \
    "macOS bundle CLI mode"
  assert_eq "755" "$(portable_file_mode "$app/Contents/MacOS/wakezilla-tray")" \
    "macOS bundle tray mode"
  assert_eq "644" "$(portable_file_mode "$info_plist")" "macOS Info.plist mode"
  assert_eq "644" "$(portable_file_mode "$app/Contents/Resources/Wakezilla.icns")" \
    "macOS bundle icon mode"
  assert_eq "644" "$(portable_file_mode "$launch_agent")" "macOS LaunchAgent mode"
  if ! cmp -s "$extract_dir/wakezilla" "$app/Contents/MacOS/wakezilla" || \
     ! cmp -s "$extract_dir/wakezilla-tray" "$app/Contents/MacOS/wakezilla-tray" || \
     ! cmp -s "$extract_dir/Wakezilla.icns" "$app/Contents/Resources/Wakezilla.icns"; then
    fail "macOS bundle assets: expected byte-identical files"
  fi

  for plist_contract in \
    '<key>CFBundleExecutable</key>' '<string>wakezilla-tray</string>' \
    '<key>CFBundleIdentifier</key>' '<string>dev.wakezilla.Wakezilla</string>' \
    '<key>CFBundleName</key>' '<string>Wakezilla</string>' \
    '<key>CFBundleDisplayName</key>' '<key>CFBundleIconFile</key>' \
    '<string>Wakezilla.icns</string>' '<key>CFBundlePackageType</key>' \
    '<string>APPL</string>' '<key>CFBundleShortVersionString</key>' \
    '<string>1.2.3</string>' '<key>CFBundleVersion</key>' \
    '<string>1.2.3b4</string>' '<key>LSUIElement</key>' '<true/>' \
    '<key>CFBundleInfoDictionaryVersion</key>' '<string>6.0</string>' \
    '<key>LSApplicationCategoryType</key>' \
    '<string>public.app-category.utilities</string>'; do
    assert_contains "$(cat "$info_plist")" "$plist_contract" \
      "macOS Info.plist contract $plist_contract"
  done

  launch_contents=$(cat "$launch_agent")
  for launch_contract in \
    '<key>Label</key>' '<string>dev.wakezilla.tray</string>' \
    '<key>ProgramArguments</key>' '<string>/usr/bin/open</string>' \
    '<string>-g</string>' '<key>RunAtLoad</key>' \
    '<key>LimitLoadToSessionType</key>' '<string>Aqua</string>' \
    '<key>ProcessType</key>' '<string>Interactive</string>' \
    '<key>AssociatedBundleIdentifiers</key>' \
    '<string>dev.wakezilla.Wakezilla</string>' '&amp;' '&lt;' '&apos;'; do
    assert_contains "$launch_contents" "$launch_contract" \
      "macOS LaunchAgent contract $launch_contract"
  done
  assert_not_contains "$launch_contents" '<key>KeepAlive</key>' \
    "macOS LaunchAgent omits KeepAlive"
  assert_not_contains "$launch_contents" 'Terminal' "macOS LaunchAgent omits Terminal"
  assert_not_contains "$launch_contents" '/bin/sh' "macOS LaunchAgent omits shell"
  assert_not_contains "$launch_contents" '\&apos;' \
    "macOS LaunchAgent apostrophe escape omits literal backslash"
  assert_contains "$launch_contents" '<key>RunAtLoad</key>
  <true/>' "macOS LaunchAgent RunAtLoad boolean"
  assert_contains "$launch_contents" '<key>AssociatedBundleIdentifiers</key>
  <array>
    <string>dev.wakezilla.Wakezilla</string>
  </array>' "macOS LaunchAgent associated bundle array"

  cli_link="$bin_dir/wakezilla"
  if [ ! -L "$cli_link" ]; then
    fail "macOS CLI endpoint: expected symlink"
  else
    assert_eq "$app/Contents/MacOS/wakezilla" "$(readlink "$cli_link")" \
      "macOS CLI absolute bundle symlink"
  fi
  if [ -e "$bin_dir/wakezilla-tray" ] || [ -L "$bin_dir/wakezilla-tray" ]; then
    fail "macOS loose tray helper: expected removal"
  fi
  plutil_calls=$(cat "$plutil_log")
  assert_contains "$plutil_calls" \
    "-lint|$home_physical/Applications/.Wakezilla.app.stage." \
    "macOS staged Info.plist lint"
  assert_contains "$plutil_calls" '/Contents/Info.plist' \
    "macOS staged Info.plist lint path"
  assert_contains "$plutil_calls" \
    "-lint|$home_physical/Library/LaunchAgents/.dev.wakezilla.tray.plist.stage." \
    "macOS staged LaunchAgent lint"
  launchctl_calls=$(cat "$launchctl_log")
  assert_contains "$launchctl_calls" "print|gui/$test_uid" "macOS GUI domain lookup"
  assert_contains "$launchctl_calls" "print|gui/$test_uid/dev.wakezilla.tray" \
    "macOS prior agent lookup"
  assert_contains "$launchctl_calls" "bootstrap|gui/$test_uid|$launch_agent" \
    "macOS LaunchAgent bootstrap"
  assert_contains "$launchctl_calls" "kickstart|-k|gui/$test_uid/dev.wakezilla.tray" \
    "macOS immediate tray kickstart"
  assert_contains "$output" "launch requested" "macOS graphical launch wording"

  if command -v plutil >/dev/null 2>&1 && [ "$(uname -s 2>/dev/null || true)" = Darwin ]; then
    plutil -lint "$info_plist" >/dev/null || fail "macOS real plutil rejected Info.plist"
    plutil -lint "$launch_agent" >/dev/null || fail "macOS real plutil rejected LaunchAgent"
  fi
  rm -rf "$temp_dir"
}

test_macos_headless_install_keeps_launch_agent() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  launchctl_log="$temp_dir/launchctl.log"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  : > "$launchctl_log"
  home_physical=$(CDPATH= cd -- "$home_dir" && pwd -P)
  test_uid=$(id -u)

  output=$(WAKEZILLA_MACOS_LAUNCHCTL_LOG="$launchctl_log" \
    WAKEZILLA_MACOS_GUI_DOMAIN=absent \
    install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" 2>&1)

  if [ ! -f "$home_dir/Library/LaunchAgents/dev.wakezilla.tray.plist" ]; then
    fail "macOS headless install: expected persistent LaunchAgent"
  fi
  assert_contains "$output" "next graphical login" "macOS headless next-login warning"
  assert_not_contains "$(cat "$launchctl_log")" 'bootstrap|' \
    "macOS headless skips bootstrap"
  assert_not_contains "$(cat "$launchctl_log")" 'kickstart|' \
    "macOS headless skips kickstart"
  rm -rf "$temp_dir"
}

test_macos_rejects_root_unsafe_home_and_invalid_assets_before_writes() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)

  if output=$(install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" 0 \
    "$tools_dir/plutil" "$tools_dir/launchctl" 2>&1); then
    fail "macOS root install: expected rejection"
  else
    assert_contains "$output" "without sudo" "macOS root rerun guidance"
  fi
  if [ -e "$home_dir/Applications" ] || [ -e "$home_dir/Library" ]; then
    fail "macOS root install: expected no profile publication"
  fi

  if install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 relative-home "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS relative HOME: expected rejection"
  fi
  real_home="$temp_dir/real-home"
  linked_home="$temp_dir/linked-home"
  mkdir -p "$real_home"
  ln -s "$real_home" "$linked_home"
  if install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$linked_home" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS symlink HOME: expected rejection"
  fi

  rm -f "$extract_dir/Wakezilla.icns"
  if install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS missing icon: expected rejection"
  fi
  if [ -e "$home_dir/Applications" ] || [ -e "$home_dir/Library" ]; then
    fail "macOS invalid assets: expected validation before profile writes"
  fi
  rm -rf "$temp_dir"
}

test_macos_rejects_foreign_or_symlink_bundle_without_unrelated_deletion() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  app="$home_dir/Applications/Wakezilla.app"
  mkdir -p "$app/Contents" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)
  cat > "$app/Contents/Info.plist" <<'EOF'
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key>
<string>com.example.Foreign</string>
</dict></plist>
EOF
  printf 'foreign sentinel\n' > "$app/foreign-sentinel"
  if install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS foreign bundle: expected rejection"
  fi
  assert_eq "foreign sentinel" "$(cat "$app/foreign-sentinel")" \
    "macOS foreign bundle preserved"

  rm -rf "$app"
  outside="$temp_dir/outside-app"
  mkdir -p "$outside"
  printf 'outside sentinel\n' > "$outside/sentinel"
  ln -s "$outside" "$app"
  if install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS bundle destination symlink: expected rejection"
  fi
  assert_eq "outside sentinel" "$(cat "$outside/sentinel")" \
    "macOS bundle symlink target preserved"
  rm -rf "$temp_dir"
}

test_macos_rejects_wrong_owner_and_symlinked_profile_destinations() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  outside_dir="$temp_dir/outside"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir" "$outside_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)
  wrong_uid=$((test_uid + 1))

  if install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$wrong_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS HOME ownership: expected UID mismatch rejection"
  fi
  if [ -e "$home_dir/Applications" ] || [ -e "$home_dir/Library" ]; then
    fail "macOS HOME ownership: expected rejection before profile writes"
  fi

  for symlink_case in Applications Library LaunchAgents; do
    rm -rf "$home_dir/Applications" "$home_dir/Library" "$outside_dir"
    mkdir -p "$outside_dir"
    printf 'outside sentinel\n' > "$outside_dir/sentinel"
    case "$symlink_case" in
      Applications)
        ln -s "$outside_dir" "$home_dir/Applications"
        ;;
      Library)
        ln -s "$outside_dir" "$home_dir/Library"
        ;;
      LaunchAgents)
        mkdir -p "$home_dir/Library"
        ln -s "$outside_dir" "$home_dir/Library/LaunchAgents"
        ;;
    esac
    if install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
      fail "macOS $symlink_case parent symlink: expected rejection"
    fi
    outside_count=$(find "$outside_dir" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')
    assert_eq "1" "$outside_count" "macOS $symlink_case parent symlink containment"
  done

  rm -rf "$home_dir/Applications" "$home_dir/Library" "$outside_dir"
  mkdir -p "$home_dir/Library/LaunchAgents" "$outside_dir"
  printf 'foreign agent bytes\n' > "$outside_dir/foreign-agent"
  ln -s "$outside_dir/foreign-agent" \
    "$home_dir/Library/LaunchAgents/dev.wakezilla.tray.plist"
  if install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS LaunchAgent destination symlink: expected rejection"
  fi
  assert_eq "foreign agent bytes" "$(cat "$outside_dir/foreign-agent")" \
    "macOS foreign LaunchAgent symlink target preserved"
  rm -rf "$temp_dir"
}

test_macos_launchctl_partial_failure_rolls_back_loaded_state() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  launchctl_log="$temp_dir/launchctl.log"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)

  WAKEZILLA_MACOS_GUI_DOMAIN=absent install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1

  : > "$launchctl_log"
  if WAKEZILLA_MACOS_LAUNCHCTL_LOG="$launchctl_log" \
    WAKEZILLA_MACOS_GUI_DOMAIN=present WAKEZILLA_MACOS_OLD_AGENT_LOADED=yes \
    WAKEZILLA_MACOS_FAIL_LAUNCHCTL=bootstrap \
    install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS partial bootstrap failure: expected fatal status"
  fi
  bootout_count=$(grep -c '^bootout|' "$launchctl_log" 2>/dev/null || true)
  assert_eq "2" "$bootout_count" \
    "macOS partial bootstrap rollback attempts new-agent bootout"
  bootstrap_count=$(grep -c '^bootstrap|' "$launchctl_log" 2>/dev/null || true)
  assert_eq "2" "$bootstrap_count" \
    "macOS partial bootstrap rollback reloads previously unloaded agent"

  : > "$launchctl_log"
  if WAKEZILLA_MACOS_LAUNCHCTL_LOG="$launchctl_log" \
    WAKEZILLA_MACOS_GUI_DOMAIN=present WAKEZILLA_MACOS_OLD_AGENT_LOADED=yes \
    WAKEZILLA_MACOS_FAIL_LAUNCHCTL=bootout \
    install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS old-agent bootout failure: expected fatal status"
  fi
  assert_not_contains "$(cat "$launchctl_log")" 'bootstrap|' \
    "macOS failed old-agent bootout does not spuriously bootstrap"
  rm -rf "$temp_dir"
}

test_macos_failed_existing_bundle_move_preserves_original() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)

  WAKEZILLA_MACOS_GUI_DOMAIN=absent install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1
  home_physical=$(CDPATH= cd -- "$home_dir" && pwd -P)
  app="$home_physical/Applications/Wakezilla.app"
  printf 'original bundle sentinel\n' > "$app/original-sentinel"
  original_cli=$(cat "$app/Contents/MacOS/wakezilla")
  real_mv=$(command -v mv)
  cat > "$tools_dir/mv" <<SH
#!/usr/bin/env sh
if [ "\${1:-}" = "\${WAKEZILLA_TEST_FAIL_MOVE_SOURCE:-}" ]; then
  exit 73
fi
exec '$real_mv' "\$@"
SH
  chmod 0755 "$tools_dir/mv"

  if PATH="$tools_dir:$PATH" WAKEZILLA_TEST_FAIL_MOVE_SOURCE="$app" \
    WAKEZILLA_MACOS_GUI_DOMAIN=absent install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS failed existing bundle move: expected fatal status"
  fi
  assert_eq "original bundle sentinel" "$(cat "$app/original-sentinel" 2>/dev/null || printf '')" \
    "macOS failed existing bundle move preserves original bundle"
  assert_eq "$original_cli" "$(cat "$app/Contents/MacOS/wakezilla" 2>/dev/null || printf '')" \
    "macOS failed existing bundle move preserves original CLI"
  backup_count=$(find "$home_physical/Applications" -maxdepth 1 \
    -name '.Wakezilla.app.backup.*' -print | wc -l | tr -d ' ')
  assert_eq "0" "$backup_count" "macOS failed existing bundle move cleans backup placeholder"
  rm -rf "$temp_dir"
}

test_macos_incomplete_bundle_restore_preserves_recovery_backup() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)

  WAKEZILLA_MACOS_GUI_DOMAIN=absent install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1
  home_physical=$(CDPATH= cd -- "$home_dir" && pwd -P)
  app="$home_physical/Applications/Wakezilla.app"
  printf 'recoverable old bundle\n' > "$app/recovery-sentinel"
  real_mv=$(command -v mv)
  cat > "$tools_dir/mv" <<SH
#!/usr/bin/env sh
case "\${1:-}" in
  */.Wakezilla.app.backup.*) exit 74 ;;
esac
exec '$real_mv' "\$@"
SH
  chmod 0755 "$tools_dir/mv"

  if PATH="$tools_dir:$PATH" WAKEZILLA_MACOS_GUI_DOMAIN=present \
    WAKEZILLA_MACOS_OLD_AGENT_LOADED=yes WAKEZILLA_MACOS_FAIL_LAUNCHCTL=kickstart \
    install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS incomplete bundle restore: expected fatal status"
  fi
  backup=$(find "$home_physical/Applications" -maxdepth 1 -type d \
    -name '.Wakezilla.app.backup.*' -print | head -n 1)
  if [ -z "$backup" ]; then
    fail "macOS incomplete bundle restore: recovery backup was deleted"
  else
    assert_eq "recoverable old bundle" "$(cat "$backup/recovery-sentinel")" \
      "macOS incomplete bundle restore keeps prior bundle bytes"
  fi
  recovery_count=$(find "$home_physical" -maxdepth 1 -type d \
    -name '.wakezilla-macos-install.*' -print | wc -l | tr -d ' ')
  assert_eq "1" "$recovery_count" \
    "macOS incomplete bundle restore keeps integration snapshots"
  rm -rf "$temp_dir"
}

test_macos_setup_failure_cleans_staged_siblings() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  mktemp_count="$temp_dir/mktemp-count"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)
  printf '0\n' > "$mktemp_count"
  real_mktemp=$(command -v mktemp)
  cat > "$tools_dir/mktemp" <<SH
#!/usr/bin/env sh
count=\$(cat "\$WAKEZILLA_TEST_MKTEMP_COUNT")
count=\$((count + 1))
printf '%s\n' "\$count" > "\$WAKEZILLA_TEST_MKTEMP_COUNT"
if [ "\$count" -eq 3 ]; then
  exit 75
fi
exec '$real_mktemp' "\$@"
SH
  chmod 0755 "$tools_dir/mktemp"

  if PATH="$tools_dir:$PATH" WAKEZILLA_TEST_MKTEMP_COUNT="$mktemp_count" \
    WAKEZILLA_MACOS_GUI_DOMAIN=absent install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS setup mktemp failure: expected fatal status"
  fi
  temporary_count=$(find "$home_dir" \
    \( -name '.wakezilla-macos-install.*' -o -name '.Wakezilla.app.stage.*' \
       -o -name '.Wakezilla.app.backup.*' -o -name '.dev.wakezilla.tray.plist.stage.*' \) \
    ! -name '.wakezilla-macos-install.lock' \
    -print | wc -l | tr -d ' ')
  assert_eq "0" "$temporary_count" "macOS setup mktemp failure cleans staged siblings"
  if [ -e "$home_dir/.wakezilla-macos-install.lock" ]; then
    fail "macOS setup mktemp failure: installer lock remained"
  fi
  rm -rf "$temp_dir"
}

test_macos_install_lock_contention_fails_without_removing_owner() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  lock_file="$home_dir/.wakezilla-macos-install.lock"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  printf '%s\n' "$$" > "$lock_file"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)

  if WAKEZILLA_MACOS_GUI_DOMAIN=absent install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS installer lock contention: expected fatal status"
  fi
  assert_eq "$$" "$(cat "$lock_file")" \
    "macOS installer lock contention preserves owner"
  if [ -e "$home_dir/Applications" ] || [ -e "$home_dir/Library" ]; then
    fail "macOS installer lock contention: published profile artifacts"
  fi
  rm -rf "$temp_dir"
}

test_macos_stale_install_lock_is_recovered() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  lock_file="$home_dir/.wakezilla-macos-install.lock"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  printf '99999999\n' > "$lock_file"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)

  if ! WAKEZILLA_MACOS_GUI_DOMAIN=absent install_macos_desktop_integration_at \
    "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
    "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS stale installer lock: expected recovery"
  fi
  if [ -e "$lock_file" ]; then
    fail "macOS stale installer lock: lock remained after recovery"
  fi
  rm -rf "$temp_dir"
}

test_macos_bundle_publication_race_preserves_foreign_app() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)
  home_physical=$(CDPATH= cd -- "$home_dir" && pwd -P)
  app="$home_physical/Applications/Wakezilla.app"
  real_mv=$(command -v mv)
  cat > "$tools_dir/mv" <<SH
#!/usr/bin/env sh
case "\${1:-}" in
  -*) source_path="\${2:-}"; destination_path="\${3:-}" ;;
  *) source_path="\${1:-}"; destination_path="\${2:-}" ;;
esac
case "\$source_path" in
  */.Wakezilla.app.stage.*)
    if [ "\$destination_path" = "\$WAKEZILLA_TEST_RACE_BUNDLE" ]; then
      mkdir -p "\$destination_path"
      printf 'concurrent foreign app\n' > "\$destination_path/foreign-sentinel"
    fi
    ;;
esac
exec '$real_mv' "\$@"
SH
  chmod 0755 "$tools_dir/mv"

  if PATH="$tools_dir:$PATH" WAKEZILLA_TEST_RACE_BUNDLE="$app" \
    WAKEZILLA_MACOS_GUI_DOMAIN=absent install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS bundle publication race: expected fatal status"
  fi
  assert_eq "concurrent foreign app" "$(cat "$app/foreign-sentinel" 2>/dev/null || printf '')" \
    "macOS bundle publication race preserves foreign app"
  nested_count=$(find "$app" -maxdepth 1 -name '.Wakezilla.app.stage.*' -print \
    2>/dev/null | wc -l | tr -d ' ')
  assert_eq "0" "$nested_count" "macOS bundle publication race removes only nested own stage"
  if [ -e "$bin_dir/wakezilla" ] || [ -e "$home_physical/Library/LaunchAgents/dev.wakezilla.tray.plist" ]; then
    fail "macOS bundle publication race: published CLI or LaunchAgent"
  fi
  rm -rf "$temp_dir"
}

test_macos_agent_publication_race_preserves_foreign_agent() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  test_uid=$(id -u)
  home_physical=$(CDPATH= cd -- "$home_dir" && pwd -P)
  app="$home_physical/Applications/Wakezilla.app"
  agent="$home_physical/Library/LaunchAgents/dev.wakezilla.tray.plist"
  real_ln=$(command -v ln)
  cat > "$tools_dir/ln" <<SH
#!/usr/bin/env sh
source_path="\${1:-}"
destination_path="\${2:-}"
case "\$source_path" in
  */.dev.wakezilla.tray.plist.stage.*)
    if [ "\$destination_path" = "\$WAKEZILLA_TEST_RACE_AGENT" ]; then
      mkdir -p "\$destination_path"
      printf 'concurrent foreign agent directory\n' > "\$destination_path/foreign-sentinel"
    fi
    ;;
esac
exec '$real_ln' "\$@"
SH
  chmod 0755 "$tools_dir/ln"

  if PATH="$tools_dir:$PATH" WAKEZILLA_TEST_RACE_AGENT="$agent" \
    WAKEZILLA_MACOS_GUI_DOMAIN=absent install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null 2>&1; then
    fail "macOS LaunchAgent publication race: expected fatal status"
  fi
  assert_eq "concurrent foreign agent directory" \
    "$(cat "$agent/foreign-sentinel" 2>/dev/null || printf '')" \
    "macOS LaunchAgent publication race preserves foreign agent directory"
  nested_count=$(find "$agent" -maxdepth 1 -name '.dev.wakezilla.tray.plist.stage.*' \
    -print 2>/dev/null | wc -l | tr -d ' ')
  assert_eq "0" "$nested_count" \
    "macOS LaunchAgent publication race removes only nested owned hard link"
  if [ -e "$app" ] || [ -L "$bin_dir/wakezilla" ]; then
    fail "macOS LaunchAgent publication race: failed to roll back owned bundle or CLI"
  fi
  if [ -e "$home_dir/.wakezilla-macos-install.lock" ]; then
    fail "macOS LaunchAgent publication race: installer lock remained"
  fi
  rm -rf "$temp_dir"
}

test_macos_reinstall_is_idempotent_and_rolls_back_late_failure() {
  temp_dir=$(mktemp -d)
  home_dir="$temp_dir/home"
  extract_dir="$temp_dir/extract"
  bin_dir="$temp_dir/bin"
  tools_dir="$temp_dir/tools"
  launchctl_log="$temp_dir/launchctl.log"
  mkdir -p "$home_dir" "$bin_dir" "$tools_dir"
  write_macos_integration_fixture "$extract_dir"
  write_fake_plutil "$tools_dir/plutil"
  write_fake_launchctl "$tools_dir/launchctl"
  : > "$launchctl_log"
  home_physical=$(CDPATH= cd -- "$home_dir" && pwd -P)
  test_uid=$(id -u)

  WAKEZILLA_MACOS_LAUNCHCTL_LOG="$launchctl_log" \
    WAKEZILLA_MACOS_GUI_DOMAIN=present \
    install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null
  printf 'macos cli second bytes\n' > "$extract_dir/wakezilla"
  printf 'macos tray second bytes\n' > "$extract_dir/wakezilla-tray"
  WAKEZILLA_MACOS_LAUNCHCTL_LOG="$launchctl_log" \
    WAKEZILLA_MACOS_GUI_DOMAIN=present WAKEZILLA_MACOS_OLD_AGENT_LOADED=yes \
    install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" >/dev/null
  app="$home_physical/Applications/Wakezilla.app"
  assert_eq "macos cli second bytes" "$(cat "$app/Contents/MacOS/wakezilla")" \
    "macOS idempotent bundle replacement"
  assert_contains "$(cat "$launchctl_log")" \
    "bootout|gui/$test_uid|$home_physical/Library/LaunchAgents/dev.wakezilla.tray.plist" \
    "macOS idempotent old agent bootout"
  temporary_count=$(find "$home_dir/Applications" "$home_dir/Library/LaunchAgents" \
    -name '.Wakezilla*' -o -name '.dev.wakezilla.tray*' | wc -l | tr -d ' ')
  assert_eq "0" "$temporary_count" "macOS idempotent temporary cleanup"

  old_cli=$(cat "$app/Contents/MacOS/wakezilla")
  old_tray=$(cat "$app/Contents/MacOS/wakezilla-tray")
  old_info=$(cat "$app/Contents/Info.plist")
  old_icon=$(cat "$app/Contents/Resources/Wakezilla.icns")
  launch_agent="$home_physical/Library/LaunchAgents/dev.wakezilla.tray.plist"
  old_agent=$(cat "$launch_agent")
  old_link=$(readlink "$bin_dir/wakezilla")
  rm -f "$bin_dir/wakezilla"
  ln -s "$temp_dir/prior-cli-target" "$bin_dir/wakezilla"
  prior_link=$(readlink "$bin_dir/wakezilla")
  printf 'prior loose helper\n' > "$bin_dir/wakezilla-tray"
  chmod 0640 "$bin_dir/wakezilla-tray"
  printf 'macos cli failed bytes\n' > "$extract_dir/wakezilla"
  printf 'macos tray failed bytes\n' > "$extract_dir/wakezilla-tray"

  set +e
  output=$(WAKEZILLA_MACOS_LAUNCHCTL_LOG="$launchctl_log" \
    WAKEZILLA_MACOS_GUI_DOMAIN=present WAKEZILLA_MACOS_OLD_AGENT_LOADED=yes \
    WAKEZILLA_MACOS_FAIL_LAUNCHCTL=kickstart \
    install_macos_desktop_integration_at \
      "$extract_dir" "$bin_dir" 1.2.3 "$home_dir" "$test_uid" \
      "$tools_dir/plutil" "$tools_dir/launchctl" 2>&1)
  status=$?
  set -e
  if [ "$status" -eq 0 ]; then
    fail "macOS late kickstart failure: expected fatal status"
  fi
  assert_eq "$old_cli" "$(cat "$app/Contents/MacOS/wakezilla")" \
    "macOS rollback bundle CLI bytes"
  assert_eq "$old_tray" "$(cat "$app/Contents/MacOS/wakezilla-tray")" \
    "macOS rollback bundle tray bytes"
  assert_eq "$old_info" "$(cat "$app/Contents/Info.plist")" \
    "macOS rollback Info.plist bytes"
  assert_eq "$old_icon" "$(cat "$app/Contents/Resources/Wakezilla.icns")" \
    "macOS rollback icon bytes"
  assert_eq "$old_agent" "$(cat "$launch_agent")" \
    "macOS rollback LaunchAgent bytes"
  assert_eq "$prior_link" "$(readlink "$bin_dir/wakezilla")" \
    "macOS rollback prior CLI symlink"
  assert_eq "prior loose helper" "$(cat "$bin_dir/wakezilla-tray")" \
    "macOS rollback loose helper bytes"
  assert_eq "640" "$(portable_file_mode "$bin_dir/wakezilla-tray")" \
    "macOS rollback loose helper mode"
  assert_not_contains "$(readlink "$bin_dir/wakezilla")" "$old_link" \
    "macOS rollback restores immediate preinstall symlink state"
  rm -rf "$temp_dir"
}

if test_linux_desktop_integration_helpers_defined; then
  test_linux_desktop_integration_writes_xdg_entries_headless
  test_desktop_exec_quote_escapes_reserved_characters
  test_desktop_exec_gio_launches_hostile_path_without_arguments
  test_desktop_exec_quote_rejects_line_breaks
  test_desktop_entry_encoders_reject_ascii_controls
  test_linux_desktop_integration_launches_graphical_helper_once
  test_linux_integration_relative_xdg_falls_back_and_copies_icons
  test_linux_integration_validates_all_assets_before_writes
  test_linux_integration_is_idempotent_without_temp_siblings
  test_linux_integration_rolls_back_late_autostart_failure
  test_linux_integration_profile_directory_modes
  test_linux_integration_reports_incomplete_rollback_and_continues
  test_linux_integration_root_helper_rolls_back_late_failure
  test_linux_integration_root_without_valid_sudo_user_skips_everything
  test_linux_integration_euid_override_is_test_mode_only
  test_linux_integration_valid_sudo_user_applies_as_target_user
  test_linux_integration_root_treats_home_desktop_as_disabled
  test_linux_integration_privilege_failures_are_fatal
  test_resolve_linux_integration_user_rejects_unsafe_sudo_homes
  test_linux_integration_ignores_test_sudo_overrides_outside_test_mode
  test_linux_integration_root_discards_desktop_outside_target_home
  test_linux_integration_root_rejects_profile_traversal_and_symlinks
  test_linux_integration_root_dropped_apply_rejects_intermediate_symlinks
  test_linux_integration_post_validation_setup_failures_are_fatal
  test_linux_integration_rejects_symlinked_archive_assets
  test_linux_integration_rejects_line_break_in_helper_path_before_writes
  test_atomic_install_file_rolls_back_failed_rename
  test_atomic_install_file_rejects_directory_destination
fi

if test_linux_desktop_resolution_helpers_defined; then
  test_resolve_linux_desktop_dir_prefers_xdg_user_dir
  test_resolve_linux_desktop_dir_parses_without_execution
  test_resolve_linux_desktop_dir_treats_home_as_disabled
  test_linux_desktop_copy_and_gio_trust_are_best_effort
  test_linux_integration_removes_only_owned_legacy_autostart
fi

if test_macos_integration_helpers_defined; then
  test_macos_bundle_version_normalization
  test_macos_bundle_contract_and_gui_activation
  test_macos_headless_install_keeps_launch_agent
  test_macos_rejects_root_unsafe_home_and_invalid_assets_before_writes
  test_macos_rejects_foreign_or_symlink_bundle_without_unrelated_deletion
  test_macos_rejects_wrong_owner_and_symlinked_profile_destinations
  test_macos_launchctl_partial_failure_rolls_back_loaded_state
  test_macos_failed_existing_bundle_move_preserves_original
  test_macos_incomplete_bundle_restore_preserves_recovery_backup
  test_macos_setup_failure_cleans_staged_siblings
  test_macos_install_lock_contention_fails_without_removing_owner
  test_macos_stale_install_lock_is_recovered
  test_macos_bundle_publication_race_preserves_foreign_app
  test_macos_agent_publication_race_preserves_foreign_agent
  test_macos_reinstall_is_idempotent_and_rolls_back_late_failure
fi

if [ "$failures" -ne 0 ]; then
  printf '%s test(s) failed\n' "$failures" >&2
  exit 1
fi

printf 'install.sh tests passed\n'
