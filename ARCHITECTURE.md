# Architecture

This document describes the proposed technical design for the MVP in [PROJECT.md](PROJECT.md). Confirmed constraints come from that document. Proposed designs and decisions marked **TBD** are open for review.

## Architectural decisions

| ID  | Status    | Decision and rationale                                                                                                                                                                                        |
| --- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A01 | Confirmed | Use Rust on macOS with `clap` for CLI argument parsing. Use a compatible stable Rust toolchain.                                                                                                               |
| A02 | Confirmed | Use local SQLite for vocabulary and study progress. The application runs locally without a remote backend.                                                                                                    |
| A03 | Confirmed | Focus on German vocabulary and make active study hands-free. Setup, vocabulary management, and starting a session can use the CLI.                                                                            |
| A04 | Proposed  | Use one executable with Rust modules. Keep session rules separate from macOS audio and SQLite so they can be tested without either.                                                                           |
| A05 | Proposed  | Alternate playback and listening to keep the recognizer from hearing the app's own prompts.                                                                                                                   |
| A06 | Proposed  | Save each graded answer before advancing. A session interrupted later can still retain its completed reviews.                                                                                                 |
| A07 | TBD       | Speech recognition and synthesis providers, their Rust integration, minimum macOS version, and distribution method. Evaluate native macOS capabilities against the local-operation and language requirements. |
| A08 | TBD       | SQLite crate, schema and migration mechanism, database location, and supported access from simultaneous CLI invocations.                                                                                      |
| A09 | TBD       | Study direction, answer language, accepted variants, normalization, grading, card selection, and retry or skip policies.                                                                                      |

A08 resolution: `rusqlite` is used with one connection opened at startup. Migrations are versioned, embedded from `migrations/` in order, and recorded in `schema_migrations`; an existing database is never recreated. The database file is `rust_anki.db` in the working directory (project root when run via `cargo run`). Access from simultaneous CLI invocations remains TBD.

A09 resolution for this iteration: the study flow uses live, on-device speech recognition. The terminal displays the capitalized Spanish `word_translation` and listens for the German `word_study`, prefixed with `der`/`die`/`das` when the entry has a gender. Transcripts and the expected answer are lowercased, punctuation and symbols become spaces, and matching requires the expected whole tokens in order. A match within 20 seconds is graded `Good`; silence or no match at the deadline is graded `Again` and shows the expected German answer. The official `fsrs` crate (6.6.2) computes the next due time with its default parameters and a fixed desired retention of 0.95. A session selects up to five due entries (oldest due first) and then never-reviewed entries; future-scheduled entries are excluded and each entry appears at most once.

## Proposed modules

| Module    | Responsibility                                                                                                                                                                      |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `main`    | Construct adapters and connect them to the CLI and session runner.                                                                                                                  |
| `cli`     | Parse commands with `clap`, handle setup and vocabulary management, start study sessions, and map failures to diagnostics and exit codes. Command names and import formats are TBD. |
| `study`   | Own session state, card selection, and answer evaluation. Define the interfaces the runner needs from storage and speech.                                                           |
| `speech`  | Implement recognition and synthesis adapters. Own platform permissions, audio resource lifetime, and translation of platform callbacks into typed results.                          |
| `storage` | Implement vocabulary reads and review writes against SQLite. Own schema initialization, migrations, and transactions.                                                               |

The session runner depends on the speech and storage interfaces. Platform types, SQL rows, and CLI arguments stay at their respective boundaries.

Keep a module in a single file while it has one clear responsibility. When a feature adds a distinct responsibility that would be easier to understand or test separately, split the module into focused submodules. Use responsibilities as the deciding factor and file length as a signal. Keep existing public import paths working and avoid unrelated restructuring during small fixes.

Feature 003 adds `review` for grade and FSRS scheduling types, and `storage::reviews` for due/new selection plus transactional review persistence. `study` remains responsible for prompting, answer comparison, and session advancement through a grading callback; it performs no database access.

Feature 004 adds `speech` with a live microphone recognizer behind a `Recognizer` trait and a `Clock` abstraction for the 20-second deadline. `study` prompts, normalizes transcripts, decides the grade, and advances through a grading callback; macOS Speech framework types stay inside `speech`.
## External crates

| Crate | Responsibility |
| --- | --- |
| `clap` | CLI parsing. |
| `rusqlite` | Local SQLite access. |
| `eyre` | Application error propagation. |
| `chrono` | UTC timestamps. |
| `csv` | Vocabulary CSV import. |
| `speech` | On-device Apple Speech recognition. |
| `avspeechsynthesizer-rs` | Local AVSpeechSynthesizer text-to-speech and voice selection. |

No async runtime is required. The synthesizer pumps the main-thread macOS run loop while it waits for an utterance to finish.

## DB Creations

We are going to handle the `migrations` directory for all the new implementations or changes that will be implemented in the `DB`. The reason for that, is that in the meantime that the platform is being under develop, other people are using this for studying, so we cannot never re-create from zero the `DB`. 

Feature 002 resolution: `migrations/02_unique_word_study.sql` creates a unique index on `vocabulary_entries.word_study`. If an existing database already contains duplicates, the migration fails so the user can resolve the conflict instead of the database being recreated. CSV imports validate the full file and check for duplicate `word_study` values (case-sensitive after trimming) before calling the existing bulk insert in one transaction; any validation or insert failure leaves the database unchanged. Empty `gender` fields are stored as SQL `NULL`, and CSV labels `masculine`/`feminine`/`neuter` are mapped to the stored `Male`/`Female`/`Neutral` values.

Study direction for feature 004: the terminal displays the capitalized Spanish `word_translation`. The expected answer is the German `word_study`, prefixed with `der`/`die`/`das` when the entry has a gender, recognized from live speech and matched after lowercasing and replacing punctuation and symbols with spaces. The previous typed-answer direction is no longer used by `study vocabulary`.

Feature 003 resolution: `migrations/03_review_schedule.sql` adds `review_events` (one row per graded answer with vocabulary entry, UTC review time, and `again`/`good` grade) and `review_schedule` (one row per vocabulary entry with UTC due time, FSRS stability/difficulty, and last review time). Each graded answer inserts its event and upserts the schedule in one transaction, so a failed schedule write also rolls back the event.

## Speech interfaces

The implemented adapters isolate macOS framework types behind small traits.

| Operation | Input | Result |
| --- | --- | --- |
| Recognize one answer | German locale, 20-second deadline | Match, timeout, global `exit`, or typed failure |
| Synthesize and wait | Text and a voice locale | Playback completed or typed failure |
| Stop synthesis | None | Best-effort immediate stop |

`Recognizer` owns microphone capture. `Synthesizer` owns local playback and supports `speak_and_wait` plus `stop`. `MacSynthesizer` pumps the macOS run loop required by AVSpeechSynthesizer events. Recognition and playback are sequential today; overlapping playback and capture is TBD.

## Session state machine

The states below describe a proposed turn lifecycle. The study workflow speaks a word, listens for its translation, grades the answer, and advances automatically.

| State            | Event                                 | Transition                                                                                                                |
| ---------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Prepare          | Vocabulary available and speech ready | Select a card.                                                                                                            |
| Select card      | Card available                        | Speak the prompt.                                                                                                         |
| Select card      | No cards remain                       | Finish. An empty initial selection ends with guidance on adding vocabulary.                                               |
| Speak prompt     | Playback completed                    | Listen for one answer.                                                                                                    |
| Listen           | Final transcript received             | Evaluate the answer.                                                                                                      |
| Listen           | No speech or timeout                  | Apply a bounded recovery policy that retries, skips, or ends the session without keyboard input. Exact policy is **TBD**. |
| Evaluate         | Grade produced                        | Save the review.                                                                                                          |
| Save review      | Commit succeeded                      | Give spoken feedback if enabled, then select the next card. Feedback content and defaults are **TBD**.                    |
| Any active state | Cancellation or unrecoverable failure | Stop capture and playback, preserve committed reviews, and finish with the appropriate status.                            |

A transcript becomes a candidate answer only for its current card. The evaluator uses the card's accepted answers and the grading policy. A timeout or adapter failure does not produce an incorrect grade. How a user ends a session early hands-free and whether spoken control commands share the answer vocabulary are **TBD**. The normal study path and recoverable failures require no keyboard or mouse after launch.

## Data flow and SQLite persistence

1. The CLI validates the session request and opens the local database.
2. The runner reads a card with a stable ID, prompt, expected translations, and language information. The selection policy is **TBD**.
3. The synthesis adapter plays the prompt. The recognition adapter returns a final transcript or a typed outcome.
4. The evaluator compares a transient transcript with the expected translations. The normalization rules and handling of German articles, and equivalent translations are **TBD**.
5. The storage adapter commits the review. The runner then advances or ends the session.

The proposed data model has **vocabulary entries** and **review events**. A review event records a unique attempt ID, vocabulary entry ID, timestamp, and grade. Whether to add a derived progress table depends on the selection policy. The concrete schema, timestamp format, and resume behavior are **TBD**.

Use versioned migrations and parameterized SQL. Write the review and any associated progress update in one transaction. Keep transactions short and outside audio operations. Enforce uniqueness of the attempt ID so a write retry can reuse it without creating a duplicate review. If the process exits before a commit, the attempt may be lost. Earlier commits remain available.

## Audio architecture

Speech recognition and local text-to-speech are implemented. Playback currently completes before microphone capture begins; overlapping playback and recognition remains planned.

| Decision | Status |
| --- | --- |
| Use `speech` for on-device German microphone recognition on macOS. | Implemented |
| Use `avspeechsynthesizer-rs` for local macOS playback. | Implemented |
| Speak every Spanish prompt and German corrective answer with a locale-selected voice. | Implemented |
| Allow recognition during prompt playback and support immediate playback interruption. | Planned |

- **Recognition adapter.** `speech::MacRecognizer` owns on-device `de-DE` capture, transcript updates, and microphone cleanup. It accepts global English `exit` and `next` commands when their exact whole token appears anywhere in a recognition update.
- **Synthesis adapter.** `synthesis::MacSynthesizer` wraps `avspeechsynthesizer-rs`, selects an installed `es-MX` or `de-DE` voice, runs the owning thread's run loop until playback finishes, and can stop immediately. Spanish voices read prompts; German voices read corrective feedback.
- **Study session.** `study` builds the expected German answer once, including the article when required. It plays the Spanish `word_translation`, then opens recognition for the German answer. On timeout it saves `Again`, displays the answer, plays that German answer, then advances.
- **Storage.** Saves the grade and FSRS schedule update in one SQLite transaction. Audio and transcripts remain in memory.

## Study session state machine

| State | Behavior and transitions |
| --- | --- |
| Prompting | Display and play the Spanish prompt once using an `es-MX` voice. Playback completes before capture starts in the current implementation. |
| Awaiting answer | Start `de-DE` recognition and the 20-second timer. A German match saves `Good`. A transcript containing an exact whole `exit` token ends the session with the current card ungraded. |
| Corrective feedback | On timeout, save `Again`, display and play the expected German answer with a `de-DE` voice, then advance after playback finishes. |
| Finished | Stop capture and playback. An audio error before grading leaves the current card ungraded; a saved review remains if corrective playback later fails. |

The planned overlapping playback/recognition state machine, including answer interruption during prompt playback and `exit` during corrective feedback, remains TBD.
## Audio isolation and verification

The audio path must prevent the app's own playback from triggering a grade or `salir`. The isolation technique is TBD. Verify the behavior on macOS with built-in speakers and headphones. Test both playback without user speech and a learner interrupting an active prompt. Run the complete flow with networking disabled.

Use fake audio adapters and a fixed clock to test state transitions, including early matches, timeout after prompt completion, cancellation during feedback, and late callbacks.
## Error handling

| Condition                                                   | Proposed behavior                                                                                                                                  |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Invalid CLI input or empty vocabulary                       | Give a clear diagnostic and end before the study loop.                                                                                             |
| Permission denied, audio unavailable, or locale unsupported | Explain the setup issue and end if the required speech capability is unavailable.                                                                  |
| Silence, timeout, or recoverable recognition failure        | Follow the bounded recovery policy. Keep the card ungraded unless a usable transcript is evaluated.                                                |
| SQLite busy or write failed                                 | Apply a bounded retry for transient errors using the same attempt ID. If the write still fails, end with a diagnostic. Retry settings are **TBD**. |
| Invalid database or failed migration                        | Report the failure and end without recreating the user's database.                                                                                 |
| Cancellation                                                | Release microphone and playback resources and report committed results.                                                                            |

Use typed errors across adapters and translate them into user-facing messages at the boundary. Speak recovery prompts when synthesis is available and use terminal diagnostics when it fails.

## Privacy

Vocabulary and progress stay in local SQLite storage. The proposed default is to capture audio only while awaiting an answer, process transcripts in memory, and keep raw audio, transcripts, and vocabulary text out of logs. Provider selection must verify that both speech operations work locally for the required languages and that they cannot silently fall back to a network service. The platform permission flow and any opt-in diagnostics are **TBD**.

## Testing

- **Unit tests** cover legal session transitions and the chosen grading rules, including German text cases once the policy is defined.
- **Session tests with fake adapters** cover a complete hands-free run, silence and timeout recovery, cancellation, late callbacks, and stopping after a failed review write. Verify that listening starts after playback and the next card starts after a successful commit.
- **SQLite integration tests** use a temporary database to verify initialization, migrations, persistence after reopening, transaction rollback, and duplicate attempt handling.
- **macOS smoke tests** exercise real playback and microphone input, permission denial, locale availability, and a complete keyboard-free session after launch. Repeat the session with networking disabled to verify local operation for the selected speech providers.

## Considerations
- All time format that will be use, should use ISO 8601 with in UT.
