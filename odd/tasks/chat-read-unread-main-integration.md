# Contact read/unread integration
Pre-merge review checkpoint: integrate the verified local contact-menu behavior into main while preserving existing remote attempts. Final delivery state is recorded in PR #518.
Refs #517. Remote WhatsApp synchronization remains unresolved; this delivery must not close that issue.

## Specs
S1. "Hm, si vamo a dejarlo así. Lo vamos a mergear, pero no daremos como terminado, sino como mark read only in local,"
S2. Clarification selected "Conservar los intentos actuales" and "main".

## Tasks
- I1 [S1-S2] parent: establish the draft integration tracker; done; PR #518, initial tracker b5c085a.
- I2 [S1-S2] parent: integrate the complete contextual local action and its coupled bridge tests; done (child CI passed and PR #519 merged into tracker); historical boundary 28e4e72.
- I3 [S1-S2] parent: integrate opt-in redacted diagnostics; done (child CI passed and PR #520 merged into tracker); historical boundary 7eb12df.
- I4 [S1-S2] parent: integrate exported typed error classifications; done (child CI passed and PR #521 merged into tracker); historical boundary c7a15af.
- I5 [S1-S2] parent: integrate structural secondary-cause classifications; done (child CI passed and PR #522 merged into tracker); historical boundary 3f25231.
- I6 [S1-S2] parent: integrate bounded official LTHash recovery; done (child CI passed and PR #523 merged into tracker); historical boundary 5b5dd10.
- I7 [S1-S2] parent: integrate stage diagnostics and the explicit local-effect limitation; done (child CI passed and PR #524 merged into tracker); historical boundary ff5be56.
- I8 [S1-S2] parent: verify the final integration head and merge to main without closing #517; final gate tracked by PR #518 (not yet executed at this checkpoint).

## Log
L1. User disposition verbatim: Hm, si vamo a dejarlo así. Lo vamos a mergear, pero no daremos como terminado, sino como mark read only in local,
L2. Structured clarification selected "Conservar los intentos actuales" and "main". This does not authorize disabling remote sends or claiming successful phone synchronization.
L3. Delivery uses the previously selected feature-branch-chain strategy. Keep the tracker draft until child review and integration are complete. Early test-only checkpoints are not independently deliverable slices. The base action slice is larger than the advisory review budget because it retains coupled behavior and its tests; no hard repository size exception is required.
L4. The source-stamped diagnostic preview at 5f5bbbe passed full preview CI and scoped independent static Go/Rust privacy review. That evidence does not substitute for CI on the current-main integration head or establish remote service success.
L5. Main integration rehearsal at 8de52bc combined main d6c1a57 and feature ff5be56 without conflicts. Preserve the original feature worktree, binaries, reports, and local tracking notes. No local Cargo verification is permitted.

L6. Child PRs #519–524 passed CI on their exact heads and merged into the tracker in order. #523 required rerunning only failed jobs after an initial Linux linker SIGBUS; the second attempt passed on identical source. The original failure is retained as evidence, not erased.
L7. After the final child integration, the tracker tree matched statically reviewed candidate 59e67d6 exactly. This checkpoint update changes only this document; current-main required CI and final PR-head/threads/base guards remain required. #517 stays open; remote synchronization is not fixed.
