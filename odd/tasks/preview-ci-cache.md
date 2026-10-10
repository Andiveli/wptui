# Preview CI dependency cache
Branch: `preview/issue-21-mouse-arrows` at `030542759df08ad6e1224d8f48aa305249d24525`, based directly on beta. Workflow changes and work-unit commits/push are authorized on preview only; no beta/main push, PR or release. Never run local Cargo/Go. Preserve `.pi/` and `wptui-crash.log`.

## Specs

- S1: "Okay, hablando de github actions, si tenemos activo el caché? Para que los build sean más rápido ya que cargo es lento"
- S2: "Si nos sale rentable y lo activamos para todas las preview yo lo firmo definitivamente"

## Tasks

- C1 [S1-S2] in_progress; inline work unit, Conventional Commit on preview only: establish baseline and add bounded reusable Cargo/Go caches to the workflow for every `preview/**` push without granting a beta/main delivery. Avoid uploading release binaries, workspace build products or secrets to caches; retain source job's tests, release build and artifact. Do not push while an earlier preview candidate is running because the workflow's concurrency setting would cancel its validation.
- C2 [S2] pending; independent verifier because CI/delivery is risk item 5: observe exact-SHA cold/warm preview runs, cache hit and save behavior, Go/Rust test/build/artifact results, overhead and practical value; record any limitation that tests still run serially and separate preview branches cannot restore sibling branch caches. If no real gain, report it rather than promising savings.

## Log

L1 user verbatim: "Okay, hablando de github actions, si tenemos activo el caché? Para que los build sean más rápido ya que cargo es lento"
L2 user authorization and condition verbatim: "Si nos sale rentable y lo activamos para todas las preview yo lo firmo definitivamente". Scope preview builds across `preview/**`; do not infer permission for beta/main delivery or a PR.
L3 observation: `.github/workflows/preview.yml`, `ci.yml`, and `release.yml` lack an explicit Cargo cache. Preview's `setup-go@v7` tries implicit Go caching but logs warn that root `go.mod` does not exist; nested bridge uses `whatsrust/lib/go.sum`. `gh cache list` showed only two ~49 MB Nix caches on `main`. Artifacts uploaded by CI are not build caches.
L4 baseline: successful uncached Actions run 38055639502 took ~15m44s: Go check 43s, Rust test step ~11m54s including ~2m10s compile, release build ~2m30s; the remainder is serial test execution and cannot be eliminated by build caching. Cache branch access is scoped to current/default/PR base, not sibling preview branches. Use a bounded Rust cache for dependent build artifacts and nested Go module cache; compare real timings before calling it definitively worthwhile. Run 38059167535 was still in progress at planning time; pushing this workflow change then would have canceled it due `concurrency.cancel-in-progress: true`.
L5 run 38059167535 finished GREEN for exact SHA `0305427` (Go, Rust, Linux artifact 11672808312); no concurrent candidate will be canceled by the upcoming cache-only push. Draft `.github/workflows/preview.yml` guards `setup-go` cache to `preview/**` and points it at nested `whatsrust/lib/go.sum`; a separate Rust cache step is guarded to `preview/**`, pins `Swatinem/rust-cache` v2.9.2 to commit `6323deb102c322ba6fcbdcafc7e3dddab59af2b6`, excludes Cargo binaries and retains compiled dependencies even on expected RED tests. `docs/preview-builds.md` states first-run and cross-branch limitations. YAML parsed via PyYAML and `git diff --check` passed; `actionlint` is unavailable, no cloud run/cache hit yet. Risk item 5: independent verifier after exact-SHA checks and measured cold/warm results.
