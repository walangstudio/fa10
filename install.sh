#!/bin/sh
set -e

# fa10 installer for Linux and macOS. Downloads the prebuilt binary from GitHub
# Releases, verifies its SHA-256, and installs it. Re-running upgrades in place.
#   curl -fsSL https://raw.githubusercontent.com/walangstudio/fa10/main/install.sh | sh
#   ... | sh -s -- --version v0.1.0      install a specific version
#   ... | sh -s -- --pre-release         install the latest pre-release
#   ... | sh -s -- --uninstall           remove fa10
# On Windows use install.ps1 instead.

REPO="walangstudio/fa10"
BINARY="fa10"

if [ -t 1 ]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'
else
  RED='' GREEN='' YELLOW='' CYAN='' BOLD='' RESET=''
fi

info()    { printf "${CYAN}==>${RESET} ${BOLD}%s${RESET}\n" "$1"; }
success() { printf "${GREEN}ok${RESET} %s\n" "$1"; }
warn()    { printf "${YELLOW}warning:${RESET} %s\n" "$1"; }
fatal()   { printf "${RED}error:${RESET} %s\n" "$1" >&2; exit 1; }

TMP_DIR=""
cleanup() { [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ] && rm -rf "$TMP_DIR"; }
trap cleanup EXIT INT TERM

# Map this machine to a Rust release target triple (must match the release assets).
detect_target() {
  case "$(uname -s)" in
    Linux*)  plat="unknown-linux-gnu" ;;
    Darwin*) plat="apple-darwin" ;;
    MINGW*|MSYS*|CYGWIN*)
      fatal "On Windows, use the PowerShell installer: irm https://raw.githubusercontent.com/${REPO}/main/install.ps1 | iex" ;;
    *) fatal "Unsupported OS: $(uname -s)" ;;
  esac
  case "$(uname -m)" in
    x86_64|amd64)  cpu="x86_64" ;;
    aarch64|arm64) cpu="aarch64" ;;
    *) fatal "Unsupported architecture: $(uname -m)" ;;
  esac
  echo "${cpu}-${plat}"
}

http_get() {
  if command -v curl >/dev/null 2>&1; then curl -fsSL "$1"
  elif command -v wget >/dev/null 2>&1; then wget -qO- "$1"
  else fatal "curl or wget is required"; fi
}

download() {
  if command -v curl >/dev/null 2>&1; then
    if [ -t 1 ]; then curl -fL --progress-bar "$1" -o "$2"; else curl -fsSL "$1" -o "$2"; fi
  else wget -q "$1" -O "$2"; fi
}

fetch_target_version() {
  use_prerelease="$1"; requested="$2"
  if [ -n "$requested" ]; then
    tag="$requested"; case "$tag" in v*) ;; *) tag="v${tag}" ;; esac
    result="$(http_get "https://api.github.com/repos/${REPO}/releases/tags/${tag}" \
      | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
    [ -n "$result" ] || fatal "Version ${tag} not found"
    echo "$result"
  elif [ "$use_prerelease" = "1" ]; then
    http_get "https://api.github.com/repos/${REPO}/releases" \
      | awk '
          /"tag_name"/ { t=$0; sub(/.*"tag_name": *"/, "", t); sub(/".*/, "", t); c=t }
          /"prerelease": *true/ && c != "" { print c; exit }
          /"prerelease": *false/ { c="" }'
  else
    http_get "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/'
  fi
}

get_installed_version() {
  if command -v "$BINARY" >/dev/null 2>&1; then
    ver=$("$BINARY" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
    [ -n "$ver" ] && echo "v${ver}"
  fi
}

verify_checksum() {
  archive="$1"; sums="$2"; name="$(basename "$archive")"
  expected="$(grep " ${name}\$" "$sums" | awk '{print $1}')"
  [ -n "$expected" ] || { warn "No checksum entry for ${name}, skipping verification"; return; }
  if command -v sha256sum >/dev/null 2>&1; then actual="$(sha256sum "$archive" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then actual="$(shasum -a 256 "$archive" | awk '{print $1}')"
  else warn "sha256sum/shasum not found, skipping checksum verification"; return; fi
  [ "$actual" = "$expected" ] || fatal "Checksum mismatch (expected ${expected}, got ${actual})"
  success "Checksum verified"
}

select_install_dir() {
  if [ -w "/usr/local/bin" ]; then echo "/usr/local/bin"
  elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then echo "/usr/local/bin"
  else echo "${HOME}/.local/bin"; fi
}

install_binary() {
  src="$1"; dir="$2"; dest="${dir}/${BINARY}"; backup="${dest}.old"
  mkdir -p "$dir"
  use_sudo=""; [ -w "$dir" ] || use_sudo="sudo"
  [ -n "$use_sudo" ] && info "Requesting sudo to write to ${dir}"
  [ -f "$dest" ] && $use_sudo cp "$dest" "$backup"
  if $use_sudo cp "$src" "$dest" && $use_sudo chmod 755 "$dest"; then
    $use_sudo rm -f "$backup"
  else
    [ -f "$backup" ] && { warn "Install failed, restoring previous version..."; $use_sudo mv "$backup" "$dest"; }
    fatal "Installation failed"
  fi
}

check_path() {
  case ":${PATH}:" in
    *":$1:"*) ;;
    *) warn "$1 is not in your PATH"
       printf "  Add to your shell profile (~/.bashrc, ~/.zshrc, ...):\n"
       printf "    ${BOLD}export PATH=\"\$PATH:$1\"${RESET}\n" ;;
  esac
}

uninstall() {
  path="$(command -v "$BINARY" 2>/dev/null)"
  [ -n "$path" ] || { warn "fa10 is not installed (not found in PATH)"; exit 0; }
  info "Removing ${path}..."
  if [ -w "$(dirname "$path")" ]; then rm -f "$path"; else sudo rm -f "$path"; fi
  success "fa10 uninstalled"
}

main() {
  USE_PRERELEASE=0; REQUESTED_VERSION=""; need_version=0
  for arg in "$@"; do
    if [ "$need_version" = "1" ]; then REQUESTED_VERSION="$arg"; need_version=0; continue; fi
    case "$arg" in
      --uninstall)   uninstall; exit 0 ;;
      --pre-release) USE_PRERELEASE=1 ;;
      --version=*)   REQUESTED_VERSION="${arg#--version=}" ;;
      --version)     need_version=1 ;;
      *) fatal "Unknown option: $arg" ;;
    esac
  done
  [ "$need_version" = "1" ] && fatal "--version requires a value (e.g. --version v0.1.0)"
  [ "$USE_PRERELEASE" = "1" ] && [ -n "$REQUESTED_VERSION" ] && fatal "--pre-release and --version cannot be combined"

  TARGET="$(detect_target)"

  info "Fetching release info..."
  VERSION="$(fetch_target_version "$USE_PRERELEASE" "$REQUESTED_VERSION")"
  [ -n "$VERSION" ] || fatal "Could not determine target version"

  INSTALLED_VERSION="$(get_installed_version)"
  if [ -n "$INSTALLED_VERSION" ]; then
    INSTALLED_PATH="$(command -v "$BINARY")"
    if [ "$INSTALLED_VERSION" = "$VERSION" ]; then
      if [ "$USE_PRERELEASE" = "0" ] && [ -z "$REQUESTED_VERSION" ]; then
        success "fa10 ${VERSION} is already installed at ${INSTALLED_PATH} -- nothing to do"; exit 0
      fi
      warn "fa10 ${VERSION} is already installed at ${INSTALLED_PATH}; reinstalling."
    else
      info "Updating fa10 ${INSTALLED_VERSION} -> ${VERSION}  (at ${INSTALLED_PATH})"
    fi
  else
    info "Installing fa10 ${VERSION}"
  fi

  ASSET="fa10-${VERSION}-${TARGET}.tar.gz"
  BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
  TMP_DIR="$(mktemp -d)"
  ARCHIVE="${TMP_DIR}/${ASSET}"
  SUMS="${TMP_DIR}/SHA256SUMS"

  info "Downloading ${ASSET}..."
  download "${BASE_URL}/${ASSET}" "$ARCHIVE"
  download "${BASE_URL}/SHA256SUMS" "$SUMS"

  info "Verifying checksum..."
  verify_checksum "$ARCHIVE" "$SUMS"

  info "Extracting..."
  tar -xzf "$ARCHIVE" -C "$TMP_DIR"
  EXTRACTED="${TMP_DIR}/${BINARY}"
  [ -f "$EXTRACTED" ] || fatal "Binary '${BINARY}' not found in archive"

  INSTALL_DIR="$(select_install_dir)"
  info "Installing to ${INSTALL_DIR}..."
  install_binary "$EXTRACTED" "$INSTALL_DIR"
  check_path "$INSTALL_DIR"

  if [ -n "$INSTALLED_VERSION" ] && [ "$INSTALLED_VERSION" != "$VERSION" ]; then
    success "fa10 updated ${INSTALLED_VERSION} -> ${VERSION}"
  else
    success "fa10 ${VERSION} installed successfully"
  fi
  printf "\n"
  command -v "$BINARY" >/dev/null 2>&1 && "$BINARY" --version || warn "fa10 is not in PATH yet. Open a new shell or update your PATH."
}

main "$@"
