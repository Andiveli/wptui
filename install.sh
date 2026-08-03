#!/usr/bin/env bash
#
# wptui installer — downloads the prebuilt binary for the current OS and
# architecture from the latest GitHub release and installs it.
#
# Usage:
#   ./install.sh                    # latest release into ~/.local/bin
#   PREFIX=/opt/wptui ./install.sh  # custom install prefix
#
# Runtime libraries are not bundled; the script warns when libchafa is
# missing. See the README, section "Runtime dependencies", for your OS.

set -euo pipefail

REPO="Andiveli/wptui"
PREFIX="${PREFIX:-$HOME/.local/bin}"
BIN_NAME="wp-tui"

die() {
    echo "error: $*" >&2
    exit 1
}

warn() {
    echo "warning: $*" >&2
}

url_for_target() {
    local os arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    arch="$(uname -m)"
    case "$os" in
        linux)
            case "$arch" in
                x86_64 | amd64) echo "wptui-x86_64-unknown-linux-gnu.tar.gz" ;;
                *) die "no prebuilt binary for linux/$arch; build from source (see README)" ;;
            esac
            ;;
        darwin)
            case "$arch" in
                arm64) echo "wptui-aarch64-apple-darwin.tar.gz" ;;
                x86_64) echo "wptui-x86_64-apple-darwin.tar.gz" ;;
                *) die "no prebuilt binary for macos/$arch; build from source (see README)" ;;
            esac
            ;;
        *) die "unsupported operating system: $os" ;;
    esac
}

main() {
    local asset url tmp
    asset="$(url_for_target)"
    url="https://github.com/${REPO}/releases/latest/download/${asset}"

    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT

    echo "Downloading ${asset} ..."
    curl -fL --proto '=https' --tlsv1.2 -o "${tmp}/${asset}" "${url}" \
        || die "download failed (is there a release with ${asset}?)"

    mkdir -p "$PREFIX"
    tar -xzf "${tmp}/${asset}" -C "$tmp"
    install -m 0755 "${tmp}/${BIN_NAME}" "${PREFIX}/${BIN_NAME}"

    echo "Installed ${BIN_NAME} to ${PREFIX}"

    if command -v pkg-config >/dev/null 2>&1 && ! pkg-config --exists chafa 2>/dev/null; then
        warn "libchafa was not detected; image previews need it (README, 'Runtime dependencies')"
    fi

    if [[ ":$PATH:" != *":${PREFIX}:"* ]]; then
        warn "${PREFIX} is not on your PATH; add it, or run ${PREFIX}/${BIN_NAME} directly"
    fi
}

main "$@"
