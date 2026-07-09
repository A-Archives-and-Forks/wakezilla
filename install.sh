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

if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ]; then
  return 0 2>/dev/null || exit 0
fi

parse_args "$@"
check_dependencies
target=$(detect_target)
bin_dir=$(resolve_bin_dir)

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
