# Issue 471: race-safe media reads with contained symlinks
Branch: fix/issue-471-symlink-safe-media from origin/main e86b397; delivery: ask-on-risk; tests: GitHub Actions Linux/macOS/Nix (no local Cargo).
Scope: preserve contained relative and absolute symlinks, reject escapes, bind file reads to opened descriptors; no issue closure/merge.

## Specs
S1. "Vamos a revisar el 471, investiguemos primero"
S2. "No podemos mantener los symlinks?"
S3. "Claro, entonces vamos a esa solución, no la simple, pero con cuidado"
S4. Issue #471: "Expected: a preview read cannot escape the configured media root even when directory entries change concurrently. Investigate directory-descriptor-relative, constrained opening and validate the opened descriptor; preserve ordinary image/video and sticker still-fallback behavior."
S5. "Okay, continua con lo tuyo sin usar cargo en local"

## Tasks
- [ ] T1 [S2-S4] route:inline commit:pending — Implement descriptor-anchored media opening with contained relative/absolute links, wire all preview and launch consumers, and add public-interface regression/race tests.
- [ ] T2 [S3-S5] route:inline commit:pending — Run format/structural checks and GitHub Actions Linux/macOS/Nix; assess and independently verify security diff.
- [ ] T3 [S3-S5] route:inline commit:pending — Review delivery scope/CI evidence, record commits and report remaining gaps without merging or closing issue.

## Log
L1. User request verbatim: "Vamos a revisar el 471, investiguemos primero"
L2. User correction verbatim: "No podemos mantener los symlinks?"
L3. User authorization verbatim: "Claro, entonces vamos a esa solución, no la simple, pero con cuidado"
L4. Source on main e86b397: MediaRoot::media_file canonicalizes then returns a path; preview.rs and runtime_media_viewer_events.rs reopen it, and plan_launch opens after validation. Existing behavior accepts relative/absolute symlinks ending inside canonical root. Need keep Linux and macOS; libc already present; CI runs both. User previously requested no local Cargo. Initial forecast: ~350-500 authored changed lines; one high-risk cohesive work unit with its tests and possible delivery-strategy choice if >400. No source changed yet.
L5. Plan refinement: all consumers and the opener form one dependent security outcome, so T1 owns behavior and tests together; T2 runs remote checks and independent security verification; T3 closes only after proof. Source tests cannot observe local Cargo RED under user restriction; record this rather than claiming test-first runtime evidence.
L6. Implemented in branch worktree: pinned Unix root directory FD and descriptor-relative no-follow openat walk; symlinks canonicalize only to choose an in-root candidate, then the actual open restarts from the pinned root. Connected launch, animated sticker adapter, ordinary image/video and viewer to opened files; image decoding guesses format from FD rather than a pathname. Added relative/absolute contained links, escape and concurrent component-replacement tests; pre-existing tests cover still-sticker fallback, image and video. Direct rustfmt --edition 2024 --check and git diff --check passed; no Cargo run; CI and security review pending.
