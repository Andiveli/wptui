# Issue 12: start a chat by new number
Feature branch: `preview/issue-12-saved-contacts`. Verification: GitHub Actions preview workflow; no local Cargo.

## Specs
S1. "Sigamos, investiguemos si es posible escribir a un numero nuevo, debemos ver si whatsmeow nos permite verdad"; "Okay, vamos a eso entonces"
S2. "recuerda seguir las convenciones del proyecto: una sola responsabilidad por archivo, y clean y hexagonal architecture"
S3. "siempre se tiene usar una rama preview/ para poder usar las github actions y no usar cargo en local porque mi sistema va limitado"; "continua con lo tuyo sin usar cargo en local"

## Tasks
T1 [S1,S2] blocked (waiting for GitHub Actions verification) | route: inline | commit: 3848488 | Expose a typed WhatsMeow number lookup through Go FFI and Rust adapter, with isolated bridge tests.
T2 [S1,S2] in_progress (waiting for CI and independent verifier) | route: inline | commit: b82e8c0 | Validate entered number, perform lookup off the UI thread, open only a registered recipient, cover UI/result behavior, and update help/docs in the same work unit.
T3 [S1,S3] pending | route: inline | commit: pending | Verify through preview GitHub Actions; fix failures and record evidence.

## Log
L1. "Okay, vamos a eso entonces, recuerda seguir las convenciones del proyecto: una sola responsabilidad por archivo, y clean y hexagonal architecture"
L2. "No hagas cabrón, tenemos puesto que siempre se tiene usar una rama preview/ para poder usar las github actions y no usar cargo en local porque mi sistema va limitado"
L3. Investigation: pinned WhatsMeow supports `IsOnWhatsApp(ctx, []string)` for international `+` numbers, returning `IsIn` and a JID; the existing saved-alias worker is not an entry point for unsaved numbers. Opening a JID already creates an in-memory conversation without sending.
L4. T1 bridge commit 3848488; preview run 41099292549 passed Go bridge and Rust formatting, then GitHub jobs API returned HTTP 502 twice while Rust tests were running. The run endpoint still reported in_progress. Continue T2 without local Cargo; verify T1 when API recovers.
L5. T2 commit b82e8c0 covers application flow, fake-port tests and README. Work-unit slice boundary: 3848488 is the Go/Rust bridge (~258 lines); b82e8c0 is the app/worker slice (~431 lines), not one accumulated review candidate. Independent verification required for item 4 (concurrency). Preview run 41101600922 queued. Native assessment on the T2 slice returned independentVerifier=true and reviewDue=false; RDD remains off.
L6. Independent verifier found stale queued B can block new C while A is running. Replace the single queued request in place and prove with a deterministic held-worker test; preserve stale response generation checks. Recheck CI on the corrective commit.
