# Pinned Whatsmeow Message-Action Audit

Audited module: `go.mau.fi/whatsmeow v0.0.0-20260721154117-8b4a8ba0d318`
(2026-07-21, commit `8b4a8ba0d318`).

`main_test.go` compile-checks and executes these exact client methods:

| Operation | Supported contract | Semantics | Verdict |
|---|---|---|---|
| Reaction | `(*Client).BuildReaction(chat, sender, id, reaction)` | Produces `ReactionMessage`; newsletter targets require `NewsletterSendReaction` instead. | Supported for ordinary messages |
| Edit | `(*Client).BuildEdit(chat, id, newContent)` | Produces an `EditedMessage` future-proof wrapper containing a `ProtocolMessage_MESSAGE_EDIT` with replacement content. | Supported |
| Revoke | `(*Client).BuildRevoke(chat, sender, id)` | Produces a `ProtocolMessage_REVOKE`; empty/self sender revokes own messages, and a group admin supplies the target sender. | Supported |

## Unpromised capabilities

The Rust/Go bridge exposes no audited operation for profile details,
reaction-user lookup, message pinning, or message starring. Keep these
capabilities hidden and unavailable in UI affordances; do not infer support
from chat settings or from unrelated whatsmeow APIs.

## Phase 4.2 boundary

Only reaction, edit, and revoke may be considered for paired additive bridge
operations after an explicit ABI mapping design. Preserve existing C/Rust
discriminants and do not expose the unpromised capabilities above.
