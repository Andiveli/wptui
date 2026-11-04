# Media previews: integration tracker

The accepted media-preview design is being integrated with current `main`,
including News/update support. This tracker introduces no feature code;
existing auto-update behavior is unchanged.

## Three review units

1. **Pre-send previews:** show queued or pasted images and static video-frame
   previews before sending.
2. **Grouped media:** group consecutive photos and videos from the same sender,
   preserving each original message's identity, actions, captions, and read
   receipts. Keep existing whole-chat `v` viewer behavior.
3. **Media navigation:** keep the main item anchored with strips of its actual
   right-hand neighbors, plus reversible vertical navigation through originals.

Each child PR includes the tests and documentation for its behavior. Review in
this order: pre-send previews, grouping, then navigation. Each child targets its
immediate parent in the feature chain; review its incremental diff rather than
re-reviewing the cumulative feature. Link the preceding and following PRs as
those PRs become available.

The tracker PR targets `main` and stays draft/no-merge until the children are
integrated and the assembled candidate is approved for `main`. Publication is
not merge authorization. Cohesive units may exceed the advisory 400-line review
guideline; do not drop tests or documentation just to reduce review size.

## Manual expectations

Check these behaviors on the assembled preview once it is available:

- Queue or paste images and videos: inspect the image or static video-frame
  preview before sending. Animated previews, autoplay, and video playback are
  not part of these expectations.
- Find consecutive same-sender photos and videos: confirm grouping retains
  original captions, message actions, and read receipts.
- Open the existing whole-chat `v` viewer: confirm its behavior is preserved.
- Navigate grouped media: confirm the main item stays anchored, right-hand
  strips show actual neighbors, and vertical navigation can return through the
  original items in reverse.
- Confirm News/update remains available with no auto-update behavior changes.

## Verification status

- Accepted candidate `bfd467a`: preview CI run `41560180891` and independent B1
  review passed. This is evidence for that candidate only.
- Newly assembled integration candidate: preview CI, PR checks, and integration
  review are pending. The accepted preview's results do not establish that the
  new candidate is green or ready to merge.
