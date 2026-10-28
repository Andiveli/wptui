# Vim composer yank to system clipboard
Branch: `preview/issue-251-vim-composer` in the existing feature worktree. New review slice starts after word motions at 3edefb9; no PR/merge/release authorization. Rust checks via preview GitHub Actions, not local Cargo.

## Specs
S1. "Funciona, solo falta añadir las teclas avanzadas. e, b, y, w, creo que esas son las que hacen el salto entre palabras, si me dejo alguna más pues lo puedes buscar tu e incluir, no? Seguimos a esas teclas de saltos, y luego vamos a y que me dices es más complejo de lo que parece, no?"
S2. "Portapapeles del sistema (recomendado)" — "`y` + movimiento y `yy` copian para pegar fuera; `p` queda para después."

## Tasks
T1 [S1,S2] IN_PROGRESS Route: parent/worker as triggered. Public terminal-event RED for operator-pending y with word motions and yy, unchanged draft/cursor and no copy on cancellation or failure; preserve existing message y. Commit: pending.
T2 [S1,S2] PENDING Route: bounded worker if context backstop applies. Implement scoped yank through injected ClipboardWriter and document exact supported combinations; GREEN preview Actions. Commit: pending.
T3 [S1,S2] PENDING Route: parent and risk-item3 independent verifier. Assess shared composer/input path, exact-head CI, review slice and close or report blocker. Commit: pending.

## Log
L1. Funciona, solo falta añadir las teclas avanzadas. e, b, y, w, creo que esas son las que hacen el salto entre palabras, si me dejo alguna más pues lo puedes buscar tu e incluir, no? Seguimos a esas teclas de saltos, y luego vamos a y que me dices es más complejo de lo que parece, no?
L2. User selected "Portapapeles del sistema (recomendado)" from "`y` + movimiento y `yy` copian para pegar fuera; `p` queda para después." Existing chat-navigation y copies selected message using ClipboardWriter; Vim y is normally an operator with motions and an unnamed internal register (https://vimhelp.org/change.txt.html). First composer slice deliberately sends copied text to OS clipboard via injected writer; no internal register, p, or counts. Preserve draft and cursor, and copy nothing until a complete supported command. Clipboard failure must not suppress ongoing draft/editing. The existing first Vim and word-motion review boundaries remain recorded in their separate ODD documents.
L3. Scoped worker added test-only public terminal-event assertions with injected ClipboardWriter for yw/ye/yb/yge/yW, yy on the middle line including newline, pending y/Esc no write, and writer failure without draft/caret mutation. Parent reviewed exact text ranges and current-line offset; rustfmt --check and diff --check passed. Existing message y coverage remains in tests/message_actions.rs. RED pending Actions; no local Cargo.
