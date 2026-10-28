# Notification sound delivery to main

Issue: #463; base: origin/main at 286eed4; strategy: feature-branch-chain.
Source specification: `odd/tasks/configurable-notification-sound.md` (included with child slices).

## Review slices

1. Config migration and legacy fallback: 0810cba..f165dad, ~196 lines.
2. Sound selection, codecs and lockfile: 8f7a28b..d7c72b4, ~826 lines (Cargo.lock alone: 522; inseparable from dependency change).
3. Bundled default sound and license packaging: 15ec8a5..1c33a52, ~167 lines.
4. Updater compatibility and archive safety: 8c9e977..3eb9550, ~352 lines.
5. Startup-backlog audio gating and closeout: 4ed0936..bf98f32, ~158 lines.

Children #465–#469 are integrated into draft tracker #464 at 4dbdf0b. Final exact-head CI is still required before main. Every child references issue #463 and keeps its tests and documentation with the behavior. No PR is allowed to bypass failed checks. Main's reviewer exception applies only to the missing external approval for this integration.
