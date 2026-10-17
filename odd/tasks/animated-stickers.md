# Animated stickers

Branch: `feat/animated-stickers`. Animate WhatsApp stickers in supported terminals, preserving a still-image fallback.

## Specs

S1. "Quiero agregar movimiento a los stickers que tiene movimiento, obvio, depende de la terminal, pero para las terminales soportados que haya movimiento."
S2. "Podemos copiar o basarnos en como lo hace concord (~/Escritorio/Programacion/personal/concord), entiendo que también muestra movimiento en stickers y con el fork que le tengo hasta en los iconos de los servidores"

## Tasks

- [~] T1 (S1-S2): Decode and bound animated sticker frames; retain still-image behavior for other media. Route: bounded writer. Commit: bcec782 (awaiting preview CI).
- [~] T2 (S1): Schedule visible animated sticker playback and render current frame via negotiated terminal protocol, with focused tests and user documentation. Route: bounded writer. Commit: pending.
- [ ] T3 (S1-S2): Verify feature and shared-path regressions, assess risk and close. Route: parent and independent verifier for item 4. Commit: pending.

## Log

L1. Quiero agregar movimiento a los stickers que tiene movimiento, obvio, depende de la terminal, pero para las terminales soportados que haya movimiento. Podemos copiar o basarnos en como lo hace concord (~/Escritorio/Programacion/personal/concord), entiendo que también muestra movimiento en stickers y con el fork que le tengo hasta en los iconos de los servidores
L2. User authorized read-only access to `/home/samael/Escritorio/Programacion/personal/concord`.
L3. Concord uses bounded decoded frames with delays and deadline-driven playback; local preview uses a bounded protocol cache and an event-driven draw loop.
L4. User requires a separate worktree, a preview/ branch for the test binary, and GitHub Actions rather than local cargo. Active worktree: `.worktrees/animated-stickers`; preview commit: bcec782.
