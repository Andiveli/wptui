#!/usr/bin/env bash
# Exercise the public installer with local release archives; never contact GitHub.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$repo/target"
tmp="$(mktemp -d "$repo/target/install-archive-test.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/mock-bin" "$tmp/prefix" "$tmp/old" "$tmp/new/assets/sounds" "$tmp/bad/assets/sounds"
printf 'old binary' > "$tmp/old/wp-tui"
printf 'new binary' > "$tmp/new/wp-tui"
printf 'MIT notice' > "$tmp/new/THIRD_PARTY_NOTICES.md"
printf 'recipe' > "$tmp/new/assets/sounds/LICENSE.md"
printf 'bad binary' > "$tmp/bad/wp-tui"
printf 'recipe' > "$tmp/bad/assets/sounds/LICENSE.md"
printf 'extra' > "$tmp/bad/unexpected"
ln -s ../outside "$tmp/bad/THIRD_PARTY_NOTICES.md"
tar -czf "$tmp/old.tar.gz" -C "$tmp/old" wp-tui
tar -czf "$tmp/new.tar.gz" -C "$tmp/new" wp-tui THIRD_PARTY_NOTICES.md assets/sounds/LICENSE.md
tar -czf "$tmp/bad-extra.tar.gz" -C "$tmp/bad" wp-tui unexpected
tar -czf "$tmp/bad-link.tar.gz" -C "$tmp/bad" wp-tui THIRD_PARTY_NOTICES.md assets/sounds/LICENSE.md

printf '#!/usr/bin/env bash\nset -euo pipefail\nwhile (( $# > 0 )); do\n  if [[ "$1" == "-o" ]]; then cp "$MOCK_ARCHIVE" "$2"; exit 0; fi\n  shift\ndone\nexit 1\n' > "$tmp/mock-bin/curl"
printf '#!/usr/bin/env bash\nexit 0\n' > "$tmp/mock-bin/pkg-config"
chmod +x "$tmp/mock-bin/curl" "$tmp/mock-bin/pkg-config"

run_installer() {
    env PATH="$tmp/mock-bin:$tmp/prefix:$PATH" PREFIX="$tmp/prefix" TMPDIR="$tmp" \
        MOCK_ARCHIVE="$1" bash "$repo/install.sh" > "$tmp/stdout" 2> "$tmp/stderr"
}
assert_success() {
    local asset="wptui-x86_64-unknown-linux-gnu.tar.gz"
    [[ "$(< "$tmp/stdout")" == "$(printf 'Downloading %s ...\nInstalled wp-tui to %s' "$asset" "$tmp/prefix")" ]]
    [[ ! -s "$tmp/stderr" ]]
}

run_installer "$tmp/old.tar.gz" || { printf 'initial install error: %s\n' "$(< "$tmp/stderr")" >&2; exit 1; }
assert_success
[[ "$(< "$tmp/prefix/wp-tui")" == 'old binary' ]]
[[ ! -e "$tmp/prefix/wp-tui-THIRD_PARTY_NOTICES.md" ]]

run_installer "$tmp/new.tar.gz"
assert_success
[[ "$(< "$tmp/prefix/wp-tui")" == 'new binary' ]]
[[ "$(< "$tmp/prefix/wp-tui-THIRD_PARTY_NOTICES.md")" == 'MIT notice' ]]

run_installer "$tmp/old.tar.gz"
assert_success
[[ "$(< "$tmp/prefix/wp-tui")" == 'old binary' ]]
[[ "$(< "$tmp/prefix/wp-tui-THIRD_PARTY_NOTICES.md")" == 'MIT notice' ]]

for invalid in "$tmp/bad-extra.tar.gz" "$tmp/bad-link.tar.gz"; do
    if run_installer "$invalid"; then
        echo 'unsafe archive unexpectedly installed' >&2
        exit 1
    fi
    [[ "$(< "$tmp/stdout")" == 'Downloading wptui-x86_64-unknown-linux-gnu.tar.gz ...' ]]
    [[ "$(< "$tmp/stderr")" == 'error: release archive is unsafe' ]]
    [[ "$(< "$tmp/prefix/wp-tui")" == 'old binary' ]]
    [[ "$(< "$tmp/prefix/wp-tui-THIRD_PARTY_NOTICES.md")" == 'MIT notice' ]]
done
printf 'Installer archive tests passed\n'
