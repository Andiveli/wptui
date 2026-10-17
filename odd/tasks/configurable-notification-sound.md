Objective: Move existing preferences into the OS configuration directory and allow user-selected notification sounds.
Branch: preview/configurable-notification-sound; delivery strategy: ask-on-risk; forecast: ~350-500 authored lines across two work units.
Test runner: GitHub Actions only (no local Cargo); local structural checks may use git diff --check.

## Specs
S1: "primero haremos que sea más accesible sabes, como esos archivos que están en las config/" — check settings path and preservation of existing values when moving from data to config directory. Assumption: use ProjectDirs::config_dir and retain legacy settings on read without overwriting or deleting it.
S2: "quiero el predeterminado, pero quiero también que los usuarios lo puedan editar" and "el usuario pueda poner una ruta al sonido de la notificación" — check default notification sound and custom audio path behavior for eligible notifications, without sounding muted/open-chat/sync notifications.
S3: "soporte mp3 y ogg" — check WAV, MP3, OGG decoding and failed/missing file handling. Assumption: explicit format support follows the accepted recommendation to support all three.
S4: "esto va en un worktree para no usar cargo en local sino en las github actions" — work in registered same-clone worktree; no local Cargo; GitHub Actions require separate push authorization.

## Tasks
T1 [S1,S4] delegated (context backstop); in_progress; commit: 0810cba; proof: structural checks pass; CI pending — move settings lookup to OS config dir, keep backward compatibility, update tests and docs.
T2 [S2,S3,S4] delegated (context backstop); pending; commit: pending; proof: pending — sound choice, portable playback, error handling, tests/docs.
T3 [S1-S4] inline; pending; commit: pending; proof: pending — structural verification, CI authorization and results, final integration review.

## Log
L1. Quiero agregar sonido a las notificaciones
L2. Ambos, quiero el predeterminado, pero quiero también que los usuarios lo puedan editar, así ya empezamos a darle un poco de identidad mediante un archivo de configuración. un .conf o un .json para definir ciertas cosas, no lo tenemos aun, y me parece buena idea empezar por poder cambiar el sonido
L3. Okay, primero haremos que sea más accesible sabes, como esos archivos que están en las config/, me explico? y luego haremos que el usuario pueda poner una ruta al sonido de la notificación
L4. Vamos a eso entonces. Por supuesto, esto va en un worktree para no usar cargo en local sino en las github actions.
L5. Existing settings.conf already stores composer_direction and mouse under ProjectDirs::data_dir. Existing main worktree has unrelated edits; isolated new worktree was created at .worktrees/configurable-notification-sound. WAV/MP3/OGG proposed and accepted by "Vamos a eso entonces."
L6. User explicitly authorized publishing a preview/** branch to run CI and generate a downloadable test binary, without PR or merge. No local Cargo. T1 migration uses atomic no-clobber publication from the legacy file, with tests written but RED/GREEN unavailable locally; rustfmt and git diff --check passed. Commit 0810cba; CI pending. Native assessment reported unassessable because the feature document was untracked; conservatively treated as high risk item 1, independent verification required.
