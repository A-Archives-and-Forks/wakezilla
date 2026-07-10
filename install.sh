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

install_bin() (
  src="$1"
  dst="$2"
  dst_dir=$(dirname "$dst") || exit 1
  dst_name=${dst##*/}
  [ ! -d "$dst" ] || exit 1
  dst_tmp=$(mktemp "$dst_dir/.${dst_name}.install.XXXXXX") || exit 1
  trap 'if [ -n "${dst_tmp:-}" ]; then rm -f "$dst_tmp"; fi' 0 1 2 15

  cp "$src" "$dst_tmp" || exit $?
  chmod 755 "$dst_tmp" || exit $?
  mv -f "$dst_tmp" "$dst" || exit $?
  dst_tmp=
)

validate_tar_archive() (
  archive="$1"
  expected_cli_name="${2:-}"
  expected_helper_name="${3:-}"
  LC_ALL=C
  export LC_ALL
  archive_validation_dir=$(mktemp -d 2>/dev/null || mktemp -d -t wakezilla-archive-validation) || {
    printf '%s\n' 'cannot create private archive validation directory'
    exit 1
  }
  archive_names="$archive_validation_dir/names"
  archive_types="$archive_validation_dir/types"
  archive_validation_error=
  archive_validation_cleanup() {
    rm -rf "$archive_validation_dir"
  }
  trap archive_validation_cleanup 0
  trap 'exit 1' 1 2 15

  if ! TAR_OPTIONS= tar -tzf "$archive" > "$archive_names" 2>/dev/null; then
    archive_validation_error="cannot list archive members"
  elif ! TAR_OPTIONS= tar -tvzf "$archive" > "$archive_types" 2>/dev/null; then
    archive_validation_error="cannot inspect archive member types"
  fi

  if [ -z "$archive_validation_error" ]; then
    while IFS= read -r archive_member || [ -n "$archive_member" ]; do
      if [ -z "$archive_member" ]; then
        archive_validation_error="empty member name"
        break
      fi
      case "$archive_member" in
        /*)
          archive_validation_error="absolute member path: $archive_member"
          break
          ;;
        ..|../*|*/..|*/../*)
          archive_validation_error="parent traversal member path: $archive_member"
          break
          ;;
        *[!A-Za-z0-9._/-]*)
          archive_validation_error="unsupported archive member name"
          break
          ;;
      esac
      if printf '%s' "$archive_member" | LC_ALL=C grep '[[:cntrl:]]' >/dev/null 2>&1; then
        archive_validation_error="control character in member path"
        break
      fi
    done < "$archive_names"
  fi

  if [ -z "$archive_validation_error" ]; then
    if ! archive_identity_error=$(awk \
      -v cli="$expected_cli_name" \
      -v helper="$expected_helper_name" '
      {
        if (++seen[$0] > 1 && duplicate == "") duplicate = $0
        path = $0
        sub(/\/$/, "", path)
        count = split(path, components, "/")
        base = components[count]
        if ($0 !~ /\/$/ && \
            ((cli != "" && base == cli) || (helper != "" && base == helper))) {
          executable_count[base]++
        }
      }
      END {
        if (duplicate != "") {
          print "duplicate archive member: " duplicate
          exit 1
        }
        if (cli != "" && executable_count[cli] > 1) {
          print "ambiguous archive executable: " cli
          exit 1
        }
        if (helper != "" && executable_count[helper] > 1) {
          print "ambiguous archive executable: " helper
          exit 1
        }
      }
    ' "$archive_names"); then
      archive_validation_error=${archive_identity_error:-cannot inspect archive member identities}
    fi
  fi

  if [ -z "$archive_validation_error" ]; then
    while IFS= read -r archive_listing || [ -n "$archive_listing" ]; do
      archive_type=${archive_listing%"${archive_listing#?}"}
      case "$archive_type" in
        -|d) ;;
        *)
          archive_validation_error="unsupported archive member type: $archive_type"
          break
          ;;
      esac
    done < "$archive_types"
  fi

  if [ -z "$archive_validation_error" ]; then
    archive_name_count=$(wc -l < "$archive_names" | tr -d ' ')
    archive_type_count=$(wc -l < "$archive_types" | tr -d ' ')
    if [ "$archive_name_count" != "$archive_type_count" ]; then
      archive_validation_error="archive member listing mismatch"
    fi
  fi

  if [ -n "$archive_validation_error" ]; then
    printf '%s\n' "$archive_validation_error"
    exit 1
  fi
  exit 0
)

extract_binary() {
  archive="$1"
  out_dir="$2"
  bin_name="$3"

  case "$archive" in
    *.tar.gz|*.tgz)
      if ! archive_validation_error=$(validate_tar_archive \
        "$archive" "$bin_name" "$TRAY_HELPER_NAME"); then
        err "extract" "unsafe release archive: $archive_validation_error"
      fi
      mkdir -p "$out_dir"
      TAR_OPTIONS= LC_ALL=C tar -xzf "$archive" -C "$out_dir" || \
        err "extract" "failed to extract $(basename "$archive")"
      ;;
    *)
      err "extract" "unsupported archive format: $(basename "$archive")"
      ;;
  esac

  bin_file=$(find_binary_in_dir "$out_dir" "$bin_name")

  if [ -z "${bin_file:-}" ] || [ ! -f "$bin_file" ] || [ -L "$bin_file" ]; then
    err "binary_lookup" "binary $bin_name not found in downloaded asset"
  fi

  printf '%s\n' "$bin_file"
}

find_binary_in_dir() {
  root_dir="$1"
  bin_name="$2"

  if [ -f "$root_dir/$bin_name" ] && [ ! -L "$root_dir/$bin_name" ]; then
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

  if [ -n "${helper_file:-}" ] && [ -f "$helper_file" ] && [ ! -L "$helper_file" ]; then
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

# Download, verify, extract, and validate an asset without publishing it.
# Returns 2 when the target has no asset, 3 when its CLI cannot report a
# version, and 4 when the reported version does not match the release.
download_and_stage_target() {
  install_target="$1"
  STAGE_REPORTED_VERSION=
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
  install_bin_file=$(extract_binary \
    "$install_archive" "$install_extract_dir" "$BIN_NAME") || return 1

  if install_version_output=$("$install_bin_file" --version 2>/dev/null); then
    install_version_status=0
  else
    install_version_status=$?
  fi
  install_version=$(printf '%s\n' "$install_version_output" | \
    awk 'NF { value=$NF } END { print value }')
  if [ "$install_version_status" -ne 0 ] || [ -z "$install_version" ]; then
    return 3
  fi
  if [ "$install_version" != "$release_version" ]; then
    STAGE_REPORTED_VERSION=$install_version
    return 4
  fi

  install_helper_file=$(find_binary_in_dir "$install_extract_dir" "$TRAY_HELPER_NAME")
  if [ -n "${install_helper_file:-}" ] && \
     { [ ! -f "$install_helper_file" ] || [ -L "$install_helper_file" ]; }; then
    return 1
  fi

  STAGED_BIN_FILE=$install_bin_file
  STAGED_HELPER_FILE=${install_helper_file:-}
  STAGED_EXTRACT_DIR=$install_extract_dir
  STAGED_ASSET_URL=$install_asset_url
  STAGED_VERSION=$install_version
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

path_owner_uid() {
  owner_path="$1"
  owner_uid=
  owner_stat=
  if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ]; then
    owner_stat=$(command -v stat 2>/dev/null || printf '')
  else
    for owner_stat_candidate in /usr/bin/stat /bin/stat; do
      if [ -x "$owner_stat_candidate" ]; then
        owner_stat=$owner_stat_candidate
        break
      fi
    done
  fi
  [ -n "$owner_stat" ] || return 1
  if owner_uid=$("$owner_stat" -f '%u' "$owner_path" 2>/dev/null); then
    :
  elif owner_uid=$("$owner_stat" -c '%u' -- "$owner_path" 2>/dev/null); then
    :
  else
    return 1
  fi
  case "$owner_uid" in
    ''|*[!0-9]*) return 1 ;;
  esac
  printf '%s\n' "$owner_uid"
}

validate_linux_sudo_home() (
  sudo_home="$1"
  sudo_uid="$2"
  [ -d "$sudo_home" ] || exit 1
  [ ! -L "$sudo_home" ] || exit 1
  sudo_home_physical=$(CDPATH= cd -- "$sudo_home" 2>/dev/null && pwd -P) || exit 1
  case "$sudo_home_physical" in
    /|/root|/root/*) exit 1 ;;
  esac
  sudo_home_owner=$(path_owner_uid "$sudo_home_physical") || exit 1
  [ "$sudo_home_owner" = "$sudo_uid" ] || exit 1
  printf '%s\n' "$sudo_home_physical"
)

resolve_linux_integration_user() {
  if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ]; then
    integration_euid="${WAKEZILLA_EUID:-$(id -u 2>/dev/null || printf '')}"
  else
    integration_euid=$(id -u 2>/dev/null || printf '')
  fi
  integration_home=
  integration_uid=
  integration_gid=
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

  if [ "$integration_user_valid" = "yes" ]; then
    if integration_validated_home=$(validate_linux_sudo_home \
      "$integration_home" "$integration_uid"); then
      integration_home=$integration_validated_home
    else
      integration_user_valid=no
    fi
  fi

  if [ "$integration_user_valid" != "yes" ]; then
    warn "running as root without a validated non-root sudo user; skipping Linux desktop integration"
    return 1
  fi

  return 0
}

desktop_exec_quote() {
  desktop_exec_value="$1"
  desktop_exec_cr=$(printf '\r')
  case "$desktop_exec_value" in
    *"$desktop_exec_cr"*|*"
"*)
      warn "desktop launcher paths must not contain ASCII control characters"
      return 1
      ;;
  esac
  if printf '%s' "$desktop_exec_value" | LC_ALL=C grep '[[:cntrl:]]' >/dev/null 2>&1; then
    warn "desktop launcher paths must not contain ASCII control characters"
    return 1
  fi

  desktop_exec_layer=$(printf '%s' "$desktop_exec_value" | sed \
    -e 's/\\/\\\\/g' \
    -e 's/"/\\"/g' \
    -e 's/`/\\`/g' \
    -e 's/\$/\\$/g' \
    -e 's/%/%%/g') || return 1
  desktop_exec_escaped=$(printf '%s' "$desktop_exec_layer" | sed \
    -e 's/\\/\\\\/g') || return 1
  printf '"%s"\n' "$desktop_exec_escaped"
}

desktop_string_escape() {
  desktop_string_value="$1"
  desktop_string_cr=$(printf '\r')
  case "$desktop_string_value" in
    *"$desktop_string_cr"*|*"
"*)
      warn "desktop launcher paths must not contain ASCII control characters"
      return 1
      ;;
  esac
  if printf '%s' "$desktop_string_value" | LC_ALL=C grep '[[:cntrl:]]' >/dev/null 2>&1; then
    warn "desktop launcher paths must not contain ASCII control characters"
    return 1
  fi
  printf '%s' "$desktop_string_value" | sed -e 's/\\/\\\\/g'
  printf '\n'
}

resolve_linux_desktop_dir() {
  desktop_home="$1"
  desktop_config_home="$2"
  desktop_cr=$(printf '\r')
  desktop_home_physical=$(CDPATH= cd -- "$desktop_home" 2>/dev/null && pwd -P) || return 0

  if command -v xdg-user-dir >/dev/null 2>&1; then
    desktop_candidate=$(xdg-user-dir DESKTOP 2>/dev/null || printf '')
    case "$desktop_candidate" in
      /*)
        case "$desktop_candidate" in
          *"$desktop_cr"*|*"
"*) ;;
          *)
            if [ -d "$desktop_candidate" ]; then
              if desktop_candidate_physical=$(CDPATH= cd -- "$desktop_candidate" 2>/dev/null && pwd -P); then
                [ "$desktop_candidate_physical" = "$desktop_home_physical" ] && return 0
                printf '%s\n' "$desktop_candidate"
                return 0
              fi
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
            desktop_candidate_physical=$(CDPATH= cd -- "$desktop_candidate" 2>/dev/null && pwd -P) || continue
            [ "$desktop_candidate_physical" = "$desktop_home_physical" ] && return 0
            printf '%s\n' "$desktop_candidate"
            return 0
          fi
          ;;
      esac
    done < "$desktop_user_dirs"
  fi

  if [ -d "$desktop_home/Desktop" ]; then
    desktop_candidate_physical=$(CDPATH= cd -- "$desktop_home/Desktop" 2>/dev/null && pwd -P) || return 0
    [ "$desktop_candidate_physical" = "$desktop_home_physical" ] && return 0
    printf '%s\n' "$desktop_home/Desktop"
  fi
}

legacy_linux_autostart_is_owned() {
  legacy_entry="$1"
  [ -f "$legacy_entry" ] || return 1
  [ ! -L "$legacy_entry" ] || return 1
  awk '
    function finish_group() {
      if (in_desktop && entry_type && entry_name && entry_exec) found = 1
    }
    function exec_is_owned(value, i, character, escaped, executable, rest, base, fields) {
      sub(/^[ \t]+/, "", value)
      if (substr(value, 1, 1) == "\"") {
        escaped = 0
        for (i = 2; i <= length(value); i++) {
          character = substr(value, i, 1)
          if (escaped) {
            executable = executable character
            escaped = 0
          } else if (character == "\\") {
            escaped = 1
          } else if (character == "\"") {
            rest = substr(value, i + 1)
            break
          } else {
            executable = executable character
          }
        }
        if (i > length(value)) return 0
      } else {
        executable = value
        sub(/[ \t].*$/, "", executable)
        rest = substr(value, length(executable) + 1)
      }
      base = executable
      sub(/^.*\//, "", base)
      if (base == "wakezilla-tray") return 1
      if (base != "wakezilla") return 0
      sub(/^[ \t]+/, "", rest)
      split(rest, fields, /[ \t]+/)
      return fields[1] == "tray"
    }
    /^[ \t]*\[[^]]+\][ \t]*$/ {
      finish_group()
      in_desktop = ($0 ~ /^[ \t]*\[Desktop Entry\][ \t]*$/)
      entry_type = entry_name = entry_exec = 0
      next
    }
    in_desktop {
      separator = index($0, "=")
      if (!separator) next
      key = substr($0, 1, separator - 1)
      value = substr($0, separator + 1)
      gsub(/^[ \t]+|[ \t]+$/, "", key)
      gsub(/^[ \t]+|[ \t]+$/, "", value)
      if (key == "Type") entry_type = (value == "Application")
      else if (key == "Name") entry_name = (value == "Wakezilla" || value == "Wakezilla Tray")
      else if (key == "Exec") entry_exec = exec_is_owned(value)
    }
    END {
      finish_group()
      exit(found ? 0 : 1)
    }
  ' "$legacy_entry" 2>/dev/null
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

atomic_restore_file() (
  atomic_source="$1"
  atomic_target="$2"
  atomic_dir=$(dirname "$atomic_target") || exit 1
  atomic_name=${atomic_target##*/}
  [ ! -d "$atomic_target" ] || exit 1
  atomic_tmp=$(mktemp "$atomic_dir/.${atomic_name}.tmp.XXXXXX") || exit 1
  trap 'if [ -n "${atomic_tmp:-}" ]; then rm -f "$atomic_tmp"; fi' 0 1 2 15

  cp -p "$atomic_source" "$atomic_tmp" || exit 1
  mv -f "$atomic_tmp" "$atomic_target" || exit 1
  atomic_tmp=
)

write_linux_profile_apply_helper() {
  apply_helper_path="$1"
  cat > "$apply_helper_path" <<'APPLY_HELPER'
#!/usr/bin/env sh
set -eu

stage_dir="$1"
data_home="$2"
config_home="$3"
profile_home="$4"
failure_hook="${5:-}"
failure_after_hook="${6:-}"
rollback_failure_hook="${7:-}"
app_id=dev.wakezilla.Wakezilla
icon_name=$app_id.png
app_dir="$data_home/applications"
autostart_dir="$config_home/autostart"

profile_path_is_safe() (
  candidate="$1"
  home="$2"
  cr=$(printf '\r')
  case "$candidate" in
    "$home"|"$home"/*) ;;
    *) exit 1 ;;
  esac
  suffix=${candidate#"$home"}
  case "/$suffix/" in
    *"$cr"*|*"
"*|*/../*|*/./*) exit 1 ;;
  esac
  home_physical=$(CDPATH= cd -- "$home" 2>/dev/null && pwd -P) || exit 1
  probe=$candidate
  while [ ! -e "$probe" ] && [ ! -L "$probe" ]; do
    parent=${probe%/*}
    [ -n "$parent" ] || parent=/
    [ "$parent" != "$probe" ] || exit 1
    probe=$parent
  done
  [ -d "$probe" ] || exit 1
  probe_physical=$(CDPATH= cd -- "$probe" 2>/dev/null && pwd -P) || exit 1
  case "$probe_physical" in
    "$home_physical"|"$home_physical"/*) exit 0 ;;
    *) exit 1 ;;
  esac
)

resolve_desktop_dir() {
  home="$1"
  xdg_config="$2"
  cr=$(printf '\r')
  home_physical=$(CDPATH= cd -- "$home" 2>/dev/null && pwd -P) || return 0
  if command -v xdg-user-dir >/dev/null 2>&1; then
    candidate=$(xdg-user-dir DESKTOP 2>/dev/null || printf '')
    case "$candidate" in
      /*)
        case "$candidate" in
          *"$cr"*|*"
"*) ;;
          *)
            if [ -d "$candidate" ]; then
              if candidate_physical=$(CDPATH= cd -- "$candidate" 2>/dev/null && pwd -P); then
                [ "$candidate_physical" = "$home_physical" ] && return 0
                printf '%s\n' "$candidate"
                return 0
              fi
            fi
            ;;
        esac
        ;;
    esac
  fi
  user_dirs="$xdg_config/user-dirs.dirs"
  if [ -f "$user_dirs" ]; then
    while IFS= read -r line || [ -n "$line" ]; do
      case "$line" in
        XDG_DESKTOP_DIR=\"*\")
          candidate=${line#XDG_DESKTOP_DIR=\"}
          candidate=${candidate%\"}
          case "$candidate" in
            *'$('*|*'${'*|*'`'*|*"$cr"*|*"
"*) continue ;;
            '$HOME') candidate=$home ;;
            '$HOME/'*) suffix=${candidate#\$HOME}; candidate=$home$suffix ;;
            /*) ;;
            *) continue ;;
          esac
          if [ -d "$candidate" ]; then
            candidate_physical=$(CDPATH= cd -- "$candidate" 2>/dev/null && pwd -P) || continue
            [ "$candidate_physical" = "$home_physical" ] && return 0
            printf '%s\n' "$candidate"
            return 0
          fi
          ;;
      esac
    done < "$user_dirs"
  fi
  if [ -d "$home/Desktop" ]; then
    candidate_physical=$(CDPATH= cd -- "$home/Desktop" 2>/dev/null && pwd -P) || return 0
    [ "$candidate_physical" = "$home_physical" ] && return 0
    printf '%s\n' "$home/Desktop"
  fi
  return 0
}

legacy_is_owned() {
  entry="$1"
  [ -f "$entry" ] || return 1
  [ ! -L "$entry" ] || return 1
  awk '
    function finish_group() {
      if (in_desktop && entry_type && entry_name && entry_exec) found = 1
    }
    function exec_is_owned(value, i, character, escaped, executable, rest, base, fields) {
      sub(/^[ \t]+/, "", value)
      if (substr(value, 1, 1) == "\"") {
        escaped = 0
        for (i = 2; i <= length(value); i++) {
          character = substr(value, i, 1)
          if (escaped) { executable = executable character; escaped = 0 }
          else if (character == "\\") escaped = 1
          else if (character == "\"") { rest = substr(value, i + 1); break }
          else executable = executable character
        }
        if (i > length(value)) return 0
      } else {
        executable = value
        sub(/[ \t].*$/, "", executable)
        rest = substr(value, length(executable) + 1)
      }
      base = executable
      sub(/^.*\//, "", base)
      if (base == "wakezilla-tray") return 1
      if (base != "wakezilla") return 0
      sub(/^[ \t]+/, "", rest)
      split(rest, fields, /[ \t]+/)
      return fields[1] == "tray"
    }
    /^[ \t]*\[[^]]+\][ \t]*$/ {
      finish_group()
      in_desktop = ($0 ~ /^[ \t]*\[Desktop Entry\][ \t]*$/)
      entry_type = entry_name = entry_exec = 0
      next
    }
    in_desktop {
      separator = index($0, "=")
      if (!separator) next
      key = substr($0, 1, separator - 1)
      value = substr($0, separator + 1)
      gsub(/^[ \t]+|[ \t]+$/, "", key)
      gsub(/^[ \t]+|[ \t]+$/, "", value)
      if (key == "Type") entry_type = (value == "Application")
      else if (key == "Name") entry_name = (value == "Wakezilla" || value == "Wakezilla Tray")
      else if (key == "Exec") entry_exec = exec_is_owned(value)
    }
    END { finish_group(); exit(found ? 0 : 1) }
  ' "$entry" 2>/dev/null
}

atomic_copy() (
  source_file="$1"
  target_file="$2"
  file_mode="$3"
  target_dir=${target_file%/*}
  target_name=${target_file##*/}
  [ ! -d "$target_file" ] || exit 1
  temp_file=$(mktemp "$target_dir/.${target_name}.tmp.XXXXXX") || exit 1
  trap 'rm -f "$temp_file"' 0 1 2 15
  cp "$source_file" "$temp_file" || exit 1
  chmod "$file_mode" "$temp_file" || exit 1
  mv -f "$temp_file" "$target_file" || exit 1
  temp_file=
)

atomic_restore() (
  source_file="$1"
  target_file="$2"
  target_dir=${target_file%/*}
  target_name=${target_file##*/}
  [ ! -d "$target_file" ] || exit 1
  temp_file=$(mktemp "$target_dir/.${target_name}.tmp.XXXXXX") || exit 1
  trap 'rm -f "$temp_file"' 0 1 2 15
  cp -p "$source_file" "$temp_file" || exit 1
  mv -f "$temp_file" "$target_file" || exit 1
  temp_file=
)

snapshot_target() {
  snapshot_key="$1"
  snapshot_path="$2"
  if [ -e "$snapshot_path" ] || [ -L "$snapshot_path" ]; then
    [ -f "$snapshot_path" ] || return 1
    [ ! -L "$snapshot_path" ] || return 1
    cp -p "$snapshot_path" "$snapshot_dir/$snapshot_key.file" || return 1
    : > "$snapshot_dir/$snapshot_key.present" || return 1
  else
    : > "$snapshot_dir/$snapshot_key.missing" || return 1
  fi
}

restore_target() {
  restore_key="$1"
  restore_path="$2"
  [ "$rollback_failure_hook" != "$restore_key" ] || return 1
  if [ -f "$snapshot_dir/$restore_key.present" ]; then
    atomic_restore "$snapshot_dir/$restore_key.file" "$restore_path"
  else
    rm -f "$restore_path"
  fi
}

publish_target() {
  publish_key="$1"
  publish_source="$2"
  publish_path="$3"
  publish_mode="$4"
  touched_targets="$publish_key $touched_targets"
  [ "$failure_hook" != "$publish_key" ] || return 1
  atomic_copy "$publish_source" "$publish_path" "$publish_mode" || return 1
  [ "$failure_after_hook" != "$publish_key" ] || return 1
}

rollback_profile() {
  rollback_failed=no
  set +e
  for rollback_key in $touched_targets; do
    case "$rollback_key" in
      icon48) restore_target icon48 "$icon_48_target" || rollback_failed=yes ;;
      icon128) restore_target icon128 "$icon_128_target" || rollback_failed=yes ;;
      icon256) restore_target icon256 "$icon_256_target" || rollback_failed=yes ;;
      application) restore_target application "$application_target" || rollback_failed=yes ;;
      desktop) restore_target desktop "$desktop_target" || rollback_failed=yes ;;
      legacy) restore_target legacy "$legacy_entry" || rollback_failed=yes ;;
      autostart) restore_target autostart "$autostart_target" || rollback_failed=yes ;;
    esac
  done
  set -e
  [ "$rollback_failed" = no ]
}

transaction_cleanup() {
  transaction_status=$?
  trap - 0 1 2 15
  if [ "${transaction_active:-no}" = yes ]; then
    printf '%s\n' 'warning: Linux desktop integration failed; rolling back profile files' >&2
    if ! rollback_profile; then
      printf '%s\n' 'warning: profile rollback incomplete; manual recovery may be required' >&2
      transaction_status=1
    fi
  fi
  exit "$transaction_status"
}

for staged_file in application.desktop autostart.desktop icon-48.png icon-128.png icon-256.png; do
  [ -f "$stage_dir/$staged_file" ] || exit 1
  [ ! -L "$stage_dir/$staged_file" ] || exit 1
done

for destination_dir in \
  "$app_dir" \
  "$autostart_dir" \
  "$data_home/icons/hicolor/48x48/apps" \
  "$data_home/icons/hicolor/128x128/apps" \
  "$data_home/icons/hicolor/256x256/apps"; do
  profile_path_is_safe "$destination_dir" "$profile_home" || exit 1
done

desktop_dir=$(resolve_desktop_dir "$profile_home" "$config_home")
if [ -n "$desktop_dir" ] && ! profile_path_is_safe "$desktop_dir" "$profile_home"; then
  desktop_dir=
fi

umask 077
mkdir -p "$app_dir" "$autostart_dir" \
  "$data_home/icons/hicolor/48x48/apps" \
  "$data_home/icons/hicolor/128x128/apps" \
  "$data_home/icons/hicolor/256x256/apps"

for destination_dir in \
  "$app_dir" \
  "$autostart_dir" \
  "$data_home/icons/hicolor/48x48/apps" \
  "$data_home/icons/hicolor/128x128/apps" \
  "$data_home/icons/hicolor/256x256/apps"; do
  profile_path_is_safe "$destination_dir" "$profile_home" || exit 1
done

application_target="$app_dir/$app_id.desktop"
autostart_target="$autostart_dir/dev.wakezilla.tray.desktop"
icon_48_target="$data_home/icons/hicolor/48x48/apps/$icon_name"
icon_128_target="$data_home/icons/hicolor/128x128/apps/$icon_name"
icon_256_target="$data_home/icons/hicolor/256x256/apps/$icon_name"
desktop_target=
[ -z "$desktop_dir" ] || desktop_target="$desktop_dir/$app_id.desktop"
legacy_entry="$autostart_dir/wakezilla-tray.desktop"

snapshot_dir="$stage_dir/.rollback"
mkdir -p "$snapshot_dir"
snapshot_target icon48 "$icon_48_target"
snapshot_target icon128 "$icon_128_target"
snapshot_target icon256 "$icon_256_target"
snapshot_target application "$application_target"
[ -z "$desktop_target" ] || snapshot_target desktop "$desktop_target"
snapshot_target legacy "$legacy_entry"
snapshot_target autostart "$autostart_target"

touched_targets=
transaction_active=yes
trap transaction_cleanup 0
trap 'exit 1' 1 2 15

publish_target icon48 "$stage_dir/icon-48.png" "$icon_48_target" 0644
publish_target icon128 "$stage_dir/icon-128.png" "$icon_128_target" 0644
publish_target icon256 "$stage_dir/icon-256.png" "$icon_256_target" 0644
publish_target application "$stage_dir/application.desktop" "$application_target" 0644
if [ -n "$desktop_target" ]; then
  publish_target desktop "$stage_dir/application.desktop" "$desktop_target" 0755
fi
if legacy_is_owned "$legacy_entry"; then
  touched_targets="legacy $touched_targets"
  rm -f "$legacy_entry"
fi
publish_target autostart "$stage_dir/autostart.desktop" "$autostart_target" 0644
transaction_active=no

if [ -n "$desktop_target" ] && command -v gio >/dev/null 2>&1; then
  gio set "$desktop_target" metadata::trusted true >/dev/null 2>&1 || true
fi
APPLY_HELPER
  chmod 0700 "$apply_helper_path"
}

apply_linux_profile_as_user() {
  apply_stage="$1"
  apply_data_home="$2"
  apply_config_home="$3"
  apply_home="$4"
  apply_user="$5"
  apply_uid="$6"
  apply_gid="$7"
  apply_failure_hook="${8:-}"
  apply_failure_after_hook="${9:-}"
  apply_rollback_failure_hook="${10:-}"

  apply_chown=
  apply_env=
  apply_shell=
  apply_runner=
  apply_runner_kind=
  if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ]; then
    apply_chown=$(command -v chown 2>/dev/null || printf '')
    apply_env=$(command -v env 2>/dev/null || printf '')
    apply_shell=$(command -v sh 2>/dev/null || printf '')
    if [ -z "${WAKEZILLA_TEST_NO_PRIVILEGE_RUNNER:-}" ]; then
      if apply_runner=$(command -v sudo 2>/dev/null); then
        apply_runner_kind=sudo
      elif apply_runner=$(command -v runuser 2>/dev/null); then
        apply_runner_kind=runuser
      fi
    fi
  else
    for apply_chown_candidate in /usr/sbin/chown /usr/bin/chown /bin/chown; do
      if [ -x "$apply_chown_candidate" ]; then
        apply_chown=$apply_chown_candidate
        break
      fi
    done
    [ -x /usr/bin/env ] && apply_env=/usr/bin/env
    [ -x /bin/sh ] && apply_shell=/bin/sh
    if [ -x /usr/bin/sudo ]; then
      apply_runner=/usr/bin/sudo
      apply_runner_kind=sudo
    elif [ -x /usr/sbin/runuser ]; then
      apply_runner=/usr/sbin/runuser
      apply_runner_kind=runuser
    elif [ -x /usr/bin/runuser ]; then
      apply_runner=/usr/bin/runuser
      apply_runner_kind=runuser
    fi
  fi

  if [ -z "$apply_chown" ] || [ -z "$apply_env" ] || [ -z "$apply_shell" ]; then
    warn "cannot apply Linux desktop integration as $apply_user: required system tools are unavailable"
    return 125
  fi
  "$apply_chown" -R "$apply_uid:$apply_gid" "$apply_stage" || return 1
  if [ "$apply_runner_kind" = "sudo" ]; then
    "$apply_runner" -u "$apply_user" -- \
      "$apply_env" -i \
      HOME="$apply_home" \
      XDG_DATA_HOME="$apply_data_home" \
      XDG_CONFIG_HOME="$apply_config_home" \
      PATH=/usr/local/bin:/usr/bin:/bin \
      "$apply_shell" "$apply_stage/apply-profile.sh" \
      "$apply_stage" "$apply_data_home" "$apply_config_home" "$apply_home" \
      "$apply_failure_hook" "$apply_failure_after_hook" \
      "$apply_rollback_failure_hook"
    return $?
  fi
  if [ "$apply_runner_kind" = "runuser" ]; then
    "$apply_runner" -u "$apply_user" -- \
      "$apply_env" -i \
      HOME="$apply_home" \
      XDG_DATA_HOME="$apply_data_home" \
      XDG_CONFIG_HOME="$apply_config_home" \
      PATH=/usr/local/bin:/usr/bin:/bin \
      "$apply_shell" "$apply_stage/apply-profile.sh" \
      "$apply_stage" "$apply_data_home" "$apply_config_home" "$apply_home" \
      "$apply_failure_hook" "$apply_failure_after_hook" \
      "$apply_rollback_failure_hook"
    return $?
  fi
  warn "cannot apply Linux desktop integration as $apply_user: sudo or runuser is unavailable"
  return 125
}

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
  linux_stage=$(mktemp -d 2>/dev/null || mktemp -d -t wakezilla-linux-integration) || {
    warn "failed to create a temporary directory for Linux desktop integration"
    exit 1
  }
  linux_transaction_active=no
  linux_touched_targets=
  linux_integration_cleanup() {
    linux_cleanup_status=$?
    trap - 0 1 2 15
    if [ "${linux_transaction_active:-no}" = yes ]; then
      warn "Linux desktop integration failed; rolling back profile files"
      if ! linux_rollback_profile; then
        warn "profile rollback incomplete; manual recovery may be required"
        linux_cleanup_status=1
      fi
    fi
    rm -rf "$linux_stage"
    exit "$linux_cleanup_status"
  }
  trap linux_integration_cleanup 0
  trap 'exit 1' 1 2 15

  linux_exec=$(desktop_exec_quote "$linux_helper") || exit 1
  linux_try_exec=$(desktop_string_escape "$linux_helper") || exit 1
  {
    printf '%s\n' '[Desktop Entry]'
    printf '%s\n' 'Type=Application'
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

  if [ "$integration_root" = "yes" ]; then
    linux_failure_hook=
    linux_failure_after_hook=
    linux_rollback_failure_hook=
    if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ]; then
      linux_failure_hook=${WAKEZILLA_TEST_FAIL_LINUX_INTEGRATION:-}
      linux_failure_after_hook=${WAKEZILLA_TEST_FAIL_LINUX_INTEGRATION_AFTER:-}
      linux_rollback_failure_hook=${WAKEZILLA_TEST_FAIL_LINUX_ROLLBACK:-}
    fi
    for linux_size in 48 128 256; do
      cp "$linux_extract_dir/icons/hicolor/${linux_size}x${linux_size}/apps/$linux_icon_name" \
        "$linux_stage/icon-$linux_size.png" || exit 1
      chmod 0644 "$linux_stage/icon-$linux_size.png" || exit 1
    done
    write_linux_profile_apply_helper "$linux_stage/apply-profile.sh" || exit 1
    set +e
    apply_linux_profile_as_user \
      "$linux_stage" \
      "$linux_data_home" \
      "$linux_config_home" \
      "$integration_home" \
      "$integration_user" \
      "$integration_uid" \
      "$integration_gid" \
      "$linux_failure_hook" \
      "$linux_failure_after_hook" \
      "$linux_rollback_failure_hook"
    linux_apply_status=$?
    set -e
    case "$linux_apply_status" in
      0)
        info "Linux desktop integration installed; the Wakezilla tray will start at the next graphical login"
        exit 0
        ;;
      *) exit "$linux_apply_status" ;;
    esac
  fi

  linux_desktop_dir=$(resolve_linux_desktop_dir "$integration_home" "$linux_config_home")

  umask 077
  mkdir -p "$linux_app_dir" "$linux_autostart_dir" || {
    warn "failed to create Linux desktop integration directories"
    exit 1
  }
  for linux_size in 48 128 256; do
    mkdir -p "$linux_data_home/icons/hicolor/${linux_size}x${linux_size}/apps" || exit 1
  done

  linux_application_target="$linux_app_dir/dev.wakezilla.Wakezilla.desktop"
  linux_autostart_target="$linux_autostart_dir/dev.wakezilla.tray.desktop"
  linux_desktop_target=
  [ -z "$linux_desktop_dir" ] || \
    linux_desktop_target="$linux_desktop_dir/dev.wakezilla.Wakezilla.desktop"
  linux_icon_48_target="$linux_data_home/icons/hicolor/48x48/apps/$linux_icon_name"
  linux_icon_128_target="$linux_data_home/icons/hicolor/128x128/apps/$linux_icon_name"
  linux_icon_256_target="$linux_data_home/icons/hicolor/256x256/apps/$linux_icon_name"
  linux_legacy_entry="$linux_autostart_dir/wakezilla-tray.desktop"
  linux_snapshot_dir="$linux_stage/.rollback"
  mkdir -p "$linux_snapshot_dir" || exit 1

  linux_snapshot_target() {
    linux_snapshot_key="$1"
    linux_snapshot_path="$2"
    if [ -e "$linux_snapshot_path" ] || [ -L "$linux_snapshot_path" ]; then
      [ -f "$linux_snapshot_path" ] || return 1
      [ ! -L "$linux_snapshot_path" ] || return 1
      cp -p "$linux_snapshot_path" "$linux_snapshot_dir/$linux_snapshot_key.file" || return 1
      : > "$linux_snapshot_dir/$linux_snapshot_key.present" || return 1
    else
      : > "$linux_snapshot_dir/$linux_snapshot_key.missing" || return 1
    fi
  }

  linux_restore_target() {
    linux_restore_key="$1"
    linux_restore_path="$2"
    if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ] && \
       [ "${WAKEZILLA_TEST_FAIL_LINUX_ROLLBACK:-}" = "$linux_restore_key" ]; then
      return 1
    fi
    if [ -f "$linux_snapshot_dir/$linux_restore_key.present" ]; then
      atomic_restore_file "$linux_snapshot_dir/$linux_restore_key.file" "$linux_restore_path"
    else
      rm -f "$linux_restore_path"
    fi
  }

  linux_rollback_profile() {
    linux_rollback_failed=no
    set +e
    for linux_rollback_key in $linux_touched_targets; do
      case "$linux_rollback_key" in
        icon48) linux_restore_target icon48 "$linux_icon_48_target" || linux_rollback_failed=yes ;;
        icon128) linux_restore_target icon128 "$linux_icon_128_target" || linux_rollback_failed=yes ;;
        icon256) linux_restore_target icon256 "$linux_icon_256_target" || linux_rollback_failed=yes ;;
        application) linux_restore_target application "$linux_application_target" || linux_rollback_failed=yes ;;
        desktop) linux_restore_target desktop "$linux_desktop_target" || linux_rollback_failed=yes ;;
        legacy) linux_restore_target legacy "$linux_legacy_entry" || linux_rollback_failed=yes ;;
        autostart) linux_restore_target autostart "$linux_autostart_target" || linux_rollback_failed=yes ;;
      esac
    done
    set -e
    [ "$linux_rollback_failed" = no ]
  }

  linux_publish_target() {
    linux_publish_key="$1"
    linux_publish_source="$2"
    linux_publish_path="$3"
    linux_publish_mode="$4"
    linux_touched_targets="$linux_publish_key $linux_touched_targets"
    if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ] && \
       [ "${WAKEZILLA_TEST_FAIL_LINUX_INTEGRATION:-}" = "$linux_publish_key" ]; then
      return 1
    fi
    atomic_install_file "$linux_publish_source" "$linux_publish_path" "$linux_publish_mode" || return 1
    if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ] && \
       [ "${WAKEZILLA_TEST_FAIL_LINUX_INTEGRATION_AFTER:-}" = "$linux_publish_key" ]; then
      return 1
    fi
  }

  linux_snapshot_target icon48 "$linux_icon_48_target" || exit 1
  linux_snapshot_target icon128 "$linux_icon_128_target" || exit 1
  linux_snapshot_target icon256 "$linux_icon_256_target" || exit 1
  linux_snapshot_target application "$linux_application_target" || exit 1
  [ -z "$linux_desktop_target" ] || \
    linux_snapshot_target desktop "$linux_desktop_target" || exit 1
  linux_snapshot_target legacy "$linux_legacy_entry" || exit 1
  linux_snapshot_target autostart "$linux_autostart_target" || exit 1

  linux_transaction_active=yes
  linux_publish_target icon48 \
    "$linux_extract_dir/icons/hicolor/48x48/apps/$linux_icon_name" \
    "$linux_icon_48_target" 0644 || exit 1
  linux_publish_target icon128 \
    "$linux_extract_dir/icons/hicolor/128x128/apps/$linux_icon_name" \
    "$linux_icon_128_target" 0644 || exit 1
  linux_publish_target icon256 \
    "$linux_extract_dir/icons/hicolor/256x256/apps/$linux_icon_name" \
    "$linux_icon_256_target" 0644 || exit 1
  linux_publish_target application "$linux_stage/application.desktop" \
    "$linux_application_target" 0644 || exit 1
  if [ -n "$linux_desktop_target" ]; then
    linux_publish_target desktop "$linux_stage/application.desktop" \
      "$linux_desktop_target" 0755 || exit 1
  fi
  if legacy_linux_autostart_is_owned "$linux_legacy_entry"; then
    linux_touched_targets="legacy $linux_touched_targets"
    rm -f "$linux_legacy_entry" || exit 1
  fi
  linux_publish_target autostart "$linux_stage/autostart.desktop" \
    "$linux_autostart_target" 0644 || exit 1
  linux_transaction_active=no

  if [ -n "$linux_desktop_target" ] && command -v gio >/dev/null 2>&1; then
    gio set "$linux_desktop_target" metadata::trusted true >/dev/null 2>&1 || true
  fi

  if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
    if command -v nohup >/dev/null 2>&1; then
      (
        nohup "$linux_helper" </dev/null >/dev/null 2>&1 &
      ) || true
    else
      (
        "$linux_helper" </dev/null >/dev/null 2>&1 &
      ) || true
    fi
    info "Linux desktop integration installed; Wakezilla tray launch requested"
  else
    info "Linux desktop integration installed; the Wakezilla tray will start at the next graphical login"
  fi
)

macos_bundle_versions() (
  macos_release_version="$1"
  macos_without_build=${macos_release_version%%+*}
  macos_numeric_version=${macos_without_build%%-*}
  macos_prerelease=
  case "$macos_without_build" in
    *-*) macos_prerelease=${macos_without_build#*-} ;;
  esac

  macos_old_ifs=$IFS
  IFS=.
  set -- $macos_numeric_version
  IFS=$macos_old_ifs
  [ "$#" -eq 3 ] || exit 1
  for macos_component do
    case "$macos_component" in
      ''|*[!0-9]*) exit 1 ;;
    esac
  done
  macos_short_version="$1.$2.$3"
  macos_bundle_version=$macos_short_version

  if [ -n "$macos_prerelease" ]; then
    macos_prerelease_name=${macos_prerelease%%.*}
    macos_prerelease_tail=
    case "$macos_prerelease" in
      *.*) macos_prerelease_tail=${macos_prerelease#*.} ;;
    esac
    macos_prerelease_name=$(printf '%s' "$macos_prerelease_name" | tr '[:upper:]' '[:lower:]')
    case "$macos_prerelease_name" in
      a|alpha) macos_prerelease_suffix=a ;;
      b|beta) macos_prerelease_suffix=b ;;
      rc) macos_prerelease_suffix=fc ;;
      *) macos_prerelease_suffix=d ;;
    esac
    macos_prerelease_number=${macos_prerelease_tail%%.*}
    case "$macos_prerelease_number" in
      ''|*[!0-9]*) macos_prerelease_number=1 ;;
    esac
    if [ "$macos_prerelease_number" -lt 1 ] || \
       [ "$macos_prerelease_number" -gt 255 ]; then
      macos_prerelease_number=1
    fi
    macos_bundle_version="${macos_short_version}${macos_prerelease_suffix}${macos_prerelease_number}"
  fi

  printf '%s\n%s\n' "$macos_short_version" "$macos_bundle_version"
)

macos_xml_escape() {
  macos_xml_value="$1"
  macos_xml_cr=$(printf '\r')
  case "$macos_xml_value" in
    *"$macos_xml_cr"*|*"
"*) return 1 ;;
  esac
  if printf '%s' "$macos_xml_value" | LC_ALL=C grep '[[:cntrl:]]' >/dev/null 2>&1; then
    return 1
  fi
  printf '%s' "$macos_xml_value" | sed \
    -e 's/&/\&amp;/g' \
    -e 's/</\&lt;/g' \
    -e 's/>/\&gt;/g' \
    -e 's/"/\&quot;/g' \
    -e "s/'/\&apos;/g"
  printf '\n'
}

write_macos_info_plist() {
  macos_info_path="$1"
  macos_short_version="$2"
  macos_bundle_version="$3"
  cat > "$macos_info_path" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>Wakezilla</string>
  <key>CFBundleExecutable</key>
  <string>wakezilla-tray</string>
  <key>CFBundleIconFile</key>
  <string>Wakezilla.icns</string>
  <key>CFBundleIdentifier</key>
  <string>dev.wakezilla.Wakezilla</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Wakezilla</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$macos_short_version</string>
  <key>CFBundleVersion</key>
  <string>$macos_bundle_version</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.utilities</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF
  chmod 0644 "$macos_info_path"
}

write_macos_launch_agent_plist() {
  macos_agent_path="$1"
  macos_app_path="$2"
  macos_escaped_app=$(macos_xml_escape "$macos_app_path") || return 1
  cat > "$macos_agent_path" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>dev.wakezilla.tray</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/bin/open</string>
    <string>-g</string>
    <string>$macos_escaped_app</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>LimitLoadToSessionType</key>
  <string>Aqua</string>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>AssociatedBundleIdentifiers</key>
  <array>
    <string>dev.wakezilla.Wakezilla</string>
  </array>
</dict>
</plist>
EOF
  chmod 0644 "$macos_agent_path"
}

macos_launchctl_failure_is_absent() {
  macos_absent_status="$1"
  macos_absent_output="$2"
  [ "$macos_absent_status" -eq 113 ] && return 0
  case "$macos_absent_output" in
    *'Could not find service'*|*'Could not find domain'*|*'No such process'*) return 0 ;;
  esac
  return 1
}

macos_path_identity() {
  macos_identity_path="$1"
  macos_identity_stat=
  if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ]; then
    macos_identity_stat=$(command -v stat 2>/dev/null || printf '')
  else
    for macos_identity_candidate in /usr/bin/stat /bin/stat; do
      if [ -x "$macos_identity_candidate" ]; then
        macos_identity_stat=$macos_identity_candidate
        break
      fi
    done
  fi
  [ -n "$macos_identity_stat" ] || return 1
  if macos_identity_value=$("$macos_identity_stat" -f '%d:%i' \
    "$macos_identity_path" 2>/dev/null); then
    :
  elif macos_identity_value=$("$macos_identity_stat" -c '%d:%i' -- \
    "$macos_identity_path" 2>/dev/null); then
    :
  else
    return 1
  fi
  case "$macos_identity_value" in
    *[!0-9:]*|:*|*:|*:*:*) return 1 ;;
  esac
  printf '%s\n' "$macos_identity_value"
}

macos_directory_has_identity() {
  macos_identity_directory="$1"
  macos_expected_identity="$2"
  [ -n "$macos_expected_identity" ] && \
    [ -d "$macos_identity_directory" ] && [ ! -L "$macos_identity_directory" ] || return 1
  macos_actual_identity=$(macos_path_identity "$macos_identity_directory") || return 1
  [ "$macos_actual_identity" = "$macos_expected_identity" ]
}

macos_file_has_identity() {
  macos_identity_file="$1"
  macos_expected_identity="$2"
  [ -n "$macos_expected_identity" ] && \
    [ -f "$macos_identity_file" ] && [ ! -L "$macos_identity_file" ] || return 1
  macos_actual_identity=$(macos_path_identity "$macos_identity_file") || return 1
  [ "$macos_actual_identity" = "$macos_expected_identity" ]
}

validate_macos_home_for_uid() (
  macos_validate_home="$1"
  macos_validate_uid="$2"
  case "$macos_validate_uid" in
    0|''|*[!0-9]*) exit 1 ;;
  esac
  case "$macos_validate_home" in
    /*) ;;
    *) exit 1 ;;
  esac
  [ -d "$macos_validate_home" ] && [ ! -L "$macos_validate_home" ] || exit 1
  macos_validate_physical=$(CDPATH= cd -- "$macos_validate_home" 2>/dev/null && pwd -P) || \
    exit 1
  macos_validate_owner=$(path_owner_uid "$macos_validate_physical") || exit 1
  [ "$macos_validate_owner" = "$macos_validate_uid" ] || exit 1
  printf '%s\n' "$macos_validate_physical"
)

install_macos_desktop_integration_at() (
  macos_extract_dir="$1"
  macos_bin_dir="$2"
  macos_release_version="$3"
  macos_home="$4"
  macos_uid="$5"
  macos_plutil="$6"
  macos_launchctl="$7"

  if [ "$macos_uid" = 0 ]; then
    warn "macOS desktop installation must run without sudo; rerun as the target user"
    exit 1
  fi
  macos_home_physical=$(validate_macos_home_for_uid "$macos_home" "$macos_uid") || {
    warn "HOME must be absolute, physical, non-symlink, and owned by the current macOS user"
    exit 1
  }
  case "$macos_bin_dir" in
    /*) ;;
    *) warn "BIN_DIR must be absolute for macOS desktop installation"; exit 1 ;;
  esac
  [ -d "$macos_bin_dir" ] && [ ! -L "$macos_bin_dir" ] || {
    warn "BIN_DIR must be a real directory for macOS desktop installation"
    exit 1
  }
  macos_bin_physical=$(CDPATH= cd -- "$macos_bin_dir" 2>/dev/null && pwd -P) || exit 1
  macos_bin_owner=$(path_owner_uid "$macos_bin_physical") || {
    warn "cannot verify BIN_DIR ownership for macOS desktop installation"
    exit 1
  }
  [ "$macos_bin_owner" = "$macos_uid" ] || {
    warn "BIN_DIR is not owned by the current macOS user"
    exit 1
  }
  [ -x "$macos_plutil" ] && [ -x "$macos_launchctl" ] || {
    warn "plutil and launchctl are required for macOS desktop installation"
    exit 1
  }

  macos_cli_source="$macos_extract_dir/wakezilla"
  macos_tray_source="$macos_extract_dir/wakezilla-tray"
  macos_icon_source="$macos_extract_dir/Wakezilla.icns"
  for macos_executable_source in "$macos_cli_source" "$macos_tray_source"; do
    [ -f "$macos_executable_source" ] && [ ! -L "$macos_executable_source" ] && \
      [ -x "$macos_executable_source" ] || {
      warn "release archive lacks a regular executable macOS bundle binary"
      exit 1
    }
  done
  [ -f "$macos_icon_source" ] && [ ! -L "$macos_icon_source" ] || {
    warn "release archive lacks Wakezilla.icns"
    exit 1
  }

  macos_versions=$(macos_bundle_versions "$macos_release_version") || {
    warn "release version cannot be represented in a macOS bundle"
    exit 1
  }
  macos_short_version=$(printf '%s\n' "$macos_versions" | sed -n '1p')
  macos_bundle_version=$(printf '%s\n' "$macos_versions" | sed -n '2p')
  macos_applications="$macos_home_physical/Applications"
  macos_launch_agents="$macos_home_physical/Library/LaunchAgents"
  macos_app="$macos_applications/Wakezilla.app"
  macos_agent="$macos_launch_agents/dev.wakezilla.tray.plist"
  macos_cli_link="$macos_bin_physical/wakezilla"
  macos_loose_helper="$macos_bin_physical/wakezilla-tray"

  macos_validate_profile_directory() {
    macos_profile_directory="$1"
    [ -d "$macos_profile_directory" ] && [ ! -L "$macos_profile_directory" ] || return 1
    macos_profile_physical=$(CDPATH= cd -- "$macos_profile_directory" 2>/dev/null && pwd -P) || \
      return 1
    case "$macos_profile_physical" in
      "$macos_home_physical"/*) ;;
      *) return 1 ;;
    esac
    macos_profile_owner=$(path_owner_uid "$macos_profile_physical") || return 1
    [ "$macos_profile_owner" = "$macos_uid" ]
  }

  for macos_existing_profile_directory in \
    "$macos_applications" "$macos_home_physical/Library" "$macos_launch_agents"; do
    if [ -e "$macos_existing_profile_directory" ] || \
       [ -L "$macos_existing_profile_directory" ]; then
      macos_validate_profile_directory "$macos_existing_profile_directory" || {
        warn "refusing unsafe macOS profile directory: $macos_existing_profile_directory"
        exit 1
      }
    fi
  done

  if [ -L "$macos_app" ]; then
    warn "refusing symlink destination at $macos_app"
    exit 1
  fi
  macos_existing_bundle=no
  macos_existing_bundle_identity=
  if [ -e "$macos_app" ]; then
    [ -d "$macos_app" ] || {
      warn "refusing non-bundle destination at $macos_app"
      exit 1
    }
    macos_existing_info="$macos_app/Contents/Info.plist"
    [ -f "$macos_existing_info" ] && [ ! -L "$macos_existing_info" ] || {
      warn "refusing unidentifiable existing Wakezilla.app"
      exit 1
    }
    if macos_existing_id=$("$macos_plutil" -extract CFBundleIdentifier raw -o - \
      "$macos_existing_info" 2>/dev/null); then
      :
    else
      warn "refusing unidentifiable existing Wakezilla.app"
      exit 1
    fi
    [ "$macos_existing_id" = dev.wakezilla.Wakezilla ] || {
      warn "refusing foreign existing Wakezilla.app"
      exit 1
    }
    macos_existing_bundle_identity=$(macos_path_identity "$macos_app") || {
      warn "cannot identify existing Wakezilla.app"
      exit 1
    }
    macos_existing_bundle=yes
  fi

  for macos_leaf_target in "$macos_cli_link" "$macos_loose_helper" "$macos_agent"; do
    if [ -e "$macos_leaf_target" ] && [ ! -f "$macos_leaf_target" ] && \
       [ ! -L "$macos_leaf_target" ]; then
      warn "refusing non-file macOS integration destination: $macos_leaf_target"
      exit 1
    fi
  done
  if [ -L "$macos_agent" ]; then
    warn "refusing symlink LaunchAgent destination at $macos_agent"
    exit 1
  fi

  macos_state_dir=
  macos_state_dir_identity=
  macos_stage_bundle=
  macos_stage_bundle_identity=
  macos_bundle_nested_stage=
  macos_stage_agent=
  macos_stage_agent_identity=
  macos_agent_nested_stage=
  macos_backup_bundle=
  macos_backup_placeholder_identity=
  macos_lock_dir="$macos_home_physical/.wakezilla-macos-install.lock"
  macos_lock_identity=
  umask 077
  if ! mkdir "$macos_lock_dir" 2>/dev/null; then
    warn "another macOS Wakezilla installation is already in progress"
    exit 1
  fi
  macos_lock_identity=$(macos_path_identity "$macos_lock_dir") || {
    warn "cannot identify the macOS installer lock"
    rmdir "$macos_lock_dir" 2>/dev/null || true
    exit 1
  }
  macos_lock_is_owned() {
    macos_directory_has_identity "$macos_lock_dir" "$macos_lock_identity"
  }
  macos_release_lock() {
    if macos_lock_is_owned; then
      rmdir "$macos_lock_dir"
    elif [ -e "$macos_lock_dir" ] || [ -L "$macos_lock_dir" ]; then
      return 1
    fi
  }
  macos_setup_cleanup() {
    macos_setup_status=$?
    trap - 0 1 2 15
    if [ -n "${macos_stage_bundle:-}" ]; then
      if [ -n "${macos_stage_bundle_identity:-}" ] && \
         macos_directory_has_identity "$macos_stage_bundle" "$macos_stage_bundle_identity"; then
        rm -rf "$macos_stage_bundle" || macos_setup_status=1
      elif [ -e "$macos_stage_bundle" ] || [ -L "$macos_stage_bundle" ]; then
        macos_setup_status=1
      fi
    fi
    if [ -n "${macos_stage_agent:-}" ]; then
      if [ -n "${macos_stage_agent_identity:-}" ] && \
         macos_file_has_identity "$macos_stage_agent" "$macos_stage_agent_identity"; then
        rm -f "$macos_stage_agent" || macos_setup_status=1
      elif [ -e "$macos_stage_agent" ] || [ -L "$macos_stage_agent" ]; then
        macos_setup_status=1
      fi
    fi
    if [ -n "${macos_backup_bundle:-}" ] && \
       { [ -e "$macos_backup_bundle" ] || [ -L "$macos_backup_bundle" ]; }; then
      if [ -n "${macos_backup_placeholder_identity:-}" ] && \
         macos_directory_has_identity \
           "$macos_backup_bundle" "$macos_backup_placeholder_identity"; then
        rm -rf "$macos_backup_bundle" || macos_setup_status=1
      else
        macos_setup_status=1
      fi
    fi
    if [ -n "${macos_state_dir:-}" ] && \
       { [ -e "$macos_state_dir" ] || [ -L "$macos_state_dir" ]; }; then
      if [ -n "${macos_state_dir_identity:-}" ] && \
         macos_directory_has_identity "$macos_state_dir" "$macos_state_dir_identity"; then
        rm -rf "$macos_state_dir" || macos_setup_status=1
      else
        macos_setup_status=1
      fi
    fi
    if ! macos_release_lock; then
      warn "macOS installer lock changed during setup; preserving the foreign lock"
      macos_setup_status=1
    fi
    exit "$macos_setup_status"
  }
  trap macos_setup_cleanup 0
  trap 'exit 1' 1 2 15
  mkdir -p "$macos_applications" || exit 1
  macos_validate_profile_directory "$macos_applications" || exit 1
  mkdir -p "$macos_home_physical/Library" || exit 1
  macos_validate_profile_directory "$macos_home_physical/Library" || exit 1
  mkdir -p "$macos_launch_agents" || exit 1
  macos_validate_profile_directory "$macos_launch_agents" || exit 1
  macos_state_dir=$(mktemp -d "$macos_home_physical/.wakezilla-macos-install.XXXXXX") || exit 1
  macos_state_dir_identity=$(macos_path_identity "$macos_state_dir") || exit 1
  macos_stage_bundle=$(mktemp -d "$macos_applications/.Wakezilla.app.stage.XXXXXX") || exit 1
  macos_stage_bundle_identity=$(macos_path_identity "$macos_stage_bundle") || exit 1
  macos_bundle_nested_stage="$macos_app/${macos_stage_bundle##*/}"
  macos_stage_agent=$(mktemp "$macos_launch_agents/.dev.wakezilla.tray.plist.stage.XXXXXX") || exit 1
  macos_stage_agent_identity=$(macos_path_identity "$macos_stage_agent") || exit 1
  macos_agent_nested_stage="$macos_agent/${macos_stage_agent##*/}"
  macos_backup_bundle=$(mktemp -d "$macos_applications/.Wakezilla.app.backup.XXXXXX") || exit 1
  macos_backup_placeholder_identity=$(macos_path_identity "$macos_backup_bundle") || exit 1
  rmdir "$macos_backup_bundle" || exit 1

  macos_transaction_active=no
  macos_transaction_committed=no
  macos_cli_touched=no
  macos_helper_touched=no
  macos_agent_initial_identity=
  macos_agent_displaced="$macos_state_dir/agent.displaced"
  macos_agent_prior_restored=no
  macos_new_agent_bootstrap_attempted=no
  macos_old_agent_loaded=no
  macos_old_agent_unloaded=no
  macos_gui_domain=no
  macos_domain="gui/$macos_uid"

  macos_snapshot_leaf() {
    macos_snapshot_key="$1"
    macos_snapshot_path="$2"
    if [ -L "$macos_snapshot_path" ]; then
      printf '%s' "$(readlink "$macos_snapshot_path")" > \
        "$macos_state_dir/$macos_snapshot_key.link" || return 1
    elif [ -f "$macos_snapshot_path" ]; then
      cp -p "$macos_snapshot_path" "$macos_state_dir/$macos_snapshot_key.file" || return 1
    elif [ -e "$macos_snapshot_path" ]; then
      return 1
    else
      : > "$macos_state_dir/$macos_snapshot_key.missing" || return 1
    fi
  }

  macos_atomic_symlink() {
    macos_link_target="$1"
    macos_link_path="$2"
    macos_link_dir=${macos_link_path%/*}
    macos_link_name=${macos_link_path##*/}
    macos_link_temp=$(mktemp "$macos_link_dir/.${macos_link_name}.link.XXXXXX") || return 1
    rm -f "$macos_link_temp" || return 1
    if ! ln -s "$macos_link_target" "$macos_link_temp"; then
      rm -f "$macos_link_temp"
      return 1
    fi
    if ! mv -f "$macos_link_temp" "$macos_link_path"; then
      rm -f "$macos_link_temp"
      return 1
    fi
  }

  macos_remove_owned_directory() {
    macos_owned_directory="$1"
    macos_owned_identity="$2"
    if macos_directory_has_identity "$macos_owned_directory" "$macos_owned_identity"; then
      rm -rf "$macos_owned_directory" || return 1
      [ ! -e "$macos_owned_directory" ] && [ ! -L "$macos_owned_directory" ]
    elif [ -e "$macos_owned_directory" ] || [ -L "$macos_owned_directory" ]; then
      return 1
    fi
  }

  macos_remove_owned_file() {
    macos_owned_file="$1"
    macos_owned_identity="$2"
    if macos_file_has_identity "$macos_owned_file" "$macos_owned_identity"; then
      rm -f "$macos_owned_file" || return 1
      [ ! -e "$macos_owned_file" ] && [ ! -L "$macos_owned_file" ]
    elif [ -e "$macos_owned_file" ] || [ -L "$macos_owned_file" ]; then
      return 1
    fi
  }

  macos_restore_leaf() {
    macos_restore_key="$1"
    macos_restore_path="$2"
    if [ -f "$macos_state_dir/$macos_restore_key.file" ]; then
      atomic_restore_file "$macos_state_dir/$macos_restore_key.file" "$macos_restore_path"
    elif [ -f "$macos_state_dir/$macos_restore_key.link" ]; then
      macos_atomic_symlink "$(cat "$macos_state_dir/$macos_restore_key.link")" \
        "$macos_restore_path"
    else
      rm -f "$macos_restore_path"
    fi
  }

  macos_rollback() {
    macos_rollback_failed=no
    set +e
    if [ "$macos_new_agent_bootstrap_attempted" = yes ]; then
      if macos_rollback_bootout_output=$("$macos_launchctl" bootout \
        "$macos_domain" "$macos_agent" 2>&1); then
        :
      else
        macos_rollback_bootout_status=$?
        macos_launchctl_failure_is_absent \
          "$macos_rollback_bootout_status" "$macos_rollback_bootout_output" || \
          macos_rollback_failed=yes
      fi
    fi
    if macos_file_has_identity "$macos_agent" "$macos_stage_agent_identity"; then
      macos_remove_owned_file "$macos_agent" "$macos_stage_agent_identity" || \
        macos_rollback_failed=yes
    fi
    if macos_file_has_identity "$macos_agent_nested_stage" "$macos_stage_agent_identity"; then
      macos_remove_owned_file "$macos_agent_nested_stage" "$macos_stage_agent_identity" || \
        macos_rollback_failed=yes
    fi
    if [ -n "$macos_agent_initial_identity" ]; then
      if macos_file_has_identity "$macos_agent" "$macos_agent_initial_identity"; then
        macos_agent_prior_restored=yes
      elif macos_file_has_identity "$macos_agent_displaced" "$macos_agent_initial_identity" && \
           [ ! -e "$macos_agent" ] && [ ! -L "$macos_agent" ]; then
        mv "$macos_agent_displaced" "$macos_agent" || macos_rollback_failed=yes
        if macos_file_has_identity "$macos_agent" "$macos_agent_initial_identity"; then
          macos_agent_prior_restored=yes
        else
          macos_rollback_failed=yes
        fi
      else
        macos_rollback_failed=yes
      fi
    fi
    [ "$macos_helper_touched" = no ] || \
      macos_restore_leaf helper "$macos_loose_helper" || macos_rollback_failed=yes
    [ "$macos_cli_touched" = no ] || \
      macos_restore_leaf cli "$macos_cli_link" || macos_rollback_failed=yes
    if macos_directory_has_identity "$macos_app" "$macos_stage_bundle_identity"; then
      macos_remove_owned_directory "$macos_app" "$macos_stage_bundle_identity" || \
        macos_rollback_failed=yes
    fi
    if macos_directory_has_identity \
      "$macos_bundle_nested_stage" "$macos_stage_bundle_identity"; then
      macos_remove_owned_directory \
        "$macos_bundle_nested_stage" "$macos_stage_bundle_identity" || \
        macos_rollback_failed=yes
    fi
    if [ -n "${macos_backup_bundle:-}" ] && [ -d "$macos_backup_bundle" ]; then
      if ! macos_directory_has_identity \
        "$macos_backup_bundle" "$macos_existing_bundle_identity"; then
        macos_rollback_failed=yes
      elif [ -e "$macos_app" ] || [ -L "$macos_app" ]; then
        macos_rollback_failed=yes
      else
        mv "$macos_backup_bundle" "$macos_app" || macos_rollback_failed=yes
        macos_directory_has_identity "$macos_app" "$macos_existing_bundle_identity" || \
          macos_rollback_failed=yes
      fi
    fi
    if [ "$macos_gui_domain" = yes ] && [ "$macos_old_agent_unloaded" = yes ] && \
       [ "$macos_agent_prior_restored" = yes ]; then
      "$macos_launchctl" bootstrap "$macos_domain" "$macos_agent" >/dev/null 2>&1 || \
        macos_rollback_failed=yes
    fi
    set -e
    [ "$macos_rollback_failed" = no ]
  }

  macos_cleanup() {
    macos_cleanup_status=$?
    macos_preserve_recovery=no
    trap - 0 1 2 15
    if [ "${macos_transaction_active:-no}" = yes ]; then
      warn "macOS desktop installation failed; rolling back bundle and startup files"
      if ! macos_rollback; then
        warn "macOS rollback incomplete; manual recovery may be required"
        macos_cleanup_status=1
        macos_preserve_recovery=yes
      fi
    fi
    if [ "${macos_transaction_committed:-no}" = yes ] && \
       { ! macos_directory_has_identity "$macos_app" "$macos_stage_bundle_identity" || \
         ! macos_file_has_identity "$macos_agent" "$macos_stage_agent_identity"; }; then
      warn "published macOS artifacts changed before cleanup; preserving recovery state"
      macos_cleanup_status=1
      macos_preserve_recovery=yes
    fi
    [ -z "${macos_stage_bundle:-}" ] || \
      macos_remove_owned_directory "$macos_stage_bundle" "$macos_stage_bundle_identity" || \
      macos_cleanup_status=1
    [ -z "${macos_bundle_nested_stage:-}" ] || \
      macos_remove_owned_directory \
        "$macos_bundle_nested_stage" "$macos_stage_bundle_identity" || \
      macos_cleanup_status=1
    [ -z "${macos_stage_agent:-}" ] || \
      macos_remove_owned_file "$macos_stage_agent" "$macos_stage_agent_identity" || \
      macos_cleanup_status=1
    [ -z "${macos_agent_nested_stage:-}" ] || \
      macos_remove_owned_file "$macos_agent_nested_stage" "$macos_stage_agent_identity" || \
      macos_cleanup_status=1
    if [ "$macos_preserve_recovery" = yes ]; then
      if [ -n "${macos_backup_bundle:-}" ] && [ -d "$macos_backup_bundle" ]; then
        warn "previous Wakezilla.app preserved for recovery at $macos_backup_bundle"
      fi
      if [ -n "${macos_state_dir:-}" ] && [ -d "$macos_state_dir" ]; then
        warn "macOS integration snapshots preserved for recovery at $macos_state_dir"
      fi
    else
      if [ -n "${macos_state_dir:-}" ] && \
         { [ -e "$macos_state_dir" ] || [ -L "$macos_state_dir" ]; }; then
        if macos_directory_has_identity "$macos_state_dir" "$macos_state_dir_identity"; then
          rm -rf "$macos_state_dir" || macos_cleanup_status=1
        else
          warn "unexpected recovery state retained at $macos_state_dir"
          macos_cleanup_status=1
        fi
      fi
    fi
    if [ "$macos_preserve_recovery" = no ] && \
       [ -n "${macos_backup_bundle:-}" ] && [ -d "$macos_backup_bundle" ]; then
      if macos_directory_has_identity \
        "$macos_backup_bundle" "$macos_existing_bundle_identity"; then
        rm -rf "$macos_backup_bundle" || macos_cleanup_status=1
      else
        warn "unexpected bundle retained at $macos_backup_bundle"
        macos_cleanup_status=1
      fi
    fi
    if ! macos_release_lock; then
      warn "macOS installer lock changed during cleanup; preserving the foreign lock"
      macos_cleanup_status=1
    fi
    exit "$macos_cleanup_status"
  }
  trap macos_cleanup 0
  trap 'exit 1' 1 2 15

  mkdir -p "$macos_stage_bundle/Contents/MacOS" \
    "$macos_stage_bundle/Contents/Resources" || exit 1
  cp "$macos_cli_source" "$macos_stage_bundle/Contents/MacOS/wakezilla" || exit 1
  cp "$macos_tray_source" "$macos_stage_bundle/Contents/MacOS/wakezilla-tray" || exit 1
  cp "$macos_icon_source" "$macos_stage_bundle/Contents/Resources/Wakezilla.icns" || exit 1
  chmod 0755 "$macos_stage_bundle/Contents/MacOS/wakezilla" \
    "$macos_stage_bundle/Contents/MacOS/wakezilla-tray" || exit 1
  chmod 0644 "$macos_stage_bundle/Contents/Resources/Wakezilla.icns" || exit 1
  write_macos_info_plist "$macos_stage_bundle/Contents/Info.plist" \
    "$macos_short_version" "$macos_bundle_version" || exit 1
  write_macos_launch_agent_plist "$macos_stage_agent" "$macos_app" || exit 1
  "$macos_plutil" -lint "$macos_stage_bundle/Contents/Info.plist" >/dev/null || exit 1
  "$macos_plutil" -lint "$macos_stage_agent" >/dev/null || exit 1

  macos_snapshot_leaf cli "$macos_cli_link" || exit 1
  macos_snapshot_leaf helper "$macos_loose_helper" || exit 1
  if [ -f "$macos_agent" ] && [ ! -L "$macos_agent" ]; then
    macos_agent_initial_identity=$(macos_path_identity "$macos_agent") || exit 1
  fi
  macos_snapshot_leaf agent "$macos_agent" || exit 1
  if [ -n "$macos_agent_initial_identity" ]; then
    macos_file_has_identity "$macos_agent" "$macos_agent_initial_identity" || exit 1
  elif [ -e "$macos_agent" ] || [ -L "$macos_agent" ]; then
    exit 1
  fi

  if macos_domain_output=$("$macos_launchctl" print "$macos_domain" 2>&1); then
    macos_gui_domain=yes
  else
    macos_domain_status=$?
    if macos_launchctl_failure_is_absent "$macos_domain_status" "$macos_domain_output"; then
      macos_gui_domain=no
    else
      warn "failed to inspect the macOS GUI launchd domain"
      exit 1
    fi
  fi
  if [ "$macos_gui_domain" = yes ]; then
    if macos_service_output=$("$macos_launchctl" print \
      "$macos_domain/dev.wakezilla.tray" 2>&1); then
      macos_old_agent_loaded=yes
    else
      macos_service_status=$?
      if ! macos_launchctl_failure_is_absent "$macos_service_status" "$macos_service_output"; then
        warn "failed to inspect the existing Wakezilla LaunchAgent"
        exit 1
      fi
    fi
  fi

  macos_transaction_active=yes
  if [ "$macos_existing_bundle" = yes ]; then
    macos_lock_is_owned || exit 1
    macos_directory_has_identity "$macos_app" "$macos_existing_bundle_identity" || exit 1
    mv "$macos_app" "$macos_backup_bundle" || exit 1
    macos_directory_has_identity \
      "$macos_backup_bundle" "$macos_existing_bundle_identity" || exit 1
  fi
  macos_lock_is_owned || exit 1
  mv -n "$macos_stage_bundle" "$macos_app" || exit 1
  if macos_directory_has_identity "$macos_app" "$macos_stage_bundle_identity"; then
    :
  elif macos_directory_has_identity \
    "$macos_bundle_nested_stage" "$macos_stage_bundle_identity"; then
    warn "Wakezilla.app appeared concurrently during bundle publication"
    exit 1
  else
    warn "failed to publish the staged Wakezilla.app at its exact destination"
    exit 1
  fi

  macos_cli_touched=yes
  macos_atomic_symlink "$macos_app/Contents/MacOS/wakezilla" "$macos_cli_link" || exit 1
  macos_helper_touched=yes
  rm -f "$macos_loose_helper" || exit 1
  macos_lock_is_owned || exit 1
  if [ -n "$macos_agent_initial_identity" ]; then
    macos_file_has_identity "$macos_agent" "$macos_agent_initial_identity" || exit 1
    mv "$macos_agent" "$macos_agent_displaced" || exit 1
    macos_file_has_identity "$macos_agent_displaced" "$macos_agent_initial_identity" || exit 1
  elif [ -e "$macos_agent" ] || [ -L "$macos_agent" ]; then
    warn "LaunchAgent appeared concurrently before publication"
    exit 1
  fi
  macos_lock_is_owned || exit 1
  ln "$macos_stage_agent" "$macos_agent" || exit 1
  if ! macos_file_has_identity "$macos_agent" "$macos_stage_agent_identity"; then
    warn "LaunchAgent appeared concurrently during publication"
    exit 1
  fi
  macos_remove_owned_file "$macos_stage_agent" "$macos_stage_agent_identity" || exit 1

  if [ "$macos_gui_domain" = yes ]; then
    macos_lock_is_owned || exit 1
    if [ "$macos_old_agent_loaded" = yes ]; then
      macos_old_agent_unloaded=yes
      if ! "$macos_launchctl" bootout "$macos_domain" "$macos_agent" >/dev/null; then
        macos_old_agent_unloaded=no
        exit 1
      fi
    fi
    macos_new_agent_bootstrap_attempted=yes
    "$macos_launchctl" bootstrap "$macos_domain" "$macos_agent" >/dev/null || exit 1
    "$macos_launchctl" kickstart -k "$macos_domain/dev.wakezilla.tray" >/dev/null || exit 1
  fi
  macos_lock_is_owned || exit 1
  macos_directory_has_identity "$macos_app" "$macos_stage_bundle_identity" || exit 1
  macos_file_has_identity "$macos_agent" "$macos_stage_agent_identity" || exit 1
  if [ "$macos_gui_domain" = yes ]; then
    info "macOS desktop integration installed; Wakezilla tray launch requested"
  else
    warn "macOS desktop integration installed; the Wakezilla tray will start at the next graphical login"
  fi
  macos_transaction_committed=yes
  macos_transaction_active=no
)

install_macos_desktop_integration() {
  macos_install_extract="$1"
  macos_install_bin="$2"
  macos_install_version="$3"
  macos_install_uid=$(id -u 2>/dev/null || printf '')
  install_macos_desktop_integration_at \
    "$macos_install_extract" "$macos_install_bin" "$macos_install_version" \
    "${HOME:-}" "$macos_install_uid" /usr/bin/plutil /bin/launchctl
}

run_installer_main_at() (
installer_main_uid="$1"
installer_macos_plutil="$2"
installer_macos_launchctl="$3"
shift 3

parse_args "$@"
check_dependencies
target=$(detect_target)
case "$target" in
  *-apple-darwin)
    case "$installer_main_uid" in
      0) err "install" "macOS installation must run without sudo; rerun as your user" ;;
      ''|*[!0-9]*) err "install" "cannot resolve the current macOS user ID" ;;
    esac
    validate_macos_home_for_uid "${HOME:-}" "$installer_main_uid" >/dev/null || \
      err "install" "HOME must be absolute, physical, non-symlink, and owned by the current macOS user"
    ;;
esac
requested_bin_dir=$(resolve_bin_dir)
bin_dir=$(canonicalize_bin_dir "$requested_bin_dir") || \
  err "install" "failed to create or canonicalize install directory: $requested_bin_dir"

info "installing $BIN_NAME for $target"
json=$(fetch_release_json "${VERSION:-}") || err "download" "failed to fetch release metadata from GitHub"

release_version=$(printf '%s' "$json" | release_version_from_json)

tmpdir=$(mktemp -d 2>/dev/null || mktemp -d -t wakezilla-install)
binary_transaction_active=no
binary_touched_targets=
cleanup() {
  cleanup_status=$?
  trap - 0 1 2 15
  if [ "${binary_transaction_active:-no}" = yes ]; then
    if ! binary_rollback; then
      warn "binary rollback incomplete; manual recovery may be required"
      cleanup_status=1
    fi
  fi
  rm -rf "$tmpdir" || cleanup_status=1
  exit "$cleanup_status"
}
cleanup_and_exit() {
  exit 1
}
trap cleanup 0
trap cleanup_and_exit 1 2 15

checksums="$tmpdir/SHA256SUMS"

mkdir -p "$bin_dir" || err "install" "failed to create install directory: $bin_dir"

binary_snapshot_dir="$tmpdir/binary-rollback"
mkdir -p "$binary_snapshot_dir" || err "install" "failed to create binary rollback state"
binary_cli_target="$bin_dir/$BIN_NAME"
binary_helper_target="$bin_dir/$TRAY_HELPER_NAME"

binary_snapshot_target() {
  binary_snapshot_key="$1"
  binary_snapshot_path="$2"
  if [ -e "$binary_snapshot_path" ] || [ -L "$binary_snapshot_path" ]; then
    [ -f "$binary_snapshot_path" ] || return 1
    [ ! -L "$binary_snapshot_path" ] || return 1
    cp -p "$binary_snapshot_path" \
      "$binary_snapshot_dir/$binary_snapshot_key.file" || return 1
    : > "$binary_snapshot_dir/$binary_snapshot_key.present" || return 1
  else
    : > "$binary_snapshot_dir/$binary_snapshot_key.missing" || return 1
  fi
}

binary_restore_target() {
  binary_restore_key="$1"
  binary_restore_path="$2"
  if [ -f "$binary_snapshot_dir/$binary_restore_key.present" ]; then
    atomic_restore_file "$binary_snapshot_dir/$binary_restore_key.file" \
      "$binary_restore_path"
  else
    rm -f "$binary_restore_path"
  fi
}

binary_rollback() {
  binary_rollback_failed=no
  set +e
  for binary_rollback_key in $binary_touched_targets; do
    case "$binary_rollback_key" in
      cli)
        binary_restore_target cli "$binary_cli_target" || \
          binary_rollback_failed=yes
        ;;
      helper)
        binary_restore_target helper "$binary_helper_target" || \
          binary_rollback_failed=yes
        ;;
    esac
  done
  set -e
  [ "$binary_rollback_failed" = no ]
}

case "$target" in
  *-apple-darwin)
    # The bundle transaction snapshots and publishes the CLI symlink and stale
    # helper. Never expose the archive binaries as loose macOS executables.
    ;;
  *)
    binary_snapshot_target cli "$binary_cli_target" || \
      err "install" "cannot snapshot existing CLI destination"
    binary_snapshot_target helper "$binary_helper_target" || \
      err "install" "cannot snapshot existing tray helper destination"
    ;;
esac

if download_and_stage_target "$target"; then
  stage_status=0
else
  stage_status=$?
fi
case "$stage_status" in
  0) ;;
  2)
    {
      printf '%s %s does not include a prebuilt binary for %s.\n' \
        "$BIN_NAME" "v$release_version" "$target"
      printf '\nAvailable targets:\n'
      printf '%s' "$json" | available_targets_from_json "$BIN_NAME" | \
        while IFS= read -r available_target; do
          [ -n "$available_target" ] && printf ' - %s\n' "$available_target"
        done
    } >&2
    err "download" "no release asset found for target: $target"
    ;;
  3)
    # A glibc build can be incompatible with an older host. Validate the static
    # musl candidate before allowing either candidate to reach the install dir.
    original_target=$target
    fallback_target=$(musl_fallback_target "$target")
    if [ -z "$fallback_target" ] || [ "$fallback_target" = "$target" ]; then
      err "install" "no runnable $BIN_NAME binary is available for $target"
    fi
    warn "$BIN_NAME for $target failed validation; retrying with $fallback_target"
    if download_and_stage_target "$fallback_target"; then
      fallback_status=0
    else
      fallback_status=$?
    fi
    if [ "$fallback_status" -ne 0 ]; then
      err "install" "no runnable $BIN_NAME binary is available for $original_target or $fallback_target"
    fi
    target=$fallback_target
    ;;
  4)
    err "install" "$BIN_NAME candidate version $STAGE_REPORTED_VERSION does not match release version $release_version"
    ;;
  *) err "install" "failed to stage $BIN_NAME for $target" ;;
esac

INSTALLED_ASSET_URL=$STAGED_ASSET_URL
FINAL_EXTRACT_DIR=$STAGED_EXTRACT_DIR
installed_version=$STAGED_VERSION

case "$target" in
  *-apple-darwin)
    if ! install_macos_desktop_integration_at \
      "$FINAL_EXTRACT_DIR" "$bin_dir" "$release_version" "${HOME:-}" \
      "$installer_main_uid" "$installer_macos_plutil" "$installer_macos_launchctl"; then
      err "integration" "failed to install macOS desktop integration"
    fi
    ;;
  *-unknown-linux-gnu|*-unknown-linux-musl)
    binary_transaction_active=yes
    binary_touched_targets="helper $binary_touched_targets"
    if [ -n "$STAGED_HELPER_FILE" ]; then
      install_bin "$STAGED_HELPER_FILE" "$binary_helper_target" || \
        err "install" "failed to install binary to $binary_helper_target"
    else
      rm -f "$binary_helper_target" || \
        err "install" "failed to remove stale tray helper at $binary_helper_target"
    fi
    binary_touched_targets="cli $binary_touched_targets"
    install_bin "$STAGED_BIN_FILE" "$binary_cli_target" || \
      err "install" "failed to install binary to $binary_cli_target"
    if ! install_linux_desktop_integration "$FINAL_EXTRACT_DIR" "$bin_dir"; then
      err "integration" "failed to install Linux desktop integration"
    fi
    binary_transaction_active=no
    ;;
esac

if [ -n "$installed_version" ]; then
  info "installed $BIN_NAME v$installed_version to $bin_dir/$BIN_NAME"
else
  err "install" "validated $BIN_NAME version unexpectedly became empty"
fi

info "resolved $BIN_NAME v$release_version"
info "asset: $INSTALLED_ASSET_URL"
info "install dir: $bin_dir"
path_guidance "$bin_dir"
case "$target" in
  *-apple-darwin) ;;
  *) offer_sudo_symlink "$bin_dir" ;;
esac
)

run_installer_main() {
  installer_euid=$(id -u 2>/dev/null || printf '')
  run_installer_main_at \
    "$installer_euid" /usr/bin/plutil /bin/launchctl "$@"
}

if [ -n "${WAKEZILLA_INSTALL_SH_TEST_MODE:-}" ]; then
  return 0 2>/dev/null || exit 0
fi

run_installer_main "$@"
