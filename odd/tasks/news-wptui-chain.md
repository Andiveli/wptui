# News wptui review chain
Objective: prepare the approved News feature as a feature-branch PR chain, without merging. Base: origin/main 6c15d48; tracker: feat/news-wptui.
Delivery: feature-branch-chain; three cohesive slices (~600/900/1000 authored lines, advisory overages justified below). Rust verification: remote CI only; never local Cargo/rustfmt/toolchain setup.

## Specs
S1. "Las pruebas de actualización y reconexión toca hacerlas cuando tengamos una una release, no? Por ahroa puedes preparar la cadena para mergear"
S2. "Okay, anota que debemos hacer en cada release que saquemos."
S3. "Claro, es lo que pensaba, dejar la de ahora sin información, pero de ahora en más si preparo la descripción"

## Tasks
C1 | S1,S2,S3 | parent + delegated assembly | in_progress | approve issue, commit existing release-checklist docs, prepare clean tracker and child slices from existing verified source without changing behavior; commits pending.
C2 | S1 | parent | pending | publish draft/no-merge tracker and three child PRs with approved issue, one type label each, correct immediate-parent bases and chain context; PRs pending.
C3 | S1 | parent | pending | observe target-policy CI on exact candidate branches and report blockers/ready status, without merge; checks pending.

## Log
L1. User request: "Las pruebas de actualización y reconexión toca hacerlas cuando tengamos una una release, no? Por ahroa puedes preparar la cadena para mergear"
L2. Prior delivery strategy selected by user: feature-branch-chain. Original feature spec/evidence: .worktrees/news-wptui/odd/tasks/news-wptui.md. Source tested by full preview CI41523345165 at ce39bba; binary retained in that original worktree target/preview/wp-tui. Independent static review found no code blockers; its executable rustfmt check timed out during implicit rustup setup, not retried. Do not relaunch that verifier or retry its setup; preserve limitation. Real install/reconnect E2E unobserved and explicitly disclosed; no release publishing/session/update action authorized here.
L3. User approved current v0.3.0 without notes; future release descriptions must also be bundled before build. Existing uncommitted CONTRIBUTING/README checklist belongs in this delivery, no historical note reconstruction.
L4. Remote verified github.com/Andiveli/wptui, default main, actor ADMIN, main unchanged at6c15d48. No matching News/release-note issues or News PRs found; unrelated updater issue448 is closed. Labels status:approved,type:feature exist. User selected exact authorization: "Sí, crear y aprobar la issue"; prompt named github.com/Andiveli/wptui, creation-time status:approved, Closes in integrator and Refs in children, no merge. Issue form feature_request.yml uses enhancement and Problem/Proposed solution/Alternatives considered; inspect existing form label before create.
L5. One honest slicing pass retained backend/history+tests, integrated section/navigation+tests, and exact-target update/restart+tests. Each exceeds advisory400; repository CONTRIBUTING/PR template recommend splitting, no hard size gate observed. Do not golf, remove tests, or invent size:exception. Original preview/source history and unrelated worktrees/artifacts must stay intact. Assemble new child branches without rewriting existing branches, exclude only original ODD bookkeeping from source patches, and verify code/docs equality with reviewed snapshots. New tracking doc owns delivery bookkeeping.
