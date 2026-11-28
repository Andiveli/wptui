#!/usr/bin/env bash
set -euo pipefail

# Run from the repository root inside the native Alpine build image.
if [[ $(uname -m) != aarch64 ]]; then
  echo 'The Linux ARM64 musl build requires AArch64.' >&2
  exit 1
fi

export LC_ALL=C
export CARGO_TARGET_DIR="$PWD/target"
# Link the native system libraries with the dynamic musl CRT.
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS='-C target-feature=-crt-static'

target=aarch64-unknown-linux-musl
release_dir="$CARGO_TARGET_DIR/$target/release"
archive="/dist/wptui-$target.tar.gz"

go -C whatsrust/lib vet -mod=readonly ./...
go -C whatsrust/lib test -mod=readonly ./...
cargo test --locked --target "$target" --manifest-path whatsrust/Cargo.toml -- --test-threads=1
cargo test --locked --target "$target" -- --test-threads=1
cargo build --locked --target "$target" --release

binary="$release_dir/wp-tui"
elf_header=$(readelf -hW "$binary")
elf_segments=$(readelf -lW "$binary")
if ! grep -Eq '^[[:space:]]*Machine:[[:space:]]+AArch64$' <<< "$elf_header" ||
   ! grep -Fq '[Requesting program interpreter: /lib/ld-musl-aarch64.so.1]' <<< "$elf_segments"; then
  printf 'Expected an AArch64 ELF with the musl interpreter.\n%s\n%s\n' "$elf_header" "$elf_segments" >&2
  exit 1
fi
printf '%s\n%s\n' "$elf_header" "$elf_segments"

mkdir -p /dist
tar -czf "$archive" -C "$release_dir" wp-tui -C "$PWD" THIRD_PARTY_NOTICES.md assets/sounds/LICENSE.md
members=$(tar -tzf "$archive")
printf '%s\n' "$members"
if [[ $members != $'wp-tui\nTHIRD_PARTY_NOTICES.md\nassets/sounds/LICENSE.md' ]]; then
  echo 'Unexpected ARM64 musl archive members.' >&2
  exit 1
fi
