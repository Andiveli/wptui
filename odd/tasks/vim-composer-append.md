# Vim composer append
Branch: `preview/issue-251-vim-composer`; worktree: `.worktrees/issue-251-vim-compose`.
Scope: distinguish Normal-mode `a` from existing `i`; Rust verified through GitHub Actions, never local Cargo.

## Specs
S1. "Funcione correcto, me gusta, ahora quiero agregar la funcionalidad y diferencia que en modo normal tienen a e i"

## Tasks
T1 [S1] Route: inline; test and implement Normal `a` versus existing `i`, document key behavior. Commit: pending.
T2 [S1] Route: inline; verify source via preview Actions and install the validated worktree-local binary for live testing. Commit: pending.

## Log
L1. Funcione correcto, me gusta, ahora quiero agregar la funcionalidad y diferencia que en modo normal tienen a e i
L2. Interpreted Spanish "a e i" as `a` and `i`, not an unrequested change to the already-supported `e` word-end motion. `i` currently enters Insert at the caret; `a` should append after the character/grapheme at that position and enter Insert. On an empty draft or end of a logical line, there is nothing to pass and it stays at the caret. Existing `l` uses visual movement and may cross lines, so `a` needs line-local movement. No local Cargo; keep changes in this feature worktree only. The branch already has two unpushed documentation-only closeout commits 2c31320 and 7abca07; pushing any new work requires separate permission.
L3. Added public terminal-event tests for `a` at the first grapheme, EOL, empty draft, line end in multiline draft, combining grapheme and emoji; existing `i` remains before the grapheme and blocked `a` stays in Normal without changes. Before implementation, confirmed no `a` route or append method existed; runnable RED unavailable under the no-local-Cargo rule. Implemented line-local grapheme advance in composer/vim.rs and a guarded Normal route in composer_integration.rs; README now distinguishes `a` from `i`. Local rustfmt --check and git diff --check passed, remote Rust tests pending.
L4. User authorized pushing 2c31320, 7abca07 and source commit 8e40860. Actions 40832140772 at 8e4086069f26868f49057f55581eba52b49bb10a: Go and rustfmt passed; 23/24 Vim tests passed, but the new blocked-a test expected `ab` after Composer::set_blocked(true) had already cleared the draft by existing design (composer.rs:335). Release build and artifact were skipped. Corrected the test to capture and compare the post-block baseline; no production behavior changed. Rust GREEN and artifact remain pending; no local Cargo.
