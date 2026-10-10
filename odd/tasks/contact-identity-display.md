# Contact identity display
Status: active. Scope: saved names, profile names, verified phone fallback, never render unresolved LID.

## Specs
S1. "Ahora mismo vinculé mi celular y mi nombre no sale, sale mi numero."
S2. "Luego, a ciertos contactos los está mostrando con el lid, y quiero que nunca muestre el lid, pues el whatsapp del celular no lo hace. Lo de los lid son a unos cuantos contactos"
S3. "En un grupo, en la pantalla de estados, y en cualquier chat"
S4. "Sí, corrije los nombres, pero no debería mostrar contacto desconocido si se supone lo tengo agregado, y en caso que no lo tenga agregado debería mostrar el nombre que el usuario se tiene (que es lo que debe pasar conmigo)"
S5. "Ahora sí mostrar el numero, no \"Nombre no disponible\""
S6. "No debería existir ese caso, el ultimo caso sería solo mostrar el numero"
S7. "Sí" (answer to temporarily hiding entries with no name and no verified number).

## Tasks
T1 [S1-S7] inline: add regression tests, implement bridge and view resolution, verify via preview/** push GitHub Actions (no local cargo, no PR), one work-unit commit pending.

## Log
L1. Quiero que revisemos varias cosas que me fallan: Ahora mismo vinculé mi celular y mi nombre no sale, sale mi numero. Luego, a ciertos contactos los está mostrando con el lid, y quiero que nunca muestre el lid, pues el whatsapp del celular no lo hace. Lo de los lid son a unos cuantos contactos
L2. En un grupo, en la pantalla de estados, y en cualquier chat
L3. Sí, corrije los nombres, pero no debería mostrar contacto desconocido si se supone lo tengo agregado, y en caso que no lo tenga agregado debería mostrar el nombre que el usuario se tiene (que es lo que debe pasar conmigo)
L4. Ahora sí mostrar el numero, no "Nombre no disponible"
L5. No debería existir ese caso, el ultimo caso sería solo mostrar el numero
L6. Sí (accepts hiding only when name and verified phone are unavailable).
L7. Evidence: src/app/chat_store.rs:85-100 falls back to raw JID; whatsrust/lib/contacts.go:70-88 exports only named store entries; participant_identity.go:120-150 self name is not used in general chat/status views. Existing unrelated modifications in src/app/input_mapping.rs, src/app/input_mapping/tests.rs, src/ui.rs; current branch fix/issue-396-reaction-emoji-rendering.
L8. Trabaja en un worktree aparte que toma de base a beta
L9. Por eso mismo no tienes que usar cargo, sino github actions
L10. Linked worktree odd/worktrees/contact-identity-display uses branch fix/contact-identity-display from beta at 4de7b65.
L11. User authorized publishing the branch and opening a PR to beta for GitHub Actions, not merging. CI is triggered only for pull_request or pushes to main. The runner is unavailable before PR publication, so record unobserved RED and verify on CI; do not invoke local cargo.
L12. Plan condensed to one coupled work unit because tests/bridge/views cannot close independently before CI.
L13. Draft bridge/view changes and synthetic tests written, but none executed; `git diff --check` passed and Go files were gofmt-formatted. No Cargo run. Origin local URL is https://github.com/Andiveli/wptui.git. No remote read, commit, push or PR performed.
L14. se está yendo por las ramas: GitHub Actions ya corre al hacer push a preview/**, sin issue ni PR. De hecho, ya lo estamos usando para #21, que además ya tiene issue.

 Un PR podría responder a una política de revisión del repositorio, pero no hace falta para ejecutar Actions y no está autorizado para esta etapa. Si el otro agente trabaja en otro repo, habría que mirar su workflow antes de juzgarlo.
L15. Correction: .github/workflows/preview.yml runs on pushes to beta and preview/**, including Go and Rust checks. Earlier PR/approved-issue prerequisite was wrong for CI. Do not open PR.
L16. Local branch renamed preview/contact-identity-display. Independent read-only verification found incomplete profile-over-number precedence, nondeterministic PN/LID aliases, missing self PN fallback, and numeric group LID leaks; code and synthetic tests were corrected. Go files gofmt-formatted; `git diff --check` passed. Neither local Cargo nor GitHub Actions has run.
L17. User authorized a commit and push of only preview/contact-identity-display to origin for GitHub Actions, with no PR or merge. Work-unit commit e947163f5c5f20e060ac14f05d8528f4acb32b37 (fix(contacts): resolve profile names and verified phone aliases).
L18. GitHub Actions preview run 38060445357 on 048608d failed `TestNormalAndOptimisticProductionRoutesHaveWireAndCallbackParity`: callback text had @111 instead of the saved self name; Rust checks did not run. Cause: self PN numeric fallback rank 4 overwrote the saved GetAllContacts name when GetContact had no name. Correction preserves ranked saved aliases and adds a regression test in commit 37efcaf5fc6daf3c6b7ad0dba202b4354fe78d81.
L19. GitHub Actions preview run 38060811256 on 37efcaf passed Go bridge, failed Rust formatting on two exact lines in src/app/chat_store/hydration.rs; Rust tests/build did not run. Applied CI-specified formatting only; rerun pending.
