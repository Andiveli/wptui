# macOS PR CI and media FD portability
Branch: ci/macos-pr-suite (from origin/main c5b1e7641406592252c006159063bf4853e2e409).
Scope: PR test coverage and issue #482; issue #471 remains separate.

## Specs
S1. "Okay, primero agreguemos mac en las pr antes de seguir al fix de la carrera"
S2. "El CI no se ejecutaba ya para mac? Si le damos soporte debería hacerlo, no? Pasar la suit por ahí?"
S3. "CI macOS + corregir #482"
S4. "Sí, corregir Go en el mismo PR #483"

## Tasks
- [x] T1 [S1-S2] route:inline commit:f3e4246,9a28b82 — Configure supported build/test suite for macOS PRs; adapt installer fixture and describe coverage.
- [ ] T2 [S3] route:inline commit:3e5b496 — Make inherited media FD argument usable on macOS without weakening pinned-file behavior; test public launch plan.
- [ ] T3 [S1-S3] route:verify commit:pending — Assess security and CI diff, verify platform checks, record open proof and delivery decision.
- [x] T4 [S2,S4] route:diagnostic commit:0dc29ff — Emit failure-only temp root and ancestor types for representative Go test; use next macOS runner to identify failing component before considering a fix.
- [ ] T5 [S2,S4] route:inline commit:pending — Correct proven macOS Go temp-root portability while preserving symlink rejection; verify Linux and macOS suites.

## Log
L1. User request verbatim: "Okay, primero agreguemos mac en las pr antes de seguir al fix de la carrera"
L2. Prior user clarification verbatim: "El CI no se ejecutaba ya para mac? Si le damos soporte debería hacerlo, no? Pasar la suit por ahí?"
L3. User recall verbatim: "Además creo que sobre el directorio que mencionas abrimos también un issue, no?"
L4. User selected exactly: "CI macOS + corregir #482"; distinct from issue #471.
L5. Existing RED: macOS Actions 40831795294 failed tests/media_lifecycle.rs on missing /proc/self/fd; issue #482 records failure. Ordinary PR CI currently Ubuntu-only and release macOS job builds without tests.
L6. Do not run local Cargo, push, or create PR without separate permission; leave local sticker documentation commits unpublished.
L7. T1: Added ubuntu-latest and macos-latest PR test matrix, platform dependencies, target-aware installer archive assertions, and README coverage. Local `bash -n tests/install_archive.sh`, `bash tests/install_archive.sh` (Linux: passed), PyYAML structural CI check, and `git diff --check` passed. T1 commit f3e4246; CI platform proof pending T3.
L8. T2 implementation: Mac uses /dev/fd/N instead of Linux-only /proc/self/fd/N, retaining the already-open inherited file. Public media lifecycle tests now expect the supported OS-specific path and assert a direct child can still read the pinned file after the original path is replaced. Existing macOS Actions 40831795294 supplies RED; no local Cargo run. Local rustfmt --check, bash installer test, CI structural assertion, and git diff --check passed. Linux/macOS Rust and Go suite proof pending Actions. T2 commit 3e5b496. The installer archive fixture also passed with a fake Darwin/arm64 `uname` on Linux; this is not macOS proof.
L9. Required branch-protection context is exactly `build-and-test` (GitHub Actions app 15368). Matrix conversion would rename that check and could block all PRs. Corrected CI to keep a `build-and-test` aggregate job requiring both platform suites, without changing repository protection. PyYAML structure and git diff check pass; correction commit 9a28b82.
L10. origin/main advanced concurrently by 46 Vim-composer integration commits to c8bda1039fe739aea1cdce910f28df3d02a62ca9; non-destructive clean merge commit 4046826 into this branch; origin/main...HEAD remains six intended CI/media/docs files. No Cargo locally.
L11. Independent gentle-ai-verify mvu56ate-9-mvyr: source-level S1 satisfied, S2 and S3 source-level satisfied but macOS runner proof pending. On merged HEAD, rustfmt --check, bash -n, Linux installer fixture and git diff --check passed; no Cargo or Go executed. Native ASSESS returned high-risk independent verifier plan, but its diff scan was unavailable because it selected the session's other root with untracked files. User separately authorized publishing only ci/macos-pr-suite and opening one PR, without merge or sticker documentation publication.
L12. Published only ci/macos-pr-suite HEAD 1725b0e and opened PR #483 https://github.com/Andiveli/wptui/pull/483 against main; macOS/Linux PR Actions wait launched as background job-34. No merge, release or sticker-documentation push.
L13. macOS run 40970799440 failed four t.TempDir tests, but logs lack temp paths. No security exception yet: diagnose the exact failing ancestor first.
L14. T4 adds failure-only fixture root and symlink/non-directory ancestor logging to the first representative Go test, preserving its assertion. `gofmt -w` completed; focused Go test timed out after 120s while fetching dependencies, and offline retry timed out after 30s. Linux and macOS test proof remains pending; no production change or security exception.
L15. Renewed session reviewed and committed diagnostic as 0dc29ff. First push failed with remote `fatal error in commit_refs`; the user authorized exactly one retry, which succeeded. PR #483 now points to 0dc29ff.
L16. Actions run 41071634574 at 0dc29ff: Linux and Nix succeeded; macOS Go tests failed. Diagnostic logged root `/var/folders/n6/hn37q6pj7sj_9dmqqc2113800000gn/T/TestSafeDownloadTargetRejectsUnsafePathsAndPreservesNestedPaths1602063974/001` and ancestor `/var`: symlink. The matching test failed with `not a directory`; remaining three Go download-storage tests also failed ENOTDIR. This proves the trusted macOS system ancestor is symlinked; preserve rejection of user-controlled symlinked roots. Rust macOS tests were skipped by the Go failure, so S3 remains unproven.
L17. T5 implementation recognizes only macOS `/var` pointing exactly to `private/var` or `/private/var`, then walks `/private/var` and every remaining component using O_NOFOLLOW. A macOS regression test rejects a caller-controlled symlink below that alias and checks preexisting outside content remains unchanged; final-symlink rejection now checks preexisting outside content too. Independent security verifier mvv3kcx6-3-fnz4: source-level conditional PASS S2/S4; runtime GREEN still unverified. Native ASSESS escalation item 2 requires independent verifier and returned one, but native diff scan was unassessable due unrelated root untracked files. No local Cargo; earlier Go dependency timeout not retried. Linux/macOS Actions still required.
