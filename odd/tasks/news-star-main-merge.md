Integrate the user-tested News repository-star feature into main without changing its approved behavior.
Target: github.com/Andiveli/wptui, default branch main; existing source preview/news-repository-star at a4809dc6df1964b97c27b53824e961c42295ee1b.
Delivery: feature-branch-chain (owner-selected); Rust verification remote only; main requires current build-and-test, resolved conversations and an up-to-date base. Never run Cargo locally.

## Specs
S1. "Okay, he probado el flujo y está todo correcto, puedes mergearlo a main".
S2. "Sí, crear y aprobar el issue" — selected exact approval action: create-and-apply-status-approved, authorizing creation of the News star issue with `status:approved`.
S3. Preserve previously selected delivery strategy: "Feature/tracker branch chain — recommended" (feature-branch-chain).

## Tasks
T1 [S1-S2] route=parent status=in_progress commit=none — Confirm repository policy, refresh base, create/read back approved feature issue and prepare source-equivalent branch chain.
T2 [S1-S3] route=parent status=pending commit=none — Publish tracker and coherent child PRs; verify exact heads through applicable CI, resolve any in-scope failures.
T3 [S1-S3] route=parent status=pending commit=none — Integrate children and tracker after required checks, binding merges to verified heads; update local main safely and record final merge evidence.

## Log
L1. Original request verbatim: "Okay, he probado el flujo y está todo correcto, puedes mergearlo a main".
L2. User explicitly authorized issue creation and the protected `status:approved` label through the structured choice: "Sí, crear y aprobar el issue". Authenticated actor has admin/maintain permission on github.com/Andiveli/wptui. No matching open/closed star issue found. Available labels include status:approved,type:feature.
L3. Verified source a4809dc passed full preview run41932259079, and the human confirmed the complete flow. Original base f5ff0fb; remote main has advanced to 9ad6957, refresh pending. Existing review branches inherit a compile-RED commit and cannot be used as independently green backend PRs without rebuilding new clean branch history; no force pushes or resets authorized.
L4. Main protection: strict build-and-test, one independent approval, required resolved conversations, administrator enforcement off. Project-local owner rule allows --admin solely for unavailable independent approval after every other check passes, with exact head binding. No protection settings may be changed.
