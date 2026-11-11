---
status: review
---
# Delete Vocabulary

- Support `cargo run -- delete vocabulary <word_study>` with the same case-insensitive lookup and ambiguity protection as editing.
- Show all vocabulary fields and review statistics, including grade counts and the current schedule, in a table before requesting confirmation. Display timestamps in a human-readable local format without changing their stored UTC values.
- Only an explicit yes confirms deletion; no, Enter, or end-of-input leaves all data unchanged.
- Atomically delete the selected vocabulary entry and its associated review events and schedule. Preserve unrelated entries and reviews.
