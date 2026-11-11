---
status: review
---
## Acceptance Criteria

- Use the Rust crate `speech` from [doom-fish/speech-rs](https://github.com/doom-fish/speech-rs) for live microphone recognition in `cargo run -- study vocabulary`. Require on-device recognition and use the locale of the expected answer: German (`de-DE`) for the current vocabulary.
- Display the capitalized `word_translation`, for example `Please translate this word: Perro`, and indicate that the app is listening. Start a 20-second timer when microphone capture begins.
- The expected spoken answer is `word_study`, prefixed with `der`, `die`, or `das` when the entry has a gender; for example, `gato` expects `die Katze`.
- Show the current in-memory transcript in the terminal as updates arrive (for example, `Heard: der Hund`). Keep finalized segments and replace provisional text when the recognizer revises it. Start with an empty transcript for each entry.
- Normalize both the transcript and expected answer: lowercase, replace punctuation and symbols with spaces, and tokenize with `split_whitespace()`. Preserve accented letters and digits.
- Let `n` be the number of tokens in the normalized expected answer. Compare the transcript's `windows(n)` directly with those tokens. A match requires the same whole tokens in the same order. If the expected answer has no tokens after normalization, report an invalid vocabulary entry and leave it ungraded.
- Recognize the global English spoken command `exit` when an exact whole `exit` token appears anywhere in the recognized update. On `exit`, stop capture and end the session with the current entry ungraded. On the first answer match within the listening window, stop capture immediately, cancel the timer, and grade the answer `Good`. If the timer expires without a match, stop capture, grade the answer `Again`, show the expected German answer, and advance. Silence follows the same timeout rule.
- Save exactly one review and its FSRS schedule update in the existing transaction before advancing. Ignore late transcript updates and timer events once the entry has been graded. Keep the existing selection of up to five new or due entries.
- If permissions are denied, on-device recognition is unavailable for the locale, or capture fails before grading, report the issue and end the session with the current entry ungraded. Cancellation also releases the microphone and leaves the current entry ungraded.
- Keep audio and transcripts out of persistent storage and logs. Continue displaying prompts in the terminal.

## Verification

- Verify that `der Hund` matches variations such as `DER HUND!` and `ich glaube, der Hund`, while `Hunde` is rejected.
- Verify that `die Straße` matches `Die, Straße` and that `die lange Straße` is rejected. Verify that accents remain significant by checking that `Straße` does not match `Strasse`.
- With a fake recognizer and clock, verify that a match at second 4 stops capture and advances immediately. Verify that silence or a nonmatching transcript produces `Again` at the 20-second deadline.
- Verify that each nonempty transcript update is displayed, including a revised provisional transcript. Verify that standalone `exit` ends the session and leaves the current entry ungraded. Verify that late callbacks cannot grade an entry twice or affect the next entry, and that recognition errors leave the current entry ungraded.
- On macOS, verify that the CLI recognizes a spoken German answer with networking disabled, saves the review, and advances automatically.
