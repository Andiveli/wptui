# Test a cloud-built preview locally

Push `beta` or a `preview/**` branch to run the Go and Rust checks on GitHub Actions. A successful run uploads a Linux x86-64 binary for local testing; it does **not** publish a release. Pushing a preview branch does not update `beta`.

## Quick path

1. Find the successful run for your pushed branch:

   ```sh
   gh run list --workflow preview.yml --branch beta
   # For a preview branch, replace beta with its branch name.
   ```

2. Download its artifact, using the run ID shown above:

   ```sh
   gh run download RUN_ID --dir preview-artifact
   ```

3. Extract `wptui-linux-x86_64.tar.gz` from the downloaded artifact directory into a temporary folder, then run `wp-tui` there. Check the archive path before extraction; do not overwrite your installed binary until you have tested this candidate.

## What the preview checks

Every push to `beta` or a `preview/**` branch runs `go vet`, Go tests, Rust formatting, and the serial Rust test suite before producing a release-mode Linux binary. Future feature branches can start from `beta`, but only pushes to `beta` or `preview/**` trigger this workflow. If any check fails, there is no preview artifact. GitHub Actions sees only committed changes pushed to that branch, not local worktree edits.

The artifact expires after 14 days. A downloaded binary still needs compatible Linux system libraries (including Chafa, GLib, and Wayland) and local interactive validation. An Actions pass does not prove that live WhatsApp status publication works. Releases remain a separate, manual decision.
