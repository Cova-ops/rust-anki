# Roadmap

## In progress

- [001 - Initialization](features/001_Initialization.md) — steps 1-4 implemented: CLI skeleton, SQLite initialization, vocabulary storage, and dynamic embedded migrations. In review.
- [002 - Vocabulary Import and Text-Based Study](features/002_logic_studying.md) — CSV vocabulary import, unique `word_study` migration, and typed-answer terminal study flow implemented. In review.
- [003 - Spaced Repetition for Vocabulary](features/003_spaced_repetition.md) — FSRS scheduling, due/new session selection, and transactional review persistence implemented. In review.
- [004 - Adding Speech Recognition](features/004_adding_speech_recogniztion.md) — on-device German live recognition, transcript normalization, a 20-second listening deadline, and global spoken `exit` implemented. In review.
- [005 - Adding Speech Synthesizer](features/005_adding_speech_synthesizer.md) — local Spanish prompt and German corrective playback implemented; overlapping recognition and playback remains in progress.
- [006 - Study Queue and Same-Session Retries](features/006_study_queue_next_command.md) — unbounded new-then-due selection, persistent retry schema groundwork, and spoken `next` implemented; retry reinsertion and retry-specific FSRS scheduling remain in progress.
- [007 - Edit Vocabulary](features/007_edit_vocabulary.md) — interactive editing of translation, part of speech, and gender without changing review scheduling. In review.
- [008 - Delete Vocabulary](features/008_delete_vocabulary.md) — confirmation with vocabulary and review details, followed by transactional deletion. In review.

## Completed

None yet.
