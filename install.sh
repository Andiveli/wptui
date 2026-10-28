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
            esac
            ;;
        *) die "unsupported operating system: $os" ;;
    esac
}

main() {
    local asset url tmp members details has_notice
    asset="$(url_for_target)"
    url="https://github.com/${REPO}/releases/latest/download/${asset}"

    tmp="$(mktemp -d)"
    trap "rm -rf -- $(printf '%q' "$tmp")" EXIT

    echo "Downloading ${asset} ..."
    curl -fL --proto '=https' --tlsv1.2 --max-filesize 67108864 \
        -o "${tmp}/${asset}" "${url}" \
        || die "download failed (is there a release with ${asset}?)"

    members="$(tar -tzf "${tmp}/${asset}")" || die "release archive is unsafe"
    case "$members" in
        "$BIN_NAME") has_notice=false ;;
        "$(printf '%s\n%s\n%s' "$BIN_NAME" THIRD_PARTY_NOTICES.md assets/sounds/LICENSE.md)")
            has_notice=true ;;
        *) die "release archive is unsafe" ;;
    esac
    details="$(tar -tvzf "${tmp}/${asset}")" || die "release archive is unsafe"
    # GNU tar puts size in field 3; BSD tar includes a link-count and uses field 5.
    printf '%s\n' "$details" | awk -v expected="$(if "$has_notice"; then echo 3; else echo 1; fi)" '
        {
            size = ($3 ~ /^[0-9]+$/) ? $3 : $5
            if (substr($1, 1, 1) != "-" || size !~ /^[0-9]+$/ ||
                size + 0 > (NR == 1 ? 268435456 : 65536)) exit 1
        }
        END { if (NR != expected) exit 1 }
    ' || die "release archive is unsafe"

    tar -xOzf "${tmp}/${asset}" "$BIN_NAME" > "${tmp}/${BIN_NAME}" \
        || die "release archive is unsafe"
    if "$has_notice"; then
        tar -xOzf "${tmp}/${asset}" THIRD_PARTY_NOTICES.md > "${tmp}/THIRD_PARTY_NOTICES.md" \
            || die "release archive is unsafe"
    fi
    mkdir -p "$PREFIX"
    if "$has_notice"; then
        install -m 0644 "${tmp}/THIRD_PARTY_NOTICES.md" "${PREFIX}/${BIN_NAME}-THIRD_PARTY_NOTICES.md"
    fi
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
