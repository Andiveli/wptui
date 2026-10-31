# Issue #358: conversation search PR chain

Tracker: `feat/conversation-search` (draft; do not merge until child PRs and required checks pass).
Issue: #358 (close only when this tracker merges into `main`).
Base: verified `main` e86b3977ff69ca8d9806a9cb7b56401bc4883f02.

1. `feat/conversation-search-01-navigation` → tracker: chat-local input, navigation, result feedback, docs and public tests.
2. `feat/conversation-search-02-highlighting` → child #1: wrapped/RTL highlight and unchanged quote/status styles with public tests.

The full integrated source was verified at 36d7d85e2d6d2094439292b87a2c5cfdf6054850 by Actions 41215453830; independently reviewed as mvwkx8ci-6-9kql. Each child still needs its own PR CI. Do not use local Cargo or bypass required checks. No PR merges directly into `main` except this tracker.
