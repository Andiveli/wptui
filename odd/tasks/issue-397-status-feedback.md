# Issue 397: Own-status send feedback
Branch: `preview/issue-397-status-feedback` from `preview/issue-397-status-publishing`; target: `beta`; no main push, PR, or release.
Checks: GitHub Actions only for builds/tests; preserve unrelated `.pi/` and draft/retry semantics.

## Specs

- S1: "Primero dice que está subiendo el estado ( si lo hace, no avisa) quiero que pase como con los mensajes, que se identifique con dos colores"
- S2: "Se supone tenemos \"un chat\" para nuestros estados, se van agregando a la cola como cuando enviamos mensajes en un chat normal, que primero es un color menos intenso que el color de cuando ya se envió"
- S3: "Sí, correlación fiable (recomendado)". Authorized scope: Go, Rust and UI, maintaining the existing transport contract rather than guessing a synced echo by content.
- S4: "Sí, worktree beta y rama preview remota". Work only in the authorized beta worktree; publish a preview branch to run checks on GitHub, not main or a release.

## Tasks

- T1 [S1-S3] in_progress; transport commit `d770375` + App/UI commit `dac6d13`; functional checks pending remote CI.
- T2 [S1-S4] pending; push preview commit, remote CI/build, independent verification and local manual validation; evidence pending.

## Log

L1 user: "Okay, ahora estoy probando subiendo estados. Primero dice que está subiendo el estado ( si lo hace, no avisa) quiero que pase como con los mensajes, que se identifique con dos colores"
L2 user: "Se supone tenemos \"un chat\" para nuestros estados, se van agregando a la cola como cuando enviamos mensajes en un chat normal, que primero es un color menos intenso que el color de cuando ya se envió"
L3 user selected: "Sí, worktree beta y rama preview remota"; authorized `/home/samael/Escritorio/Programacion/public/wptui-public-worktrees/beta` and remote preview branch.
L4 user selected: "Sí, correlación fiable (recomendado)"; approved Go/Rust/UI correlation for truthful feedback and no ambiguous content matching.
L5 evidence: #397 preview artifact 03dca11 passed Go/Rust CI; user reports live publishing succeeds but completion is not apparent. Status send currently returns uint8 only and discards the canonical ID from Whatsmeow SendResponse; chat sends emit a canonical-ID callback. Local Cargo/Go tests are disallowed by resource preference; CI will verify the candidate.
L6 plan update: Go transport callback, Rust batch IDs, App optimistic queue and UI rendering form one dependent behavior; merged the originally separate T1/T2 into one tracked task, split into transport and App/UI commits to keep review slices focused. A transport-only commit cannot be functionally verified without the App/UI counterpart. No runtime checks have run on this candidate yet.
L7 transport commit `d770375` (`feat(status): correlate publish responses with local send ids`), 4 files +180/-7; gofmt, rustfmt --check, git diff --check passed locally as static checks only. Cargo/Go tests not run locally; candidate correctness unverified pending GitHub CI.
L8 App/UI commit `dac6d13` (`feat(status): show queued own updates until confirmed`), 8 files +263/-14. Direct rustfmt --check and diff --check passed; no Cargo/Go test locally. Both commits form one cohesive feature of 443 added lines and must be reviewed as two slices to protect reviewer focus. No correctness claim before cloud CI.
