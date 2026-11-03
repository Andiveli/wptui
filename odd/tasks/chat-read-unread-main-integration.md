# Contact read/unread integration
Integrate the verified local contact-menu behavior into main while preserving existing remote attempts.
Refs #517. Remote WhatsApp synchronization remains unresolved; this delivery must not close that issue.

## Specs
S1. "Hm, si vamo a dejarlo así. Lo vamos a mergear, pero no daremos como terminado, sino como mark read only in local,"
S2. Clarification selected "Conservar los intentos actuales" and "main".

## Tasks
- I1 [S1-S2] parent: establish the draft integration tracker; in progress; commit pending.
- I2 [S1-S2] parent: integrate the complete contextual local action and its coupled bridge tests; pending; historical boundary 28e4e72.
- I3 [S1-S2] parent: integrate opt-in redacted diagnostics; pending; historical boundary 7eb12df.
- I4 [S1-S2] parent: integrate exported typed error classifications; pending; historical boundary c7a15af.
- I5 [S1-S2] parent: integrate structural secondary-cause classifications; pending; historical boundary 3f25231.
- I6 [S1-S2] parent: integrate bounded official LTHash recovery; pending; historical boundary 5b5dd10.
- I7 [S1-S2] parent: integrate stage diagnostics and the explicit local-effect limitation; pending; historical boundary ff5be56.
- I8 [S1-S2] parent: verify the final integration head and merge to main without closing #517; pending.

## Log
L1. User disposition verbatim: Hm, si vamo a dejarlo así. Lo vamos a mergear, pero no daremos como terminado, sino como mark read only in local,
L2. Structured clarification selected "Conservar los intentos actuales" and "main". This does not authorize disabling remote sends or claiming successful phone synchronization.
L3. Delivery uses the previously selected feature-branch-chain strategy. Keep the tracker draft until child review and integration are complete. Early test-only checkpoints are not independently deliverable slices. The base action slice is larger than the advisory review budget because it retains coupled behavior and its tests; no hard repository size exception is required.
L4. The source-stamped diagnostic preview at 5f5bbbe passed full preview CI and scoped independent static Go/Rust privacy review. That evidence does not substitute for CI on the current-main integration head or establish remote service success.
L5. Main integration rehearsal at 8de52bc combined main d6c1a57 and feature ff5be56 without conflicts. Preserve the original feature worktree, binaries, reports, and local tracking notes. No local Cargo verification is permitted.
