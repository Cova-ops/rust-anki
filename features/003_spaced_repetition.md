---
status: review
---
# Spaced Repetition for Vocabulary

## Acceptance Criteria

- `cargo run -- study vocabulary` loads up to five entries at the start of a session. It selects reviews due at or before the current time first, starting with the oldest due date, then fills the remaining slots with entries that have never been reviewed. Entries scheduled for the future are excluded.
- The command reuses the existing answer comparison. A correct answer is graded `Good`. An incorrect answer is graded `Again`, and the command shows the expected translation. After either grade is saved, the session advances to the next entry. Each entry appears at most once per session.
- An existing Rust implementation of FSRS calculates the next review time from the grade and the entry's prior scheduling state, when available. Use the implementation's default parameters for this iteration.
- A new SQLite migration creates `review_events` and `review_schedule`. Each graded answer adds one event with the vocabulary entry ID, review time, and grade. The schedule stores one row per vocabulary entry with its next due time and the state required by FSRS. Store timestamps in UTC.
- The review event and schedule update are committed in the same transaction. If either write fails, neither change is saved, and the command reports the error and ends the session.
- Typing `exit` or closing input ends the session without grading the current entry. Reviews already saved remain available in the next invocation.
- If no entries are new or due, the command tells the user that there is nothing to review and exits without prompting for an answer.

## Verification

- Use a fixed clock to verify that an entry is excluded before its due time and becomes eligible at that time.
- Verify that due entries take priority over new entries and that a session presents at most five.
- Verify that both grades are persisted, that an incorrect answer advances the session, and that the next invocation uses the saved schedule.
- Simulate a failed schedule write and verify that no review event was committed. Verify that `exit` and closed input leave the current entry ungraded.
