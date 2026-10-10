# Chat filters
Base: beta at 4369d537b3ee2d096eb1c71896f22601203457ed; branch preview/chat-filters.
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
L8. Local full cargo test failed at lld with a bus error while linking integration tests; focused tests had passed before L7. CI/preview workflow runs on pushes to preview/**.
L9. User authorized renaming to preview/chat-filters and pushing this branch to origin for GitHub Actions, without PR or changes to beta. Work-unit commit 7b4f59413aefc30366ab3974bec9dae610eaa205 was pushed; its CI run was superseded by the RED push.
L10. Independent verifier found the restored-unread community navigation mismatch; regression test commit 639435eadbca40700e41c887313713a97e721d1e was pushed. GitHub Actions run 38014687591 observed RED: 483 tests passed, restored_unread_group_filter_opens_the_only_unread_community_member failed (open_chat None vs expected restored@g.us), format and Go bridge checks passed.
L11. Scoped correction uses durable pending_chat_activity only in the Unread filter's community-opening path; other views retain the prior transient counter. GREEN CI pending.
