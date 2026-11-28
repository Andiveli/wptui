# Test a cloud-built preview locally

Push a `preview/<feature>` branch based on `main` when you want GitHub Actions to check the feature and build a Linux x86-64 binary for local testing. This does **not** publish a release or change `main`.

## Quick path

1. Commit the feature work on a `preview/my-feature` branch based on `main`, then push it:

   ```sh
   git push -u origin preview/my-feature
   ```

2. Find the successful run for that branch:

   ```sh
   gh run list --workflow preview.yml --branch preview/my-feature
   ```

3. Download its artifact, using the run ID shown above:

   ```sh
   gh run download RUN_ID --dir preview-artifact
   ```

4. Extract `wptui-linux-x86_64.tar.gz` from the downloaded artifact directory into a temporary folder, then run `wp-tui` there. Check the archive path before extraction; do not overwrite your installed binary until you have tested this candidate.

## What the preview checks

Every push to `preview/**` runs `go vet`, Go tests, Rust formatting, and the serial Rust test suite before producing a release-mode Linux x86-64 binary. If a check fails in that job, it does not upload the x86-64 preview artifact. GitHub Actions sees only committed changes pushed to that branch, not local worktree edits. Pull requests and pushes to `main` run CI but do not upload preview binaries.

Actions caches the nested Go module and Cargo dependency/build artifacts for subsequent pushes to the same preview branch. The first run may be cold, and sibling preview branches do not share their branch-local cache. Caching can shorten compilation but does not skip the serial test suite; the cache is not the downloadable binary artifact.

The preview artifact expires after 14 days. A downloaded binary still needs compatible Linux system libraries (including Chafa, GLib, and Wayland) and local interactive validation. An Actions pass does not prove that live WhatsApp status publication works.

To get an integrated-`main` binary on demand, run `gh workflow run release.yml --ref main`. That workflow uploads Linux x86-64 GNU, Linux ARM64 musl, and macOS ARM64 Actions artifacts without attaching them to a GitHub release when dispatched from `main`; publishing a stable release still requires a tag.

## Linux ARM64 musl preview for postmarketOS

Push `preview/linux-arm64-musl` (or another branch matching `preview/linux-arm64*`) to run the native ARM64 Alpine job alongside the regular x86-64 job. It checks the Go bridge and locked Rust suites, verifies the binary is AArch64 with the musl loader, and runs `wp-tui --version` from the packaged archive in an Alpine image without build tools. The same build job supplies `wptui-aarch64-unknown-linux-musl.tar.gz` for future tagged releases. A preview push does not publish a release.

1. Find a successful ARM64 preview run and download only its ARM64 artifact:

   ```sh
   gh run list --workflow preview.yml --branch preview/linux-arm64-musl
   gh run download RUN_ID --name wptui-aarch64-unknown-linux-musl.tar.gz --dir preview-arm64
   ```

2. On the target device (for example, OnePlus 6T / `fajita`), confirm `uname -m` returns `aarch64` and `/lib/ld-musl-aarch64.so.1` exists. This is a **dynamically linked** musl build, not a static binary, a glibc ARM64 build, or an ARMv7 build. It requires compatible native runtime libraries, including Chafa, GLib, Wayland client, and ALSA; the Alpine 3.23 CI smoke does not guarantee compatibility with older postmarketOS versions.

3. Copy the archive to the device, extract it into a fresh directory, and run the candidate without replacing an installed binary:

   ```sh
   smoke_dir=$(mktemp -d)
   tar -xzf wptui-aarch64-unknown-linux-musl.tar.gz -C "$smoke_dir"
   cd "$smoke_dir"
   ./wp-tui --version
   XDG_CONFIG_HOME="$smoke_dir/config" XDG_DATA_HOME="$smoke_dir/data" XDG_CACHE_HOME="$smoke_dir/cache" ./wp-tui
   ```

   Check that the terminal UI opens, responds to keys, and exits cleanly. Report the run ID, postmarketOS version, and any loader or startup errors. Headless CI does not replace this device check.
