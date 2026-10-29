# macOS PR CI and media FD portability
Branch: ci/macos-pr-suite (from origin/main c5b1e7641406592252c006159063bf4853e2e409).
Scope: PR test coverage and issue #482; issue #471 remains separate.

## Specs
S1. "Okay, primero agreguemos mac en las pr antes de seguir al fix de la carrera"
S2. "El CI no se ejecutaba ya para mac? Si le damos soporte debería hacerlo, no? Pasar la suit por ahí?"
S3. "CI macOS + corregir #482"

## Tasks
- [x] T1 [S1-S2] route:inline commit:see L7 — Configure supported build/test suite for macOS PRs; adapt installer fixture and describe coverage.
- [ ] T2 [S3] route:inline commit:pending — Make inherited media FD argument usable on macOS without weakening pinned-file behavior; test public launch plan.
- [ ] T3 [S1-S3] route:verify commit:pending — Assess security and CI diff, verify platform checks, record open proof and delivery decision.

## Log
L1. User request verbatim: "Okay, primero agreguemos mac en las pr antes de seguir al fix de la carrera"
L2. Prior user clarification verbatim: "El CI no se ejecutaba ya para mac? Si le damos soporte debería hacerlo, no? Pasar la suit por ahí?"
L3. User recall verbatim: "Además creo que sobre el directorio que mencionas abrimos también un issue, no?"
L4. User selected exactly: "CI macOS + corregir #482"; distinct from issue #471.
L5. Existing RED: macOS Actions 40831795294 failed tests/media_lifecycle.rs on missing /proc/self/fd; issue #482 records failure. Ordinary PR CI currently Ubuntu-only and release macOS job builds without tests.
L6. Do not run local Cargo, push, or create PR without separate permission; leave local sticker documentation commits unpublished.
L7. T1: Added ubuntu-latest and macos-latest PR test matrix, platform dependencies, target-aware installer archive assertions, and README coverage. Local `bash -n tests/install_archive.sh`, `bash tests/install_archive.sh` (Linux: passed), PyYAML structural CI check, and `git diff --check` passed. macOS and full Rust/Go proof pending T3; T1 commit SHA to follow.
