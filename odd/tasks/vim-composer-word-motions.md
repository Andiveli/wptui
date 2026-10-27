# Vim composer word motions
Branch: `preview/issue-251-vim-composer` in the existing feature worktree. Delivery: review as a separate slice after the completed first Vim composer slice. Runner: GitHub Actions preview only; no local Cargo. Do not replace the installed preview binary without a separate request.

## Specs
S1. "Funciona, solo falta añadir las teclas avanzadas. e, b, y, w, creo que esas son las que hacen el salto entre palabras, si me dejo alguna más pues lo puedes buscar tu e incluir, no?"
S2. "Seguimos a esas teclas de saltos, y luego vamos a y que me dices es más complejo de lo que parece, no?"

## Tasks
T1 [S1,S2] IN_PROGRESS Route: parent/worker as triggered. Add focused public terminal-event RED tests for Normal-mode word motions while retaining draft, Insert mode typing and ordinary composer behavior. Commit: pending.
T2 [S1,S2] PENDING Route: scoped writer if context backstop still applies. Implement bounded Vim word and WORD cursor movements, document supported keys and the deferred yank operator; run GREEN on preview Actions. Commit: pending.
T3 [S1,S2] PENDING Route: parent. Structural readback, assess risk and remote test/build verification; record review slice and outcome. Commit: pending.

## Log
L1. Funciona, solo falta añadir las teclas avanzadas. e, b, y, w, creo que esas son las que hacen el salto entre palabras, si me dejo alguna más pues lo puedes buscar tu e incluir, no? Seguimos a esas teclas de saltos, y luego vamos a y que me dices es más complejo de lo que parece, no?
L2. User confirms live first-slice preview works. Interpret “luego vamos a y” as deferring yank to a later separate unit. Vim 9.2 reference https://vimhelp.org/motion.txt.html#word-motions defines word starts w/b, ends e/ge, and uppercase WORD variants W/B/E/gE; word separates alphanumeric/underscore from punctuation runs, WORD separates only at whitespace. Counts, operators and yank are not part of this movement-only slice; avoid claiming full Vim compatibility. Existing composer uses ratatui_textarea cursor with Unicode char columns, logical multiline offset and visual-layout affinities. Source/test diff of first slice already has its review boundary in odd/tasks/issue-251-vim-composer.md.
L3. Scoped worker added eight public-event motion tests in tests/composer_behavior.rs for lowercase/uppercase movement, punctuation, multiline Unicode, boundaries and no draft mutation. Parent readback checked expectation indices and confirmed existing visual h can cross lines; rustfmt --check and git diff --check passed. RED is not yet observed because local Cargo is forbidden.
