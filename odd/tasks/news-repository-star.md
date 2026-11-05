Implement the approved News repository-star accent row in the real Ratatui stack.
Branch: preview/news-repository-star; base: f5ff0fb426d3bfb5201937aea8bceb699e1d7fe7. Delivery: auto-chain; chain_strategy=feature-branch-chain (owner selected). Forecast 500–700 authored lines including tests/docs. Preview branch is the feature/tracker; review slices will separate the gh/state backend from News UI/input integration where coherent.
Verification: remote .github/workflows/preview.yml only for Rust; never run local Cargo. Artboard remains at the registered main worktree's design/news-repository-star/01-inline-controls.html.

## Specs
S1. "Quiero agregar en ese espacio una forma de darle estrella al repo mediante gh cli"; "este muestra un mensaje si no se ha dado estrella al repo, quiero replicar eso mismo, pero dentro de las news para no molestar".
S2. "Deberíamos destacarlo más, ahora parece información que los usuarios no van a leer". Visual spec: design/news-repository-star/01-inline-controls.html, including its approved accent row, primary star action, secondary dismissal, responsive wrapping and simulated state semantics.
S3. "Implement it in the project's real stack. Use the artboard as the visual spec: map its parts to the project's existing components and styles, and do not copy the artboard HTML verbatim."
S4. "Report progress with the gentle_design tool: action \"handoff\", target \"news-repository-star/01-inline-controls.html\", status \"in_progress\" when you start; status \"implemented\" when it is done, with previewUrl set to the running app's loopback URL (localhost, 127.0.0.1 or [::1]) when one is running, so the canvas can compare it with the artboard."

## Tasks
T1 [S1-S3] route=delegated status=done commit=a059384 — Remote RED observed in run 41925943490: E0609 missing repository_starred at src/ui/news.rs:207,263; exit 101. Formatting, Go bridge and installer checks passed. — Add deterministic News star regression tests and observe remote RED; preserve existing News behavior.
T2 [S1-S3] route=delegated status=done commits=768e122,1fb4c5c — Remote GREEN at exact source/document candidate 3d75028914cada8d490a8c667ae74f84feeb97f3, run 41928786039: formatting, all Rust tests, Go bridge, installer, release build and preview artifact passed. Review slice 01: feat/news-repository-star-01-backend at 768e122, 383 authored lines. Review slice 02: feat/news-repository-star-02-ui at 1fb4c5c, 417 authored lines. Initial RED tests: a059384. No PR created.
T3 [S1-S4] route=parent+independent-verifier status=done commit=3d75028 — Concurrency item 4 escalated through ASSESS; independent verifier found no concrete blocker at 3d75028914cada8d490a8c667ae74f84feeb97f3. Diff checks and remote tests/build passed at exact verified source candidate; S4 handoff marked implemented. No running loopback app or live terminal screenshot; native TestBackend rendering and interaction tests passed. No local Cargo or real GitHub star request executed. Native candidate unavailable due untracked files; RDD remains off. Nonblocking advisory: temporary stdout capture has no byte cap, although reads and process duration are bounded. No optional changes or additional verification added.

## Log
L1. Original request verbatim: "Okay, vamos a empezar, tenemos en wptui la sección de news ahora, verdad? Quiero agregar en ese espacio una forma de darle estrella al repo mediante gh cli, como lo hace ~/Escritorio/Programacion/personal/gentle-pi, este muestra un mensaje si no se ha dado estrella al repo, quiero replicar eso mismo, pero dentro de las news para no molestar"
L2. User authorized reading /home/samael/Escritorio/Programacion/personal/gentle-pi and updating main; fast-forward completed to f5ff0fb. Design-only preview was completed before implementation authorization.
L3. User comment verbatim: "Deberíamos destacarlo más, ahora parece información que los usuarios no van a leer". Applied to selected artboard; comment qx1gkqfh resolved.
L4. Implementation authorization verbatim:
Implement the design artboard design/news-repository-star/01-inline-controls.html

Artboard: design/news-repository-star/01-inline-controls.html
Title: Inline controls · Recommended
Tradeoff: Most discoverable without changing release content; costs one control-area row at wide widths.

Implement it in the project's real stack. Use the artboard as the visual spec: map its parts to the project's existing components and styles, and do not copy the artboard HTML verbatim.

Report progress with the gentle_design tool: action "handoff", target "news-repository-star/01-inline-controls.html", status "in_progress" when you start; status "implemented" when it is done, with previewUrl set to the running app's loopback URL (localhost, 127.0.0.1 or [::1]) when one is running, so the canvas can compare it with the artboard.
This design is not linked to an ODD feature document: size it under the project's ODD rules as usual.
This handoff itself commits nothing.
L5. Isolated registered worktree created at .worktrees/news-repository-star on preview/news-repository-star. Production source untouched so far. Large classification: remote RED/GREEN waits and independent concurrency verification require resumable evidence beyond the source diff. Risk: item 4 (background gh completion and shutdown lifecycle). gh authenticated; no real star operations authorized as tests or performed.
