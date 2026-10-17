# Issue #251: first in-app Vim composer slice
Branch: `preview/issue-251-vim-composer` from `main` 286eed4. Delivery: ask-on-risk; forecast ~250–400 authored lines, reviewable behavior commits. Runner: preview GitHub Actions only; no local Cargo.

## Specs
S1. "Pues vamos a eso entonces" — in context, accept the proposed first slice: internal Vim editing from the conversation Space leader, with entry/exit and `h/j/k/l` navigation. Check: terminal-event/public UI tests cover entry only from conversation, draft preservation, movement, and unchanged ordinary composer.
S2. "Sí, y también habíamos dicho que empezaríamos con unos, y que dejaríamos otros para luego, por ejemplo y" — first slice defers `y`; earlier scoped recommendation also defers `/`, `s`, and `d`. Check: no new copy/search/substitution/deletion behavior is shipped.
S3. "Como Vim: Insert → Normal → salir" — selected explanation: "`Esc` desde Insert vuelve a Normal; otro `Esc` sale del modo Vim, conserva el borrador y vuelve a navegación del chat." Check: public key events assert both transitions and unchanged draft; normal composer Esc behavior remains intact.
S4. "No se puede usar cargo acá en local, se usan las github actions" — Rust validation runs in GitHub Actions after separately authorized publication, never local Cargo. Check: record exact workflow run and SHA or explicitly report proof unavailable.

## Tasks
T1 [S1,S3] DONE Scoped worker + parent: public routing RED 39002410445, behavior and guards GREEN preview 39003692737; commits: e3d5dd2 (RED), 09f9028 (behavior).
T2 [S1,S2,S3] IN_PROGRESS Inline: add public movement RED tests; route `h/j/k/l` through existing composer movement, provide visible mode cue and update relevant help/docs while preserving standard composer; commit: n/a.
T3 [S1-S4] PENDING Inline: run structural checks, obtain publication authorization for GitHub Actions, observe exact-head CI and independent verification if risk item applies; commit: n/a.

## Log
L1. Listo, ahora vamos a algo super tocho, el issue 251
L2. Creo que antes ya habíamos hablado del plan, puedes revisar en la memoria?
L3. Sí, y también habíamos dicho que empezaríamos con unos, y que dejaríamos otros para luego, por ejemplo y
L4. Pues vamos a eso entonces
L5. User selected "Como Vim: Insert → Normal → salir" and "`Esc` desde Insert vuelve a Normal; otro `Esc` sale del modo Vim, conserva el borrador y vuelve a navegación del chat."
L6. Issue #251 initially asks for `$EDITOR`; issue author accepted an optional internal Vim approach in comments. Engram 4469 records conversation-only leader activation, 4467 confirms multiline composer; 7134 records `y` deferral. `Space+v` is currently unused. Existing normal composer and status composer are outside the new mode. No push, PR or merge authorization for this feature yet.
L7. Worktree created at `.worktrees/issue-251-vim-compose` from remote main 286eed4; root worktree and other features left untouched. Forecast is provisional; revisit before committing if accumulated authored diff exceeds ~400 lines.
L8. rama preview/
L9. User authorized publishing only this feature's RED/GREEN commits on `preview/issue-251-vim-composer` to GitHub Actions via the current gh account; no PR, merge or release. Preview builds an artifact automatically.
L10. T1 public terminal-event test-only commit e3d5dd2 pushed; preview Actions run 39002410445 on exact SHA observed expected RED: Go and rustfmt passed; 15 composer_behavior tests passed, only conversation_leader_enters_vim_and_escape_returns_to_chat_without_losing_draft failed at tests/composer_behavior.rs:134 (MessageNavigation vs ComposerEditing after Space+v); Rust tests exit 101, preview artifact skipped. No local Cargo.
L11. T1 implementation 09f902884a502e07bc08b1af150458a68284f3da added a guarded Space+v entry for Chats/Communities conversation only, Vim Normal/Insert, and two-stage Escape retaining draft; blocked/status/list cases covered. Exact-head preview Actions 39003692737 completed success: Go bridge, Rust format and tests, release build, artifact upload. No local Cargo. Existing composer paths are shared; T2 must prove unchanged ordinary behavior and mode isolation.
L12. T2 test-only b1bff9a6f8024031b6ad2332b7d8aa59c1cc060b pushed with public event tests for `h/j/k/l` movement without insertion and public render for Normal/Insert title; preview Actions 39039964379 observed expected RED: Go and rustfmt passed, 17 composer_behavior tests passed, only vim_composer_title_shows_the_active_mode_without_changing_standard_input_title failed at line 194 (no Vim Normal title) and vim_navigation_moves_the_caret_without_inserting_commands_or_sending failed at line 155 (h left cursor at end, `ab\ncdX` vs `ab\ncXd`). Rust tests exit 101, artifact skipped. No local Cargo.
L13. T2 implementation routes Normal h/j/k/l to the composer's existing visual-layout arrow movement (not global pane/message actions), renders Normal/Insert in the composer border title, and documents the first-slice controls in README. Structural diff check passed; exact-head GREEN pending GitHub Actions, no local Cargo.
