# Issue #21 review chain

Review mouse support, arrow-key parity, and the measured preview cache as one feature. The tracker PR targets `beta` in draft; its eight child PRs carry the code and tests in order. Integrate reviewed children into this branch before requesting the **one final merge to beta**. No child targets beta directly.

## Review path

| Child | Scope | Estimated changed lines |
|-------|-------|-------------------------|
| 1 | Arrow keys alongside `h/j/k/l` | 101 |
| 2 | Terminal mouse capture and cleanup | 431 |
| 3 | Painted-row pointer foundation and preview CI cache | 383 |
| 4 | Wheel routing and modal/picker visibility | 528 |
| 5 | Painted URL and media activation | 378 |
| 6 | Blank media-gap safety and default-on capture with opt-out | 273 |
| 7 | Enter-equivalent section/contact/message clicks | 330 |
| 8 | Clicks for the merged All/Unread/Groups/Archived filters | 129 |

The 431- and 528-line slices exceed the advisory 400-line target because their state, cleanup, hit coordinates, and tests belong together. This is a single cohesive slicing pass, not permission to omit tests or compress code. Review each child against its immediate parent, then review the accumulated tracker diff before the final beta merge.

## Verification and boundaries

- The combined preview at `4f4be2b4fa8565eebd479ec4f5f65c44667306b1` passed Go, Rust formatting/tests, Linux build, and artifact upload in Actions run [38297650292](https://github.com/Andiveli/wptui/actions/runs/38297650292). Each PR still needs its own applicable CI; the combined run does not prove every intermediate slice.
- The preview cache measured a same-branch cold 16m14 versus warm 11m37 (4m37, about 28% faster). Keep its workflow and measurement with #21.
- A read-only independent reviewer found no S1–S4 blocker in the final filter-click routing. Static checks and terminal emulation do not prove real-terminal Shift-drag, selection restoration, or real-account behavior.
- Contact identity/PN↔LID work belongs to a different preview. Do not bring its commits into this chain or touch the old private #21 binary.

**Stop:** child PR review does not authorize integrating the tracker into `beta`, a direct beta push, an issue closure, or a global installation. Obtain a separate merge decision after the chain is reviewed.
