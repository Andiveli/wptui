# Issue #462: lazy panic report, no log panel
Base: origin/main 7c6a5df; branch: fix/issue-462-crash-diagnostics.
Delivery: local worktree only; no push, PR or merge authorized.

## Specs
S1: "Whenever I enter wptui, it litters an empty `wptui-crash.log` file in the directory from which wptui was launched. Is there any way to disable this behaviour?" — no empty crash report at startup; preserve report generation on panic.
S2: "Okay, hagamos lo que dices, nosotros lo creamos al inicio del programa para debuguear problemas que no veíamos, ya hasta se podría ir, y de hecho si se va el panel de logs, no se necesita"; "Informe solo ante panic; quitar panel" — retain panic report but create it only on panic; remove in-app log panel and its Ctrl+Shift+L shortcut.
S3: "This happens regardless of whether I have pressed Ctrl+Shift+L to toggle logs (although that doesn't have any visual effect, so maybe that isn't working either)." — panel/shortcut removal is separate from panic report behavior.

## Tasks
T1 [S1,S2] inline; done; commit: this work unit's commit; proof: public crash reporter helper subprocess RED 1/2 failed (`install created an empty crash report`), then GREEN 2/2 passed in isolated worktree — add a public-interface regression proving startup does not create/truncate a report and a panic still writes it, then make crash reporting lazy.
T2 [S2,S3] inline; in_progress; commit: pending — remove panel/shortcut, update affected tests/help/docs, preserve ordinary layout/input behavior.
T3 [S1-S3] inline/independent verification if risk item 3; pending; commit: pending — run focused and full checks, audit shared layout/input contracts and report runtime limitations.

## Log
L1. Investiguemos el 462
L2. Okay, hagamos lo que dices, nosotros lo creamos al inicio del programa para debuguear problemas que no veíamos, ya hasta se podría ir, y de hecho si se va el panel de logs, no se necesita
L3. User selected "Informe solo ante panic; quitar panel".
L4. Source inspection: src/main.rs installs crash diagnostics at startup with default relative wptui-crash.log; src/crash_diagnostics.rs::install currently creates and truncates the file immediately. Ctrl+Shift+L only toggles in-app logs; macOS/Ghostty key delivery was not proved. Existing untracked root wptui-crash.log belongs to another worktree and must remain untouched.
L5. Read-only explorer mapped all panel references in app actions/input/shortcuts, shared UI layout and GoLog draw paths, relevant tests and docs; retain crash breadcrumbs and panic-time write. Base worktree created at origin/main 7c6a5df, clean except this feature document. Local disk had 6.5 GiB free before checks.
L6. Added tests/crash_report_lifecycle.rs exercising public crash_diagnostics::install in an isolated subprocess for absent and pre-existing report, then an intentional caught panic. `CARGO_BUILD_JOBS=2 cargo test --locked --test crash_report_lifecycle -- --test-threads=1` RED exit 101: helper failed because install created an empty crash report; after removing eager create/truncate in src/crash_diagnostics.rs and updating hidden CLI help in src/main.rs, focused GREEN exit 0 (2 tests passed). Existing root wptui-crash.log untouched; no full app launched.
