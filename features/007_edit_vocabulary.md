---
status: review
---
# Edit Vocabulary

- Support `cargo run -- edit vocabulary <word_study>`, matching the study word case-insensitively. Reject ambiguous matches rather than editing the wrong entry.
- Prompt separately to edit `word_translation`, `part_of_speech`, and `gender`; pressing Enter defaults to no. Do not allow editing `word_study`.
- Validate values using the existing import labels; allow removing gender.
- Save changes together, updating `updated_at` in UTC. If interrupted or no values change, leave the entry unchanged.
- Preserve review events and scheduling, including existing FSRS state and due dates.
