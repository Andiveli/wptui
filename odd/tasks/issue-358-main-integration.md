# Issue #358: integrate conversation search into main
Branch: `integration/issue-358-main`; worktree: `.worktrees/issue-358-main-integration` at `origin/main` e86b3977ff69ca8d9806a9cb7b56401bc4883f02.
Source: `feat/issue-358-conversation-search` c9970b25346eb83a42d41d77aa85b85c1a67a410; tested code 5012951ef7afdae2d3a7eeb06205876051b69762.

## Specs
S1. "Listo, manda merge a main"
S2. "No se puede usar cargo acá en local, se usan las github actions"

## Tasks
T1 [S1] IN PROGRESS Route: inline; merge the isolated feature onto current main without touching root or other worktrees, inspect conflicts and diff; commit: pending.
T2 [S1-S2] PENDING Route: inline; run exact combined-source Actions and independent risk verification; commit: pending.
T3 [S1] PENDING Route: inline; reconcile local main and remote publication decision, publish only if explicitly authorized; commit: pending.

## Log
L1. Listo, manda merge a main
L2. Current remote main is e86b3977ff69ca8d9806a9cb7b56401bc4883f02, eight first-parent commits after feature base c8bda1039fe739aea1cdce910f28df3d02a62ca9. Feature source c9970b25346eb83a42d41d77aa85b85c1a67a410 is clean; code at 5012951ef7afdae2d3a7eeb06205876051b69762 passed Actions 41211034149 and independent verification S1-S6; user manually tested the worktree binary successfully. Main advanced with dependency PR merges and an obsolete log-panel removal; five paths overlap the feature diff (README.md, src/app.rs, src/app/bootstrap.rs, src/app/inputs.rs, src/ui.rs). Root checkout feat/animated-stickers has unrelated untracked files and must not be touched; the existing local main worktree remains at older c8bda10. Created a separate registered integration worktree on integration/issue-358-main at remote main. No local Cargo, PR or release. Prior preview branch publication for #358 was authorized for CI, not automatic permission to push origin/main; resolve publication as a separate consequential decision after combined-source verification. The feature diff is about 800 lines including extensive tests/docs, so preserve the original reviewable commits and record merge scope rather than squash into one opaque change.
