# Chat filters
Base: beta at 4369d537b3ee2d096eb1c71896f22601203457ed; branch feat/chat-filters.
Scope: built-in chat filters first; personal WhatsApp lists and favorites require separate sync proof.

## Specs
S1. "Ya, quiero empezar a agrupar los chats, archivados, unread, favoritos, grupos, y las categorias que el usuario puede crear y agregar chats a esas categorias"
S2. "Sincronizadas con WhatsApp"
S3. "Empezar por los filtros incorporados" — "Implementar por etapas Archivados, No leídos y Grupos; investigar Favoritos y listas sincronizadas antes de incluirlos."
S4. "Otro worktree que toma de base a beta en 4369d53"
S5. "Sí, commits locales" — "Cierro cada filtro verificado con un commit en la rama nueva."
S6. "No tienes que usar cargo acá, usa github actions para poder usar el build o los test"

## Tasks
T1 [S1,S3,S4,S5,S6] Inline: add All/Unread/Groups chat-list filters, navigation, tests and usage docs; verify via GitHub Actions; commit: pending.
T2 [S1,S3,S4,S5,S6] Inline: connect archive metadata to the filtered chat list, test and document; verify via GitHub Actions; commit: pending.
T3 [S1,S2,S3] DROPPED from this slice: investigate favorite and personal custom-list cross-device sync as a later feature; commit: n/a.

## Log
L1. Ya, quiero empezar a agrupar los chats, archivados, unread, favoritos, grupos, y las categorias que el usuario puede crear y agregar chats a esas categorias
L2. User selected "Sincronizadas con WhatsApp" rather than local-only categories.
L3. User selected "Empezar por los filtros incorporados" rather than waiting for personal-list sync proof.
L4. Otro worktree que toma de base a beta en 4369d53
L5. User explicitly authorized local work-unit commits, without push or PR.
L6. Existing fix/issue-396-reaction-emoji-rendering checkout is dirty and must stay untouched. Personal WhatsApp custom lists are not established by whatsmeow label actions.
L7. No tienes que usar cargo acá, usa github actions para poder usar el build o los test
L8. Local full cargo test failed at lld with a bus error while linking integration tests; focused tests had passed before L7. CI/preview workflow runs on pushes to preview/**, and push authorization is not yet granted.
