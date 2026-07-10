#!/usr/bin/env sh
set -eu

REPO="${REPO:-guibeira/wakezilla}"
BIN_NAME="${BIN_NAME:-wakezilla}"
TRAY_HELPER_NAME="${TRAY_HELPER_NAME:-wakezilla-tray}"

info() {
  printf '%s\n' "$*"
}

warn() {
  printf 'warning: %s\n' "$*" >&2
}

err() {
  stage="$1"
  shift
  printf 'error[%s]: %s\n' "$stage" "$*" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Usage: install.sh [OPTIONS] [VERSION]

Install Wakezilla from GitHub Releases.

Options:
  -h, --help      Show this help message

Environment variables:
  VERSION         Version to install, without leading v (example: 0.1.49)
  BIN_DIR         Binary installation directory
  PREFIX          Installation prefix used when BIN_DIR is unset (default: $HOME/.local, or /usr/local when run as root)
  TARGET          Override target triple (example: x86_64-unknown-linux-gnu)
  REPO            GitHub repository (default: guibeira/wakezilla)
  GITHUB_TOKEN    Token for authenticated GitHub API requests
  WAKEZILLA_SUDO_SYMLINK  yes|no to skip the prompt for the /usr/local/bin symlink

Examples:
  curl -fsSL https://raw.githubusercontent.com/guibeira/wakezilla/main/install.sh | sh
  curl -fsSL https://raw.githubusercontent.com/guibeira/wakezilla/main/install.sh | sh -s -- 0.1.49
  VERSION=0.1.49 BIN_DIR=/usr/local/bin sh install.sh
USAGE
}

detect_libc() {
  if [ -n "${WAKEZILLA_LIBC:-}" ]; then
    printf '%s\n' "$WAKEZILLA_LIBC"
    return 0
  fi

  # ldd prints its banner to stderr on glibc and stdout on musl; check both.
  if ldd --version 2>&1 | grep -qi musl; then
    printf 'musl\n'
    return 0
  fi

  # Fallback: musl ships a loader named ld-musl-<arch>.so.* under /lib.
  for loader in /lib/ld-musl-*.so.*; do
    if [ -e "$loader" ]; then
      printf 'musl\n'
      return 0
    fi
  done

  printf 'gnu\n'
}

detect_target() {
  if [ -n "${TARGET:-}" ]; then
    printf '%s\n' "$TARGET"
    return 0
  fi

  uname_s="${WAKEZILLA_UNAME_S:-$(uname -s 2>/dev/null || echo unknown)}"
  uname_m="${WAKEZILLA_UNAME_M:-$(uname -m 2>/dev/null || echo unknown)}"

  case "$uname_m" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *) err "platform" "unsupported architecture: $uname_m" ;;
  esac

  case "$uname_s:$arch" in
    Linux:x86_64) printf 'x86_64-unknown-linux-%s\n' "$(detect_libc)" ;;
    Linux:aarch64) printf 'aarch64-unknown-linux-%s\n' "$(detect_libc)" ;;
    Darwin:x86_64) printf 'x86_64-apple-darwin\n' ;;
    Darwin:aarch64) printf 'aarch64-apple-darwin\n' ;;
    *)
      err "platform" "unsupported platform: $uname_s/$uname_m. Supported release targets are x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl, x86_64-apple-darwin, aarch64-apple-darwin"
      ;;
  esac
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -h|--help)
        usage
        exit 0
        ;;
      -*)
        err "args" "unknown option: $1 (use --help for usage)"
        ;;
      *)
        if [ -n "${VERSION:-}" ]; then
          err "args" "unexpected argument: $1 (VERSION is already set to $VERSION)"
        fi
        VERSION="$1"
        ;;
    esac
    shift
  done
}

resolve_bin_dir() {
  euid="${WAKEZILLA_EUID:-$(id -u 2>/dev/null || printf '')}"
  if [ -n "${BIN_DIR:-}" ]; then
    printf '%s\n' "$BIN_DIR"
  elif [ -n "${PREFIX:-}" ]; then
    printf '%s/bin\n' "$PREFIX"
  elif [ "$euid" = "0" ]; then
    # Running as root (e.g. curl ... | sudo sh): install into a system path on
    # sudo's secure_path so `sudo wakezilla` works without extra setup.
    printf '/usr/local/bin\n'
  elif [ -n "${HOME:-}" ]; then
    printf '%s/.local/bin\n' "$HOME"
  else
    err "install" "HOME is not set; set BIN_DIR or PREFIX to choose an install directory"
  fi
}

canonicalize_bin_dir() (
  canonical_input="$1"
  case "$canonical_input" in
    /*) canonical_path=$canonical_input ;;
    *)
      canonical_pwd=$(pwd -P) || exit 1
      canonical_path="$canonical_pwd/$canonical_input"
      ;;
  esac
  mkdir -p "$canonical_path" || exit 1
  CDPATH= cd -- "$canonical_path" || exit 1
  pwd -P
)

pkg_manager_hint() {
  pkg="$1"
  if command -v brew >/dev/null 2>&1; then
    printf 'brew install %s' "$pkg"
  elif command -v apt-get >/dev/null 2>&1; then
    printf 'apt-get install -y %s' "$pkg"
  elif command -v dnf >/dev/null 2>&1; then
    printf 'dnf install -y %s' "$pkg"
  elif command -v yum >/dev/null 2>&1; then
    printf 'yum install -y %s' "$pkg"
  elif command -v apk >/dev/null 2>&1; then
    printf 'apk add %s' "$pkg"
  elif command -v pacman >/dev/null 2>&1; then
    printf 'pacman -S --noconfirm %s' "$pkg"
  else
    printf 'install %s via your package manager' "$pkg"
  fi
}

have_checksum_tool() {
  command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1
}

check_dependencies() {
  command -v curl >/dev/null 2>&1 || err "dependency" "curl is required ($(pkg_manager_hint curl))"
  command -v jq >/dev/null 2>&1 || err "dependency" "jq is required ($(pkg_manager_hint jq))"
  command -v tar >/dev/null 2>&1 || err "dependency" "tar is required ($(pkg_manager_hint tar))"
  have_checksum_tool || err "dependency" "sha256sum or shasum is required ($(pkg_manager_hint coreutils))"
}

release_version_from_json() {
  jq -r '.tag_name | sub("^wakezilla/v"; "") | sub("^v"; "")'
}

asset_url_from_json() {
  bin_name="$1"
  version="$2"
  target="$3"
  asset_name="${bin_name}-${version}-${target}.tar.gz"
  jq -r --arg name "$asset_name" '.assets[] | select(.name == $name) | .browser_download_url' | head -n 1
}

available_targets_from_json() {
  bin_name="$1"
  jq -r --arg bin_name "$bin_name" '
    (.tag_name | sub("^wakezilla/v"; "") | sub("^v"; "")) as $version
    | ($bin_name + "-" + $version + "-") as $prefix
    | .assets[]
    | .name
    | select(startswith($prefix))
    | select(endswith(".tar.gz"))
    | .[($prefix | length):]
    | .[:-7]
  ' | sort -u
}

github_api() {
  url="$1"

  # A token is never required for public repositories. When one is present we
  # try it first, but fall back to an unauthenticated request if it fails (for
  # example a stale or invalid GITHUB_TOKEN left in the environment).
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    if curl -fsSL \
      -H "Accept: application/vnd.github+json" \
      -H "X-GitHub-Api-Version: 2022-11-28" \
      -H "Authorization: Bearer $GITHUB_TOKEN" \
      "$url"; then
      return 0
    fi
    warn "GitHub request with GITHUB_TOKEN failed; retrying without authentication"
  fi

  curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "$url"
}

fetch_release_json() {
  version="$1"
  if [ -n "$version" ]; then
    github_api "https://api.github.com/repos/$REPO/releases/tags/v$version"
  else
    github_api "https://api.github.com/repos/$REPO/releases/latest"
  fi
}

download_file() {
  url="$1"
  dst="$2"
  label="$3"

  info "downloading $label..."
  if [ -t 2 ]; then
    curl -fL --progress-bar "$url" -o "$dst" || err "download" "failed to download $url"
  else
    curl -fsSL "$url" -o "$dst" || err "download" "failed to download $url"
  fi
}

checksum_url_for_release() {
  version="$1"
  printf 'https://github.com/%s/releases/download/v%s/SHA256SUMS\n' "$REPO" "$version"
}

verify_checksum() {
  file="$1"
  checksums="$2"
  asset_name="$3"
  expected=$(awk -v name="$asset_name" '$2 == name { print $1; exit }' "$checksums")

  if [ -z "$expected" ]; then
    err "checksum" "checksum not found for $asset_name"
  fi

  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$file" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$file" | awk '{print $1}')
  else
    err "dependency" "sha256sum or shasum is required ($(pkg_manager_hint coreutils))"
  fi

  if [ "$actual" != "$expected" ]; then
    err "checksum" "checksum verification failed for $asset_name"
  fi
}

install_bin() {
  src="$1"
  dst="$2"
  if command -v install >/dev/null 2>&1; then
    install -m 755 "$src" "$dst"
  else
    dst_dir=$(dirname "$dst")
    dst_tmp="$dst_dir/.${BIN_NAME:-wakezilla}.install.$$"
    rm -f "$dst_tmp"
    if cp "$src" "$dst_tmp" && chmod 755 "$dst_tmp" && mv -f "$dst_tmp" "$dst"; then
      return 0
    fi
    status=$?
    rm -f "$dst_tmp"
    return "$status"
  fi
}

extract_binary() {
  archive="$1"
  out_dir="$2"
  bin_name="$3"

  mkdir -p "$out_dir"
  case "$archive" in
    *.tar.gz|*.tgz)
      tar -xzf "$archive" -C "$out_dir" || err "extract" "failed to extract $(basename "$archive")"
      ;;
    *)
      err "extract" "unsupported archive format: $(basename "$archive")"
      ;;
  esac

  bin_file=$(find_binary_in_dir "$out_dir" "$bin_name")

  if [ -z "${bin_file:-}" ] || [ ! -f "$bin_file" ]; then
    err "binary_lookup" "binary $bin_name not found in downloaded asset"
  fi

  chmod 755 "$bin_file"
  printf '%s\n' "$bin_file"
}

find_binary_in_dir() {
  root_dir="$1"
  bin_name="$2"

  if [ -f "$root_dir/$bin_name" ]; then
    printf '%s\n' "$root_dir/$bin_name"
    return 0
  fi

  find "$root_dir" -type f -name "$bin_name" 2>/dev/null | head -n 1
}

install_optional_tray_helper() {
  extract_dir="$1"
  install_dir="$2"
  helper_file=$(find_binary_in_dir "$extract_dir" "$TRAY_HELPER_NAME")
  helper_dst="$install_dir/$TRAY_HELPER_NAME"

  if [ -n "${helper_file:-}" ] && [ -f "$helper_file" ]; then
    chmod 755 "$helper_file"
    install_bin "$helper_file" "$helper_dst" || err "install" "failed to install binary to $helper_dst"
    return 0
  fi

  if [ -e "$helper_dst" ] || [ -L "$helper_dst" ]; then
    rm -f "$helper_dst" || warn "failed to remove stale tray helper at $helper_dst"
  fi
}

path_guidance() {
  bin_dir="$1"
  case ":${PATH:-}:" in
    *":$bin_dir:"*) return 0 ;;
  esac

  shell_path="${SHELL:-sh}"
  shell_name="${shell_path##*/}"
  printf '\nadd %s to your PATH.\n' "$bin_dir"
  case "$shell_name" in
    bash)
      uname_s="${WAKEZILLA_UNAME_S:-$(uname -s 2>/dev/null || echo unknown)}"
      if [ "$uname_s" = "Darwin" ]; then
        rc="${HOME:-}/.bash_profile"
      else
        rc="${HOME:-}/.bashrc"
      fi
      if [ -n "${HOME:-}" ]; then
        printf 'For bash:\n'
        printf '  echo '\''export PATH="%s:$PATH"'\'' >> "%s"\n' "$bin_dir" "$rc"
        printf '  source "%s"\n' "$rc"
      else
        printf 'For bash:\n'
        printf '  export PATH="%s:$PATH"\n' "$bin_dir"
      fi
      ;;
    zsh)
      if [ -n "${ZDOTDIR:-}" ]; then
        rc="$ZDOTDIR/.zshrc"
      elif [ -n "${HOME:-}" ]; then
        rc="$HOME/.zshrc"
      else
        rc=
      fi
      printf 'For zsh:\n'
      if [ -n "$rc" ]; then
        printf '  echo '\''export PATH="%s:$PATH"'\'' >> "%s"\n' "$bin_dir" "$rc"
        printf '  source "%s"\n' "$rc"
      else
        printf '  export PATH="%s:$PATH"\n' "$bin_dir"
      fi
      ;;
    fish)
      printf 'For fish:\n'
      printf '  fish_add_path "%s"\n' "$bin_dir"
      ;;
    *)
      printf 'For your current shell:\n'
      printf '  export PATH="%s:$PATH"\n' "$bin_dir"
      ;;
  esac
}

musl_fallback_target() {
  case "$1" in
    x86_64-unknown-linux-gnu) printf 'x86_64-unknown-linux-musl\n' ;;
    aarch64-unknown-linux-gnu) printf 'aarch64-unknown-linux-musl\n' ;;
    *) printf '' ;;
  esac
}

# Download, verify and install the asset for a target using the already-fetched
# release JSON. Sets INSTALLED_ASSET_URL on success. Returns 2 when the target
# has no published asset so the caller can decide how to handle it.
download_and_install_target() {
  install_target="$1"
  install_asset_url=$(printf '%s' "$json" | asset_url_from_json "$BIN_NAME" "$release_version" "$install_target")

  if [ -z "$install_asset_url" ] || [ "$install_asset_url" = "null" ]; then
    return 2
  fi

  install_asset_name=$(basename "$install_asset_url")
  install_archive="$tmpdir/$install_asset_name"

  download_file "$install_asset_url" "$install_archive" "$install_asset_name"
  download_file "$(checksum_url_for_release "$release_version")" "$checksums" "SHA256SUMS"
  verify_checksum "$install_archive" "$checksums" "$install_asset_name"

  install_extract_dir="$tmpdir/extract-$install_target"
  install_bin_file=$(extract_binary "$install_archive" "$install_extract_dir" "$BIN_NAME")
  install_bin "$install_bin_file" "$bin_dir/$BIN_NAME" || err "install" "failed to install binary to $bin_dir/$BIN_NAME"
  install_optional_tray_helper "$install_extract_dir" "$bin_dir"

  INSTALLED_ASSET_URL="$install_asset_url"
  FINAL_EXTRACT_DIR="$install_extract_dir"
}

binary_runs() {
  "$bin_dir/$BIN_NAME" --version >/dev/null 2>&1
}

# Directories on sudo's default secure_path. A binary in one of these is
# reachable by `sudo <bin>` without extra setup.
SECURE_PATH_DIRS="/usr/local/sbin /usr/local/bin /usr/sbin /usr/bin /sbin /bin"

bin_dir_on_secure_path() {
  for secure_dir in $SECURE_PATH_DIRS; do
    [ "$1" = "$secure_dir" ] && return 0
  done
  return 1
}

prompt_sudo_symlink() {
  {
    printf '\nTo run privileged commands like '\''sudo %s setup'\'' the binary must be on\n' "$BIN_NAME"
    printf 'sudo'\''s PATH. Link %s -> %s now? (needs sudo) [y/N] ' "$symlink_link_path" "$symlink_src"
  } 2>/dev/null > /dev/tty || return 1

  IFS= read -r symlink_answer 2>/dev/null < /dev/tty || symlink_answer=
  case "$symlink_answer" in
    y|Y|yes|YES|Yes) decision=yes ;;
    *) decision=no ;;
  esac
}

# A user-level install lands outside sudo's secure_path, so `sudo wakezilla
# setup` fails with "command not found". Offer to symlink the binary into
# /usr/local/bin (the one secure_path dir conventionally meant for local
# installs). The install itself stays sudo-free; only this opt-in step uses it.
offer_sudo_symlink() {
  symlink_bin_dir="$1"
  symlink_link_dir="/usr/local/bin"
  symlink_link_path="$symlink_link_dir/$BIN_NAME"
  symlink_src="$symlink_bin_dir/$BIN_NAME"

  # Root installs already land on secure_path; nothing to link.
  euid="${WAKEZILLA_EUID:-$(id -u 2>/dev/null || printf '')}"
  [ "$euid" = "0" ] && return 0

  # Binary is already reachable by sudo.
  bin_dir_on_secure_path "$symlink_bin_dir" && return 0

  # Without sudo we cannot write into the system path.
  command -v sudo >/dev/null 2>&1 || return 0

  # Already linked where we want it.
  if [ -L "$symlink_link_path" ] && \
     [ "$(readlink "$symlink_link_path" 2>/dev/null)" = "$symlink_src" ]; then
    return 0
  fi

  decision="${WAKEZILLA_SUDO_SYMLINK:-ask}"
  if [ "$decision" = "ask" ]; then
    if ! prompt_sudo_symlink; then
      decision=no
    fi
  fi

  if [ "$decision" != "yes" ]; then
    info "to run privileged commands, use: sudo env \"PATH=\$PATH\" $BIN_NAME ... (or: sudo $symlink_src ...)"
    return 0
  fi

  if sudo ln -sf "$symlink_src" "$symlink_link_path"; then
    info "linked $symlink_link_path -> $symlink_src ('sudo $BIN_NAME' now works)"
  else
    warn "failed to create $symlink_link_path; run privileged commands with: sudo $symlink_src ..."
  fi
}

resolve_linux_integration_user() {
  if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ]; then
    integration_euid="${WAKEZILLA_EUID:-$(id -u 2>/dev/null || printf '')}"
  else
    integration_euid=$(id -u 2>/dev/null || printf '')
  fi
  integration_home=
  integration_uid=
  integration_gid=
  integration_chown=no
  integration_root=no

  if [ "$integration_euid" != "0" ]; then
    case "${HOME:-}" in
      /*) integration_home=$HOME ;;
      *)
        warn "HOME is not an absolute path; skipping Linux desktop integration"
        return 1
        ;;
    esac
    return 0
  fi

  integration_root=yes
  if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ] && \
     { [ -n "${WAKEZILLA_TEST_SUDO_USER:-}" ] || \
       [ -n "${WAKEZILLA_TEST_SUDO_UID:-}" ] || \
       [ -n "${WAKEZILLA_TEST_SUDO_GID:-}" ] || \
       [ -n "${WAKEZILLA_TEST_SUDO_HOME:-}" ]; }; then
    integration_user="${WAKEZILLA_TEST_SUDO_USER:-}"
    integration_uid="${WAKEZILLA_TEST_SUDO_UID:-}"
    integration_gid="${WAKEZILLA_TEST_SUDO_GID:-}"
    integration_home="${WAKEZILLA_TEST_SUDO_HOME:-}"
  else
    integration_user="${SUDO_USER:-}"
    integration_uid="${SUDO_UID:-}"
    integration_gid="${SUDO_GID:-}"
    integration_home=
  fi

  case "$integration_user" in
    ''|-*|*[!A-Za-z0-9_.-]*) integration_user_valid=no ;;
    *) integration_user_valid=yes ;;
  esac
  case "$integration_uid" in
    ''|0|*[!0-9]*) integration_user_valid=no ;;
  esac
  case "$integration_gid" in
    ''|*[!0-9]*) integration_user_valid=no ;;
  esac

  if [ "$integration_user_valid" = "yes" ] && [ -z "$integration_home" ]; then
    integration_actual_uid=$(id -u "$integration_user" 2>/dev/null || printf '')
    integration_actual_gid=$(id -g "$integration_user" 2>/dev/null || printf '')
    if [ "$integration_actual_uid" != "$integration_uid" ] || \
       [ "$integration_actual_gid" != "$integration_gid" ]; then
      integration_user_valid=no
    elif command -v getent >/dev/null 2>&1; then
      integration_home=$(getent passwd "$integration_user" 2>/dev/null | \
        awk -F: -v user="$integration_user" '$1 == user { print $6; exit }')
    elif [ -r /etc/passwd ]; then
      integration_home=$(awk -F: -v user="$integration_user" \
        '$1 == user { print $6; exit }' /etc/passwd)
    fi
  fi

  integration_cr=$(printf '\r')
  case "$integration_home" in
    /*)
      case "$integration_home" in
        *"$integration_cr"*|*"
"*) integration_user_valid=no ;;
      esac
      ;;
    *) integration_user_valid=no ;;
  esac

  if [ "$integration_user_valid" != "yes" ]; then
    warn "running as root without a validated non-root sudo user; skipping Linux desktop integration"
    return 1
  fi

  integration_chown=yes
  return 0
}

desktop_exec_quote() {
  desktop_exec_value="$1"
  desktop_exec_cr=$(printf '\r')
  case "$desktop_exec_value" in
    *"$desktop_exec_cr"*|*"
"*)
      warn "desktop launcher paths must not contain CR or LF"
      return 1
      ;;
  esac

  desktop_exec_escaped=$(printf '%s' "$desktop_exec_value" | sed \
    -e 's/\\/\\\\/g' \
    -e 's/"/\\"/g' \
    -e 's/`/\\`/g' \
    -e 's/\$/\\$/g' \
    -e 's/%/%%/g') || return 1
  printf '"%s"\n' "$desktop_exec_escaped"
}

desktop_string_escape() {
  desktop_string_value="$1"
  desktop_string_cr=$(printf '\r')
  case "$desktop_string_value" in
    *"$desktop_string_cr"*|*"
"*)
      warn "desktop launcher paths must not contain CR or LF"
      return 1
      ;;
  esac
  printf '%s' "$desktop_string_value" | sed -e 's/\\/\\\\/g'
  printf '\n'
}

resolve_linux_desktop_dir() {
  desktop_home="$1"
  desktop_config_home="$2"
  desktop_cr=$(printf '\r')

  if command -v xdg-user-dir >/dev/null 2>&1; then
    desktop_candidate=$(xdg-user-dir DESKTOP 2>/dev/null || printf '')
    case "$desktop_candidate" in
      /*)
        case "$desktop_candidate" in
          *"$desktop_cr"*|*"
"*) ;;
          *)
            if [ -d "$desktop_candidate" ]; then
              printf '%s\n' "$desktop_candidate"
              return 0
            fi
            ;;
        esac
        ;;
    esac
  fi

  desktop_user_dirs="$desktop_config_home/user-dirs.dirs"
  if [ -f "$desktop_user_dirs" ]; then
    while IFS= read -r desktop_line || [ -n "$desktop_line" ]; do
      case "$desktop_line" in
        XDG_DESKTOP_DIR=\"*\")
          desktop_candidate=${desktop_line#XDG_DESKTOP_DIR=\"}
          desktop_candidate=${desktop_candidate%\"}
          case "$desktop_candidate" in
            *'$('*|*'${'*|*'`'*|*"$desktop_cr"*|*"
"*)
              continue
              ;;
            '$HOME')
              desktop_candidate=$desktop_home
              ;;
            '$HOME/'*)
              desktop_suffix=${desktop_candidate#\$HOME}
              desktop_candidate=$desktop_home$desktop_suffix
              ;;
            /*) ;;
            *) continue ;;
          esac
          if [ -d "$desktop_candidate" ]; then
            printf '%s\n' "$desktop_candidate"
            return 0
          fi
          ;;
      esac
    done < "$desktop_user_dirs"
  fi

  if [ -d "$desktop_home/Desktop" ]; then
    printf '%s\n' "$desktop_home/Desktop"
  fi
}

legacy_linux_autostart_is_owned() {
  legacy_entry="$1"
  [ -f "$legacy_entry" ] || return 1
  [ ! -L "$legacy_entry" ] || return 1
  grep -Eq '^Name=Wakezilla( Tray)?[[:space:]]*$' "$legacy_entry" 2>/dev/null || return 1
  grep -Eq "^Exec=.*wakezilla-tray([[:space:]\"']|$)" "$legacy_entry" 2>/dev/null || \
    grep -Eq "^Exec=.*wakezilla[\"']?[[:space:]]+tray([[:space:]]|$)" "$legacy_entry" 2>/dev/null
}

linux_root_profile_path_is_safe() (
  profile_path="$1"
  profile_home="$2"
  profile_cr=$(printf '\r')
  case "$profile_path" in
    "$profile_home"|"$profile_home"/*) ;;
    *) exit 1 ;;
  esac
  profile_suffix=${profile_path#"$profile_home"}
  case "/$profile_suffix/" in
    *"$profile_cr"*|*"
"*|*/../*|*/./*) exit 1 ;;
  esac

  profile_home_physical=$(CDPATH= cd -- "$profile_home" 2>/dev/null && pwd -P) || exit 1
  profile_probe=$profile_path
  while [ ! -e "$profile_probe" ] && [ ! -L "$profile_probe" ]; do
    profile_parent=${profile_probe%/*}
    [ -n "$profile_parent" ] || profile_parent=/
    [ "$profile_parent" != "$profile_probe" ] || exit 1
    profile_probe=$profile_parent
  done
  [ -d "$profile_probe" ] || exit 1
  profile_probe_physical=$(CDPATH= cd -- "$profile_probe" 2>/dev/null && pwd -P) || exit 1
  case "$profile_probe_physical" in
    "$profile_home_physical"|"$profile_home_physical"/*) exit 0 ;;
    *) exit 1 ;;
  esac
)

chown_linux_profile_dirs() (
  profile_path="$1"
  profile_home="$2"
  profile_owner="$3:$4"
  while :; do
    [ -L "$profile_path" ] && exit 1
    if [ -d "$profile_path" ]; then
      chown "$profile_owner" "$profile_path" || exit 1
    fi
    [ "$profile_path" = "$profile_home" ] && exit 0
    profile_parent=${profile_path%/*}
    [ -n "$profile_parent" ] || profile_parent=/
    case "$profile_parent" in
      "$profile_home"|"$profile_home"/*) profile_path=$profile_parent ;;
      *) exit 1 ;;
    esac
  done
)

atomic_install_file() (
  atomic_source="$1"
  atomic_target="$2"
  atomic_mode="$3"
  atomic_uid="${4:-}"
  atomic_gid="${5:-}"
  atomic_dir=$(dirname "$atomic_target") || exit 1
  atomic_name=${atomic_target##*/}
  if [ -d "$atomic_target" ]; then
    exit 1
  fi
  atomic_tmp=$(mktemp "$atomic_dir/.${atomic_name}.tmp.XXXXXX") || exit 1
  trap 'if [ -n "${atomic_tmp:-}" ]; then rm -f "$atomic_tmp"; fi' 0 1 2 15

  cp "$atomic_source" "$atomic_tmp" || exit 1
  chmod "$atomic_mode" "$atomic_tmp" || exit 1
  if [ -n "$atomic_uid" ] && [ -n "$atomic_gid" ]; then
    chown "$atomic_uid:$atomic_gid" "$atomic_tmp" || exit 1
  fi
  mv -f "$atomic_tmp" "$atomic_target" || exit 1
  atomic_tmp=
)

install_linux_desktop_integration() (
  linux_extract_dir="$1"
  linux_bin_dir="$2"
  linux_helper_source="$linux_extract_dir/$TRAY_HELPER_NAME"
  linux_helper="$linux_bin_dir/$TRAY_HELPER_NAME"
  linux_icon_name=dev.wakezilla.Wakezilla.png

  if ! resolve_linux_integration_user; then
    exit 0
  fi

  linux_missing=
  if [ ! -f "$linux_helper_source" ] || [ -L "$linux_helper_source" ] || \
     [ ! -x "$linux_helper_source" ] || [ ! -f "$linux_helper" ] || \
     [ -L "$linux_helper" ] || [ ! -x "$linux_helper" ]; then
    linux_missing=$TRAY_HELPER_NAME
  fi
  for linux_size in 48 128 256; do
    linux_icon_source="$linux_extract_dir/icons/hicolor/${linux_size}x${linux_size}/apps/$linux_icon_name"
    if [ ! -f "$linux_icon_source" ] || [ -L "$linux_icon_source" ]; then
      linux_missing="${linux_missing:+$linux_missing, }${linux_size}x${linux_size} icon"
    fi
  done
  if [ -n "$linux_missing" ]; then
    warn "release archive lacks Linux desktop assets ($linux_missing); skipping desktop integration"
    exit 0
  fi

  if [ "$integration_root" = "yes" ]; then
    linux_data_home="$integration_home/.local/share"
    linux_config_home="$integration_home/.config"
    if [ -n "${XDG_DATA_HOME:-}" ] && \
       linux_root_profile_path_is_safe "$XDG_DATA_HOME" "$integration_home"; then
      linux_data_home=$XDG_DATA_HOME
    fi
    if [ -n "${XDG_CONFIG_HOME:-}" ] && \
       linux_root_profile_path_is_safe "$XDG_CONFIG_HOME" "$integration_home"; then
      linux_config_home=$XDG_CONFIG_HOME
    fi
  else
    case "${XDG_DATA_HOME:-}" in
      /*) linux_data_home=$XDG_DATA_HOME ;;
      *) linux_data_home="$integration_home/.local/share" ;;
    esac
    case "${XDG_CONFIG_HOME:-}" in
      /*) linux_config_home=$XDG_CONFIG_HOME ;;
      *) linux_config_home="$integration_home/.config" ;;
    esac
  fi

  linux_app_dir="$linux_data_home/applications"
  linux_autostart_dir="$linux_config_home/autostart"
  if [ "$integration_root" = "yes" ] && \
     { ! linux_root_profile_path_is_safe "$linux_data_home" "$integration_home" || \
       ! linux_root_profile_path_is_safe "$linux_config_home" "$integration_home"; }; then
    warn "target profile contains an unsafe path; refusing Linux desktop integration"
    exit 1
  fi
  linux_desktop_dir=$(resolve_linux_desktop_dir "$integration_home" "$linux_config_home")
  if [ "$integration_root" = "yes" ]; then
    if [ -n "$linux_desktop_dir" ] && \
       ! linux_root_profile_path_is_safe "$linux_desktop_dir" "$integration_home"; then
      linux_desktop_dir=
    fi
  fi
  linux_stage=$(mktemp -d 2>/dev/null || mktemp -d -t wakezilla-linux-integration) || {
    warn "failed to create a temporary directory for Linux desktop integration"
    exit 1
  }
  trap 'rm -rf "$linux_stage"' 0 1 2 15

  linux_exec=$(desktop_exec_quote "$linux_helper") || exit 1
  linux_try_exec=$(desktop_string_escape "$linux_helper") || exit 1
  {
    printf '%s\n' '[Desktop Entry]'
    printf '%s\n' 'Type=Application'
    printf '%s\n' 'Version=1.0'
    printf '%s\n' 'Name=Wakezilla'
    printf '%s\n' 'Comment=Wakezilla network wake-on-LAN tray application'
    printf 'TryExec=%s\n' "$linux_try_exec"
    printf 'Exec=%s\n' "$linux_exec"
    printf '%s\n' 'Icon=dev.wakezilla.Wakezilla'
    printf '%s\n' 'Terminal=false'
    printf '%s\n' 'Categories=Network;Utility;'
    printf '%s\n' 'StartupNotify=false'
  } > "$linux_stage/application.desktop" || exit 1
  cp "$linux_stage/application.desktop" "$linux_stage/autostart.desktop" || exit 1

  mkdir -p "$linux_app_dir" "$linux_autostart_dir" || {
    warn "failed to create Linux desktop integration directories"
    exit 1
  }
  for linux_size in 48 128 256; do
    mkdir -p "$linux_data_home/icons/hicolor/${linux_size}x${linux_size}/apps" || exit 1
  done

  if [ "$integration_chown" = "yes" ]; then
    chown_linux_profile_dirs "$linux_app_dir" "$integration_home" \
      "$integration_uid" "$integration_gid" || exit 1
    chown_linux_profile_dirs "$linux_data_home/icons/hicolor/48x48/apps" "$integration_home" \
      "$integration_uid" "$integration_gid" || exit 1
    chown_linux_profile_dirs "$linux_data_home/icons/hicolor/128x128/apps" "$integration_home" \
      "$integration_uid" "$integration_gid" || exit 1
    chown_linux_profile_dirs "$linux_data_home/icons/hicolor/256x256/apps" "$integration_home" \
      "$integration_uid" "$integration_gid" || exit 1
    chown_linux_profile_dirs "$linux_autostart_dir" "$integration_home" \
      "$integration_uid" "$integration_gid" || exit 1
  fi

  linux_install_file() {
    if [ "$integration_chown" = "yes" ]; then
      atomic_install_file "$1" "$2" "$3" "$integration_uid" "$integration_gid"
    else
      atomic_install_file "$1" "$2" "$3"
    fi
  }

  linux_install_file "$linux_stage/application.desktop" \
    "$linux_app_dir/dev.wakezilla.Wakezilla.desktop" 0644 || exit 1
  linux_install_file "$linux_stage/autostart.desktop" \
    "$linux_autostart_dir/dev.wakezilla.tray.desktop" 0644 || exit 1
  if [ -n "$linux_desktop_dir" ]; then
    linux_desktop_entry="$linux_desktop_dir/dev.wakezilla.Wakezilla.desktop"
    linux_install_file "$linux_stage/application.desktop" "$linux_desktop_entry" 0755 || exit 1
    if command -v gio >/dev/null 2>&1; then
      gio set "$linux_desktop_entry" metadata::trusted true >/dev/null 2>&1 || true
    fi
  fi
  for linux_size in 48 128 256; do
    linux_install_file \
      "$linux_extract_dir/icons/hicolor/${linux_size}x${linux_size}/apps/$linux_icon_name" \
      "$linux_data_home/icons/hicolor/${linux_size}x${linux_size}/apps/$linux_icon_name" \
      0644 || exit 1
  done

  linux_legacy_entry="$linux_autostart_dir/wakezilla-tray.desktop"
  if legacy_linux_autostart_is_owned "$linux_legacy_entry"; then
    rm -f "$linux_legacy_entry" || exit 1
  fi

  if [ "$integration_root" = "yes" ]; then
    info "Linux desktop integration installed; the Wakezilla tray will start at the next graphical login"
  elif [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
    if command -v nohup >/dev/null 2>&1; then
      (
        nohup "$linux_helper" </dev/null >/dev/null 2>&1 &
      ) || true
    else
      (
        "$linux_helper" </dev/null >/dev/null 2>&1 &
      ) || true
    fi
    info "Linux desktop integration installed; started the Wakezilla tray"
  else
    info "Linux desktop integration installed; the Wakezilla tray will start at the next graphical login"
  fi
)

if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ]; then
  return 0 2>/dev/null || exit 0
fi

parse_args "$@"
check_dependencies
target=$(detect_target)
requested_bin_dir=$(resolve_bin_dir)
bin_dir=$(canonicalize_bin_dir "$requested_bin_dir") || \
  err "install" "failed to create or canonicalize install directory: $requested_bin_dir"

info "installing $BIN_NAME for $target"
json=$(fetch_release_json "${VERSION:-}") || err "download" "failed to fetch release metadata from GitHub"

release_version=$(printf '%s' "$json" | release_version_from_json)

tmpdir=$(mktemp -d 2>/dev/null || mktemp -d -t wakezilla-install)
cleanup() {
  rm -rf "$tmpdir"
}
cleanup_and_exit() {
  cleanup
  exit 1
}
trap cleanup EXIT
trap cleanup_and_exit INT TERM

checksums="$tmpdir/SHA256SUMS"

mkdir -p "$bin_dir" || err "install" "failed to create install directory: $bin_dir"

if ! download_and_install_target "$target"; then
  {
    printf '%s %s does not include a prebuilt binary for %s.\n' "$BIN_NAME" "v$release_version" "$target"
    printf '\nAvailable targets:\n'
    printf '%s' "$json" | available_targets_from_json "$BIN_NAME" | while IFS= read -r available_target; do
      [ -n "$available_target" ] && printf ' - %s\n' "$available_target"
    done
  } >&2
  err "download" "no release asset found for target: $target"
fi

# A gnu (glibc) binary built on a newer toolchain can fail to run on hosts with
# an older glibc -- a common Raspberry Pi case. Fall back to the static musl
# build, which carries no libc version requirement.
if ! binary_runs; then
  fallback_target=$(musl_fallback_target "$target")
  if [ -n "$fallback_target" ] && [ "$fallback_target" != "$target" ]; then
    warn "$BIN_NAME for $target installed but failed to run (likely an incompatible libc); retrying with $fallback_target"
    if download_and_install_target "$fallback_target"; then
      target="$fallback_target"
    else
      warn "no $fallback_target asset available for v$release_version; keeping the $target build"
    fi
  fi
fi

case "$target" in
  *-unknown-linux-gnu|*-unknown-linux-musl)
    if ! install_linux_desktop_integration "$FINAL_EXTRACT_DIR" "$bin_dir"; then
      err "integration" "failed to install Linux desktop integration"
    fi
    ;;
esac

set +e
version_output=$("$bin_dir/$BIN_NAME" --version 2>/dev/null)
version_status=$?
set -e
installed_version=
if [ "$version_status" -eq 0 ]; then
  installed_version=$(printf '%s\n' "$version_output" | awk 'NF { value=$NF } END { print value }')
fi
if [ "$version_status" -eq 0 ] && [ -n "$installed_version" ]; then
  info "installed $BIN_NAME v$installed_version to $bin_dir/$BIN_NAME"
else
  warn "$BIN_NAME installed, but '$BIN_NAME --version' failed or produced no output"
fi

info "resolved $BIN_NAME v$release_version"
info "asset: $INSTALLED_ASSET_URL"
info "install dir: $bin_dir"
path_guidance "$bin_dir"
offer_sudo_symlink "$bin_dir"
