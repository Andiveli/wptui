# Enriched Markdown review chain

This draft integration branch tracks [issue #529](https://github.com/Andiveli/wptui/issues/529).
It is not a release candidate. Review the dependent slices before integrating them;
merging this tracker into the default branch is a separate owner decision.

## Review order

1. **Messages** (`preview/enriched-markdown-messages`): display-only WhatsApp and
   extended Markdown, source-range remapping, message/caption heights and URL hit cells.
2. **Composer** (`preview/enriched-markdown-composer`): live formatting, active
   delimiter reveal, source-backed cursor movement, Unicode/RTL and wrapping.
3. **Corrections** (`preview/enriched-markdown-corrections`): preserve original
   URL destinations, closing-fence whitespace geometry and combining marks.

Each child targets the preceding branch; the first targets `feature/enriched-markdown`.
Keep source text unchanged when sending, copying or searching. Unsupported syntax
stays literal; this is a terminal renderer, not a browser or full CommonMark engine.

## Verification

Rust verification uses the repository's remote preview workflow. Never infer
that an earlier slice's successful CI covers a later head. Check each PR's exact
head and dependencies, and retain the regression tests with the code they verify.
Automated terminal-cell checks do not replace a live visual review.

The initial tracker contains only this review guide so it has a safe, nonempty
diff. It intentionally does not integrate the implementation slices yet.
