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

Every push to `preview/**` runs `go vet`, Go tests, Rust formatting, and the serial Rust test suite before producing a release-mode Linux binary. If any check fails, there is no preview artifact. GitHub Actions sees only committed changes pushed to that branch, not local worktree edits. Pull requests and pushes to `main` run CI but do not upload preview binaries.

Actions caches the nested Go module and Cargo dependency/build artifacts for subsequent pushes to the same preview branch. The first run may be cold, and sibling preview branches do not share their branch-local cache. Caching can shorten compilation but does not skip the serial test suite; the cache is not the downloadable binary artifact.

The preview artifact expires after 14 days. A downloaded binary still needs compatible Linux system libraries (including Chafa, GLib, and Wayland) and local interactive validation. An Actions pass does not prove that live WhatsApp status publication works.

To get an integrated-`main` binary on demand, run `gh workflow run release.yml --ref main`. That workflow uploads Linux and macOS Actions artifacts without attaching them to a GitHub release when dispatched from `main`; publishing a stable release still requires a tag.
