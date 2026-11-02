# Contact chat read/unread actions
Objective: enable the existing contact-list read/unread action. Branch: `preview/chat-read-unread` in `.worktrees/chat-read-unread`.
Delivery: auto-chain, chain_strategy=feature-branch-chain (human selected); revised forecast exceeds 400 authored lines. Slice 1: sync bridge; slice 2: menu and projection. No PR/merge yet. Tests: Go bridge tests and remote `.github/workflows/preview.yml`; never run Cargo locally.

## Specs
S1. "En la lista de contactos tenemos space a para ver las acciones que podemos hacer sobre ese chat, verdad? Quiero empezar a implementar eso, de primeras tenemos el mark read/unread"
S2. "investiguemos que nos ofrece whatsmeow y vamos a eso"

## Tasks
- T1 [S1-S2] delegated (context backstop): add behavior tests and observe RED via preview CI; in_progress; tests prepared, RED unobserved; commit: pending.
- T2 [S1-S2] delegated (context backstop): implement selected-chat read/unread using app-state, preserve receipts/cursors and document shortcut; pending; commit: pending.
- T3 [S1-S2] inline assessment and checks; independent verifier only for named high risk: observe preview GREEN and close; pending; commit: pending.

## Log
L1. User request verbatim: En la lista de contactos tenemos space a para ver las acciones que podemos hacer sobre ese chat, verdad? Quiero empezar a implementar eso, de primeras tenemos el mark read/unread, investiguemos que nos ofrece whatsmeow y vamos a eso
L2. Exploration confirmed Space+a and planned contextual `m` action. Whatsmeow appstate.BuildMarkChatAsRead accepts read bool; SendAppState sends it. Client.MarkRead is message receipts and is not the unread toggle. Existing bridge hardcodes true. Spot check: read_state.rs rejects remote read=false and Go event adapter requires a message range; empty-chat behavior must be resolved technically against the pinned dependency, not guessed.
L3. Route: one coupled behavior, no independent writer units; runtime context backstop fired, requiring a bounded writer. Preserve the existing read cursor, automatic reading, status exclusions, worker lifetime/ordering, filtered selection and unrelated files. Technical assumptions: dynamic read/unread label, no synthetic message count for manual-unread; disable only if the pinned API genuinely cannot represent missing identity. No persistent storage migration is authorized.
L4. Original checkout is main at 6c15d48, with unrelated untracked files preserved. Created and registered isolated worktree and preview branch. Rust checks must run remotely; no local Cargo including fmt. Parent owns commits and pushes; no PR/merge requested.
L5. T1 worker returned tests only: 335 added lines across contextual_activation.rs, whatsapp_events/tests.rs and chat_read_sync_ffi_test.go. Parent reviewed diff and diff --check passed. No RED observed yet. Pinned builder tests cover nil message keys for empty chats; server acceptance remains untested. Whole-feature forecast now exceeds 400 authored lines; ask-on-risk delivery-shape decision required before first commit. Outgoing read=false bridge test will be added with the additive API in T2.
L6. Human selected: "Cadena sobre rama de feature — recomendada". Delivery strategy auto-chain with feature-branch-chain; separate bridge and menu/projection slices. Test-only RED commit is a transient preview checkpoint; final review slices keep tests with their behavior. No main mutation or PR/merge.
