# Issue #358: search within the open conversation
Branch: `feat/issue-358-conversation-search` from `main` c8bda10; worktree: `.worktrees/issue-358-conversation-search`.
Delivery: local feature work units; no push, PR, release, or local Cargo authorized.

## Specs
S1. "Claro claro, solo debe funcionar / dentro del chat para buscar mensajes en el chat mientras no estoy en el composer y estoy dentro del chat, usamos n/N para movernos entre las coincidencias pero seguimos usando j/k para movernos en los mensajes. y resaltar el fragmento coincidente. Esto solo en el chat. Aún no vamos a incorporar la busqueda de mensajes globales, solo por chat."
S2. "Aclaramos que esto funciona solo si estamos parados en el historial de mensajes, en contactos / sigue buscando contactos. Ahora vamos a implementar"
S3. "No se puede usar cargo acá en local, se usan las github actions"

## Tasks
T1 [S1-S2] Route: inline; establish focus-gated conversation search input and public terminal-event tests preserving contact search and composer behavior; commit: pending.
T2 [S1-S2] Route: inline; navigate only matching messages in the current chat with n/N without altering j/k and verify selection/viewport via public tests; commit: pending.
T3 [S1] Route: inline; highlight matched text within rendered message body, document controls and limitations, test rendering/unchanged message styling; commit: pending.
T4 [S1-S3] Route: inline; inspect full diff and assess risk, run structural checks and GitHub Actions only after separate push authorization; independent verifier if high-risk; commit: pending.

## Log
L1. Revisemos el 358 y que tanto podríamos hacer
L2. Claro claro, solo debe funcionar / dentro del chat para buscar mensajes en el chat mientras no estoy en el composer y estoy dentro del chat, usamos n/N para movernos entre las coincidencias pero seguimos usando j/k para movernos en los mensajes. y resaltar el fragmento coincidente. Esto solo en el chat. Aún no vamos a incorporar la busqueda de mensajes globales, solo por chat.
L3. User selected "Solo definir el alcance" and no code was changed then.
L4. Aclaramos que esto funciona solo si estamos parados en el historial de mensajes, en contactos / sigue buscando contactos. Ahora vamos a implementar
L5. Issue #358 asks for in-conversation /, n/N, bring match into view and highlight; issue suggests SQLite LIKE. Inspection found locally hydrated in-memory messages and existing MessageListState selection/viewport. In-memory text-message search avoids a new database query/format, and deliberately does not claim remote/global-history coverage. Composer input and contact search already have higher-priority handlers; use the conversation-history focus gate. Main/root and the user's macOS-test work remain untouched.
L6. T1 wrote public terminal-event tests for slash in open-chat history, unchanged contact-list slash and composer slash. Structural RED before production edit: old on_terminal_event sent conversation slash through unbound global key routing, so no visible /jk search prompt existed. A runnable RED was unavailable because local Cargo is forbidden and test-only branch publication had not been authorized. T1 implementation adds chat-history-gated input state and one-row search prompt without touching contact or composer handlers; direct rustfmt --check and git diff --check passed. User then selected "Sí, usar la rama preview para #358", authorizing only this feature's commits on preview/issue-358-conversation-search for GitHub Actions, not PR, main, release or global binary. T1 CI not run yet.
