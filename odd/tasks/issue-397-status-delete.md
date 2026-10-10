# Issue 397: Delete one own published status
Base: `beta` at `3bdcb62`; branch: `preview/issue-397-status-delete`; no beta/main push, PR or release for this feature without a separate delivery decision.
Checks: GitHub Actions for Go/Rust builds and tests, never local Cargo/Go; no agent-operated live status deletion; preserve unrelated `.pi/` and `wptui-crash.log`.

## Specs

- S1: "Ahora sí, vamos al delete"
- S2: "Un estado propio elegido (recomendado)". Delete one selected own published update, not every update.
- S3: "Como si se tratara de eliminar un mensaje, así mismo". Follow the existing message-selection and `d` deletion interaction, without inventing an unrelated batch flow.
- S4: "Ahora se eliminó. Pero en wptui sigo viendo el estado como si estuviera ahí.". Previous beta delivery accepted that stale own row; this new deletion work must not misrepresent a failed revoke as success.

## Tasks

- T1 [S1-S4] in_progress; route inline, one dependent behavior-first work unit: guard a canonical selected own published update, invoke the existing revoker only there via `d` in own-status navigation, project successful deletion out of own status rows, preserve other-contact read-only path and existing chat delete. Tests and help accompany behavior in a Conventional Commit; Go bridge changes only if existing contract proves insufficient.
- T2 [S2-S4] dropped; merged into T1 because public input routing and success-only projection cannot be functionally verified separately from the selected revoke behavior.
- T3 [S1-S4] pending; route independent verification after cloud tests, then human-controlled disposable-status recipient test before claiming deletion for everyone; record proof and next delivery decision in a work-unit commit.

## Log

L1 user verbatim: "Ahora sí, vamos al delete"
L2 user selected: "Un estado propio elegido (recomendado)" rather than deleting all own published statuses.
L3 user verbatim: "Como si se tratara de eliminar un mensaje, así mismo"
L4 evidence: `beta` push `4369d53` passed GitHub Actions run 38011022720 (Go/Rust checks, Linux artifact); beta doc-only follow-up `3bdcb62` is being verified separately. Branch created at `3bdcb62`. The own-status composer already has a Navigating mode: Esc selects one message, j/k changes selection. `DeleteMessage` is currently blocked in that mode and in other contacts' status views. Ordinary message deletion has no separate confirmation prompt, calls the revoker, and records local deletion only on success; it currently supports own text messages. Pinned Whatsmeow has a generic BuildRevoke for an own message and resolves status-broadcast recipients per send; this does not prove recipient-side status deletion. No source edits, live deletions, or local Cargo/Go checks yet.
L5 plan update: T1/T2 merged into one behavior-first work unit because `d` routing, selected-target validation, and success-only local projection form one public action; a transport-only or projection-only commit cannot demonstrate the requested operation. Existing `MessageRevoker` and pinned own-message BuildRevoke can be reused if an isolated test proves the status-specific key. No ordinary-chat behavior should change; tests cover own published text/media, a rejected foreign/pending target, failure preserving local rows, and unchanged chat and contact-view guards. No tests have run on this new feature.
L6 behavior-first integration tests in `tests/status_section.rs` now exercise the public `d` key on one selected own published media update, exact status broadcast/chat sender/canonical ID call, absence from own/author rows only after success, duplicate/foreign/unpublished rejection, failed revoke preserving the row with an explicit failure notice, and previous other-contact status delete rejection. Direct rustfmt --check and git diff --check passed. No local Cargo/Go by user preference; RED is not claimed until a cloud preview run proves the expected behavior failure.
