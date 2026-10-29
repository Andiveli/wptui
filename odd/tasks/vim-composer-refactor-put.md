# Vim composer refactor and put
Branch: `preview/issue-251-vim-composer`; worktree: `.worktrees/issue-251-vim-compose`.
Sequence: refactor, verify, then implement put; do not run local Cargo or publish a PR/release.

## Specs
S1. "Okay, te preguntaba más bien por la reglas del tipo archivos cortos o respetando una sola responsabilidad, arquitectura hexagonal y clean, y esas cosas"
S2. "Okay, es cierto, funciona. Entonces primero vamos al refactor y luego vamos a implementar p, te parece? En la misma rama el refactor, pues le pertenece entero a este feature, solo que son correcciones"

## Tasks
T1 [S1-S2] Route: inline; isolate Vim logic and tests into cohesive modules without changing behavior. Done; commit: 01e1afa.
T2 [S1-S2] Route: inline; verify unchanged behavior and architectural boundaries via GitHub Actions; inspect resulting diff. Done; evidence: run 40817462447 at 01e1afa (27 architecture, 19 composer, 15 Vim tests passed; full build green). Commit: 01e1afa.
T3 [S2] Route: decide/inline; clarify observable put semantics if necessary, then implement and verify `p` after T1-T2. Commit: pending.

## Log
L1. Okay, es cierto, funciona. Entonces primero vamos al refactor y luego vamos a implementar p, te parece? En la misma rama el refactor, pues le pertenece entero a este feature, solo que son correcciones
L2. Okay, te preguntaba más bien por la reglas del tipo archivos cortos o respetando una sola responsabilidad, arquitectura hexagonal y clean, y esas cosas
L3. Baseline: src/app/composer.rs is 1218 lines (1018 before Vim motions); src/app/composer_integration.rs is 252; tests/composer_behavior.rs is 797. Rust checks must run in GitHub Actions, not local Cargo. Existing 27 architecture-boundary checks do not enforce cohesion/file length.
L4. Refactor extracted existing Vim methods/word helpers to src/app/composer/vim.rs, internal tests to src/app/composer/tests.rs, and public Vim navigation/word/yank tests to tests/vim_composer.rs. Direct whitespace/trailing-comma-normalized comparison against HEAD confirms the moved blocks are unchanged. File lengths after split: composer.rs 718, vim.rs 199, tests.rs 300; composer_behavior.rs 478, vim_composer.rs 328. Local rustfmt --check and git diff --check passed; no local Cargo.
L5. User selected "Sí, subir y verificar" for this refactor push only. Commit 01e1afa was pushed to the feature branch. GitHub Actions preview run 40817462447, head 01e1afaf1a76113ede5ce58ede35802ffd79b20a: Go bridge, Rust formatting, full Rust suite, Linux release build and artifact upload succeeded; architecture_boundaries 27/27, composer_behavior 19/19, vim_composer 15/15. Risk: large textual diff (1676 changed lines) reflects moved blocks; review as a standalone commit rather than the accumulated branch.
