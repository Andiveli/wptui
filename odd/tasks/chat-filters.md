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
S7. "Sí, separados como en WhatsApp" — "«Todos» excluye archivados; «Archivados» los muestra."

## Tasks
T1 [S1,S3,S4,S5,S6] DONE Inline: All/Unread/Groups chat-list filters, navigation, tests and usage docs; commits: 7b4f59413aefc30366ab3974bec9dae610eaa205, 639435eadbca40700e41c887313713a97e721d1e, aa8d92ab43b2cbe4e6db389b7cde9a7d6549d6de, 1869db09e0c7a140f4746bee9054bbe59ea912cf.
T2 [S1,S3,S4,S5,S6,S7] DONE Inline: project synced archive settings into All/Archived views (including mixed community rows), test and document; verified in Actions 38024106477; commits: b84968676ee763eb2e40d1488ae16ce117da3869, 381be64893a3139ee4065bdc2bfc6832e3626462, f60f74a9c71fed4df6e34b4fb7795f153ffe4c30, d9e98e87779c172489f2a00b9b9c7390aa2f3369, f20611a309d1cef885f1c79bd1c43cad5272341a, 8c7292daaf3c17afc09b34bd8e0567c537483af5, 6591d508dc80f78cef898e874ceefc677fe65b47.
T3 [S1,S3,S4,S5,S6,S7] IN_PROGRESS Inline: bridge live WhatsApp archive changes to refresh the view without switching filters, test and document; verify via GitHub Actions; commit: pending.
T4 [S1,S2,S3] DROPPED from this slice: investigate favorite and personal custom-list cross-device sync as a later feature; commit: n/a.

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
L11. Scoped correction uses durable pending_chat_activity only in the Unread filter's community-opening path; other views retain the prior transient counter. Commit aa8d92ab43b2cbe4e6db389b7cde9a7d6549d6de pushed; GREEN CI pending.
L12. User selected "Sí, separados como en WhatsApp" for archived behavior: "«Todos» excluye archivados; «Archivados» los muestra."
L13. GitHub Actions run 38016732426 for aa8d92a passed Go bridge checks but stopped at Rust formatting before tests: rustfmt requires the navigation module's two imports in the reverse order. Corrected by commit 1869db09e0c7a140f4746bee9054bbe59ea912cf.
L14. GitHub Actions run 38017549726 on 1869db0 passed Go vet/tests, Rust format, 484 Rust unit tests plus integration tests, release build and artifact upload (artifact 11656699208). Independent verifier rechecked and cleared the restored-unread finding. T1 complete; no PR or changes to beta.
L15. Split archive views (T2) and live archive-change bridge (T3) into separate reviewable work units; both belong to the user's built-ins-first stage. Former deferred custom-list task is now T4.
L16. GitHub Actions run 38018926726 observed T2 RED on test-only commit b84968676ee763eb2e40d1488ae16ce117da3869: 484 Rust unit tests passed, all_view_excludes_archived_chat_but_keeps_active_chat failed (2 rows vs 1); Go checks and rustfmt passed.
L17. Archive projection now partitions rows and community detail by synchronized LocalChatSettings.Archived; All/Unread/Groups exclude archived, Archived includes them. Filter switching rebuilds the semantic view only when crossing the archive partition. GREEN CI pending; live updates are T3.
L18. GitHub Actions run 38019915814 on 381be64893a3139ee4065bdc2bfc6832e3626462 passed Go checks and rustfmt but Rust tests did not compile: E0382, the RED test moved its archived JID into a HashSet before later borrowing it. Clone added in f60f74a9c71fed4df6e34b4fb7795f153ffe4c30.
L19. GitHub Actions run 38021006439 on f60f74a passed Go and rustfmt but had 484 Rust tests pass and three existing message_ingestion tests fail: an extra ChatSettings port call during first sort changed their previous notification-query contract. Commit d9e98e87779c172489f2a00b9b9c7390aa2f3369 defers semantic projection until the first visible render and covers first-sort visibility.
L20. GitHub Actions run 38021666380 on d9e98e8 passed Go and rustfmt with 485 Rust tests passed and the same three message_ingestion tests failing: process_message_with_lookup itself redundantly called get_selected_chat and select_chat around sort_chats, forcing projection even without a view. Commit f20611a309d1cef885f1c79bd1c43cad5272341a removes the duplicate selection and adds a process-message selection regression test.
L21. GitHub Actions run 38022289520 on f20611a passed Go checks, rustfmt, 489 Rust unit tests and two initial integration tests, but architecture_boundaries had 26 pass/1 fail: its test-only ChatSettingsQueryPort injection allowlist lacked the newly legitimate chat_projection/tests.rs consumer. Commit 8c7292daaf3c17afc09b34bd8e0567c537483af5 adds only that test path to the allowlist.
L22. Independent read-only verifier reviewed 1869db0..8c7292d against S1/S3/S4/S5/S6/S7 and found no reproducible code issue in All/Archived partition, mixed communities/navigation, semantic caching, first-sort/message-ingestion selection, UI/docs, or the narrow architecture test update.
L23. GitHub Actions run 38023313019 on 8c7292d passed Go, rustfmt, 489 Rust unit tests and 27 architecture tests, then tests/communities aborted: C_GetChatSettings dereferenced nil lifecycleState.clientSnapshot() before C_NewClient. Commit 6591d508dc80f78cef898e874ceefc677fe65b47 guards the pre-client/store lookup and adds an isolated FFI regression; release build/artifact and GREEN CI pending. Independent source review did not cover this integration failure.
L24. Scoped independent verifier rechecked 6591d50 and found no reproducible issue: the guard returns a zero/not-found setting before client initialization, preserves initialized lookup/field mapping, and the standalone test covers the formerly crashing public FFI call. No local Cargo was run.
L25. GitHub Actions run 38024106477 on 6591d508dc80f78cef898e874ceefc677fe65b47 completed successfully: Go checks, rustfmt, 489 Rust unit tests, all integration tests (including preclient chat settings and communities), Linux release build, and preview artifact 11659953080. Non-fatal cache/runner-image annotations only. T2 complete; T3 live archive events remain.
L26. Pinned whatsmeow 662ad1dc6900 persists archive state before dispatching events.Archive; full sync suppresses typed events but emits AppStateSyncComplete for regular_low. T3 RED tests require both signals to invalidate the active chat view, while regular completion retains its previous behavior. GitHub Actions run 38025365802 on test-only commit 1dedeb23b3486fd32e29fdc8ded567eb554fa3f7 observed RED in Go bridge checks: undefined archiveRefreshEvent.
L27. Event bridge now sends one payloadless ArchiveChanged event for typed Archive mutations and regular_low full-sync completion, decodes it in Rust, and invalidates/reanchors the current Chats projection. Existing regular completion behavior stays intact. Tests cover classifier boundaries, protocol kind, wiring, FFI decoding, and in-place All/Archived refresh; GREEN CI pending.
L28. Independent read-only verifier found a reproducible regression at b9194c1: ArchiveChanged reanchors shared chat_list_state from Chats rows even while Communities is selected, clearing an unchanged community selection when All contains no chats. Added a Communities-section regression with an archived group and no visible Chats; expected RED via Actions before limiting reanchoring to Chats. The verifier found no ABI or regular-completion issue; CI on b9194c1 remained pending.
