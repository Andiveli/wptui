# Animated stickers integration

Branch: `preview/animated-stickers` is the tested source; deliver via a feature/tracker PR chain based on verified `main`. No merge is authorized.

## Specs

S1. "Dale, refactoricemos eso antes de integrar"
S2. "Cadena de feature con rastreador — recomendada"
S3. "Sí: crear issue de stickers y aprobarlo"
S4. "Sí, push de ramas y creación de PRs"

## Tasks

- [x] T1 (S1, S3): Document the inherited path race separately and create the relevant approved feature issue. Route: parent. Commit: none (remote issues #471 and #472; approval verified).
- [x] T2 (S1-S4): Plan cohesive review slices of the already-tested feature and its architecture cleanup; record target branch and conflicts. Route: parent. Commit: pending tracking-document commit.
- [ ] T3 (S2-S4): Create the tracker branch from current default branch, publish a draft/no-merge tracker PR linked to #472. Route: parent. Commit: pending.
- [ ] T4 (S1-S4): Publish ordered child PRs with one coherent slice per PR and clean base-relative diffs. Route: parent (exploration subagent unavailable). Commit: pending.
- [ ] T5 (S1-S4): Read back PR metadata and GitHub Actions checks; report failed, skipped or pending proof and defer merge. Route: parent. Commit: pending.

## Log

L1. The compacted request was reported as: "Abrir un issue sobre lo que se acababa de discutir e integrar el refactor." Its original literal message is unavailable after compaction; do not present this reconstructed sentence as a verbatim quote.
L2. User selected verbatim: "Preparar PRs encadenados hacia main", "Cadena de feature con rastreador — recomendada", "Sí, ese repo y esa cuenta", "Sí: crear issue de stickers y aprobarlo", and "Sí, push de ramas y creación de PRs". Integration means prepare published PRs, not merge.
L3. Target verified as github.com/Andiveli/wptui, actor Andiveli (ADMIN), default branch main. Issue #471 separately tracks the inherited local-path race; issue #472 documents animated stickers, is non-duplicate and has status:approved after exact issue-number-bound authorization/readback. Children use Refs #472, final tracker uses Closes #472.
L4. `origin/main` advanced 40 commits beyond the preview branch's merge base daac878. Overlapping paths: README.md, src/app.rs, src/app/bootstrap.rs. The preview branch is three documentation commits ahead of remote; preserve it. No local Cargo. Existing preview CI 40817403510 passed 542 Rust tests, architecture checks, Go checks, fmt and release build; the user confirmed the refactored worktree binary works live.
L5. The read-only exploration subagent failed during startup with ENOENT; it did not read or mutate files. Continue the bounded mapping inline, report its unavailability and do not retry.
L6. One honest slicing pass, from the original tested commits: tracker carries this plan only; child 01 bcec782 decoder (~169 lines); child 02 0c30f67+991b24e playback (~296); child 03 d287aab+2f312ab+1534d84+dc28dee+fb15c60+383c16f resource/queue fix (~380); child 04 188cff3+2cecb9e+374a0c7+438b1d8+100ea03 protocol diagnostics and long-WebP sampling (~300); child 05 6ef4452+8fadc3f+7d25301+160fca2+accf3f4+59d2d26 visible four-slot rotation (~190); child 06 74eff59 visibility cleanup (~131); child 07 55bbc44 filesystem adapter (~185); child 08 bf02fe9+6679e69+b20864e+28330f0 decoder/playback split (~500 changed lines but ~35 net; moving pure decoder/test code must stay one cohesive change). Each child targets its immediate predecessor; CI must run at each final branch head, never at intermediate RED commits. Child 08 is the one unavoidable >400 review-budget slice; no code-golf or unrelated scope additions.
