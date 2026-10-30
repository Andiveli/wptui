# macOS PR CI main integration
Branch: ci/macos-pr-suite; PR #483 into main.
Scope: update to current main, verify exact candidate, use authorized admin bypass only for review gate; do not touch issue #471.

## Specs
S1. "Vamos al merge ahora"
S2. "Tenemos que usar bypass porque somos los unicos mantainers"

## Tasks
- [ ] T1 [S1] route:inline commit:9a447ce — Synchronize PR #483 with latest main without discarding feature work; publish and verify exact-head CI.
- [-] T2 [S1] route:external commit:none dropped — Obtain repository-required approving review without bypassing protection (superseded by S2).
- [ ] T3 [S1-S2] route:inline commit:pending — Merge PR #483 with admin bypass only after exact-head checks pass; verify remote main and post-merge checks.
- [ ] T4 [S2] route:inline commit:pending — Confirm admin bypass eligibility and preserve all CI checks without bypassing a failure.

## Log
L1. User request verbatim: "Vamos al merge ahora"
L2. PR #483 head 3ad9ced had exact-head green Actions 41085318489 (Linux/macOS suites, Nix and build-and-test). main advanced to e2ca888 through PR #484; PR #483 mergeStateStatus BEHIND, reviewDecision REVIEW_REQUIRED. Main protection requires strict current build-and-test and one approving review; no reviews yet. Dry-run merge-tree into origin/main reports a clean auto-merge of README.md. No merge/release authorized outside PR #483.
L3. Merged origin/main e2ca888 into ci/macos-pr-suite with clean merge commit 9a447ce (parents 3ad9ced and e2ca888); only README.md overlapped, auto-merged. Feature diff against updated origin/main remains eight intended files; no #471 or sticker changes. Await exact-head CI before PR merge.
L4. User correction verbatim: "Tenemos que usar bypass porque somos los unicos mantainers". User authorized admin bypass of the review gate; prior T2 approval task dropped, replacement T4 records bypass eligibility. GitHub permissions.admin=true, main protection enforce_admins=false and one approval required. Actions 41089618177 at 239fea4 remains in progress; do not bypass pending or failed CI.
