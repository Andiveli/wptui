# Animated stickers

Branch: `preview/animated-stickers`. Animate WhatsApp stickers in supported terminals, preserving a still-image fallback.

## Specs

S1. "Quiero agregar movimiento a los stickers que tiene movimiento, obvio, depende de la terminal, pero para las terminales soportados que haya movimiento."
S2. "Podemos copiar o basarnos en como lo hace concord (~/Escritorio/Programacion/personal/concord), entiendo que también muestra movimiento en stickers y con el fork que le tengo hasta en los iconos de los servidores"

## Tasks

- [x] T1 (S1-S2): Decode and bound animated sticker frames; retain still-image behavior for other media. Route: bounded writer. Commit: bcec782 (preview run 38993765636 passed).
- [x] T2 (S1): Schedule visible animated sticker playback and render current frame via negotiated terminal protocol, with focused tests and user documentation. Route: bounded writer. Commits: 0c30f67, 991b24e, d287aab, dc28dee, fb15c60 (preview run 39003693304 passed).
- [x] T3 (S1-S2): Verify feature and shared-path regressions, assess risk and close. Route: parent and independent verifier for item 4. Commit: final documentation slice (Actions 39003693304 and artifact installed).

## Log

L1. Quiero agregar movimiento a los stickers que tiene movimiento, obvio, depende de la terminal, pero para las terminales soportados que haya movimiento. Podemos copiar o basarnos en como lo hace concord (~/Escritorio/Programacion/personal/concord), entiendo que también muestra movimiento en stickers y con el fork que le tengo hasta en los iconos de los servidores
L2. User authorized read-only access to `/home/samael/Escritorio/Programacion/personal/concord`.
L3. Concord uses bounded decoded frames with delays and deadline-driven playback; local preview uses a bounded protocol cache and an event-driven draw loop.
L4. User requires a separate worktree, a preview/ branch for the test binary, and GitHub Actions rather than local cargo. Active worktree: `.worktrees/animated-stickers`; preview commit: bcec782.
L5. Preview run 38993765636 passed Go bridge, Rust format/tests, release build and artifact for bcec782. Integration commit 0c30f67 triggered run 38995664480, which failed to compile at `src/app/media_cache.rs:50` (mutable reference escaping `FnMut`). Replaced the iterator with an explicit loop in 991b24e; rerun pending.
L6. Independent verifier found four-slot animation eviction churn when five stickers are visible, unbounded animated decode work in media jobs, and covered stickers continuing to schedule animation behind the attachment viewer. Fixed with stable first-frame overflow, two in-flight animation slots, and viewer pause; CI proof pending.
L7. GitHub API returned HTTP 403 rate limit during status lookup for the 991b24e preview run; access recovered. Run 38996009693 passed for 991b24e. Run 38996831532 passed for d287aab: Go bridge, Rust format/tests, release build, package and artifact upload all successful. A live-terminal visual check remains for the user; no cargo ran locally.
L8. Review slices: bcec782 decoder; 0c30f67 playback; 991b24e compilation fix; d287aab resource/overlay fix; 2f312ab documentation close. Authored feature diff reached 558 lines, so review these slices separately. Risk: item 4 (runtime animation scheduling and concurrent media jobs).
L9. User correction: "Me funcionó en un sticker, en los demás no, raro". Reopened T2 and T3. Reproduction hypothesis from src/app/runtime_media_viewer_events.rs: try_acquire returns None after two concurrent loads, falls back to still image and marks preview Loaded permanently; no retry when a slot frees. Do not read real user media outside project.
L10. RED: commit 1534d84, preview run 39001969318, Rust tests exit 101: `app::media_cache::tests::simultaneous_sticker_loads_eventually_animate_after_worker_slots_free` FAILED with `stickers/animated-2.webp remained a still preview`; 533 other tests passed. Preserve installed previous binary until validated replacement.
L11. Bounded deduplicated pending sticker queue implemented for exhausted animation slots; preview results retry pending requests, reject stale message/path, and retain static fallback for unsupported media or 256-queue/four-animation overflow. Added ordinary-image and queue-saturation regressions.
L12. First GREEN attempt, preview run 39003071385 at dc28dee, failed Rust test compilation (exit 101, E0283 in `app.metadata.get(&"overflow".into())`); fixed explicit `wr::MessageId` in fb15c60. Run 39003693304 passed Go bridge, Rust formatting/tests (including three-sticker regression), release build and artifact upload at fb15c60. Independent verifier reviewed queue/slot correction; prior compile error addressed and CI supplied missing proof.
L13. Validated sole x86-64 ELF tar member from artifact 12089464412 at exact fb15c60 and atomically replaced ignored `.worktrees/animated-stickers/target/wp-tui`, SHA-256 `772a791fb6e84122eb7bb7fba37f52cd51444c4a1540044df23c77298cd21b6c`; old binary preserved at `target/wp-tui.2f312ab`. No Cargo local or global binary changes. User live-terminal retest remains outstanding.
