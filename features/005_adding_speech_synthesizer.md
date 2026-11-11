## Acceptance Criteria

- Use the `avspeechsynthesizer` crate from [doom-fish/avspeechsynthesizer-rs](https://github.com/doom-fish/avspeechsynthesizer-rs) for local text-to-speech on macOS. Keep using the `speech` crate from [doom-fish/speech-rs](https://github.com/doom-fish/speech-rs) for on-device microphone recognition.
- For each card, display and speak the Spanish prompt once using a Spanish voice. The expected answer is in German. Use `der`, `die`, or `das` before the German word when the entry has grammatical gender. Otherwise, expect the German word or phrase by itself. For example, say `perro` and expect `der Hund`. The session has no replay command.
- Start listening before the Spanish prompt begins. Recognize German answers and support `exit` as a standalone spoken command during playback and while waiting for an answer. Verify that `exit` works with the chosen recognition setup.
- Accept a correct answer during playback. If the prompt finishes before the user answers, give the user 20 more seconds. Start that timer when playback finishes.
- Keep the existing normalization and matching rules. Lowercase the recognized text and expected answer, treat punctuation and symbols as token separators, and match whole, contiguous tokens. For a card expecting `der Hund`, `ich glaube, der Hund` matches and `Hund` alone does not.
- On a correct answer, stop any active playback and microphone capture immediately, cancel the timer, save `Good` and the FSRS schedule update, then advance. On `exit`, stop playback and capture immediately, cancel the timer, and end the session with the current unanswered card ungraded.
- If the timer expires without a match, save `Again` and the FSRS schedule update. Display and say the correct answer once using a German voice, including the article when required. Wait for this feedback to finish before starting the next prompt. During feedback, `exit` can stop playback and end the session. The saved review remains in place.
- Prevent the app's own speech from triggering a match or `exit`. Keep grading disabled during corrective feedback, and clear the transcript before the next card so that feedback cannot count as its answer.
- Save each grade and schedule update in the existing transaction. Ignore late recognition, playback, and timer events from previous cards. If audio fails before grading, report the problem, release audio resources, and leave the current card ungraded. Keep audio and transcripts out of persistent storage and logs.

## Verification

- With the Mac speakers on, let the prompt play without speaking and verify that the app neither grades the answer as `Good` nor exits. Then answer over the prompt and verify that playback stops immediately and exactly one `Good` review is saved. Repeat with headphones.
- Say `exit` during the Spanish prompt and verify that playback stops and the current card stays ungraded. Say it during German feedback and verify that playback stops and the saved `Again` review remains.
- Verify that the 20-second timer starts when the Spanish prompt finishes. On timeout, verify that the app speaks `der Hund` for an entry with gender and `laufen` for an entry without gender, then advances after feedback ends.
- Run a complete session on macOS with networking disabled to verify local synthesis and recognition.

After implementing and testing this feature, update the audio architecture and session state machine in `ARCHITECTURE.md`. Replace the previous rule that playback and microphone capture alternate. Describe how the implementation handles overlapping playback and recognition, interruptions, and corrective feedback. Keep unverified behavior marked as TBD.
