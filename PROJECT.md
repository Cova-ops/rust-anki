# Project Overview

## Why?

I am currently learning German, and as with any new language, building a strong vocabulary requires consistent practice.

One of the most popular applications for vocabulary learning is [Anki](https://apps.ankiweb.net/). Although Anki is useful, its traditional study workflow requires users to interact with their computer or phone to indicate whether they remembered a word.

This is the main problem we want to solve.

The goal is to develop a **CLI application for vocabulary learning**, initially focused on German, that allows users to complete study sessions without interacting with a keyboard or mouse.

The application should provide a hands-free experience through speech recognition and speech synthesis.

## Core Concept

The application should support a study workflow where:

1. The application displays and speaks a Spanish translation using speech synthesis.
2. The user responds verbally with the corresponding German word, including its article when applicable.
3. Speech recognition processes the user's response.
4. The application evaluates whether the answer is correct.
5. The session automatically proceeds to the next word. The global spoken commands `next` and `exit` are recognized when their exact whole token appears in a recognition update.

The long-term goal is to make vocabulary practice possible while performing other activities, without requiring continuous interaction with a computer or phone.

## With What?

The application will primarily use Rust and native macOS capabilities.

| Component            | Technology                                           |
| -------------------- | ---------------------------------------------------- |
| Programming language | Rust (latest compatible stable version)              |
| Speech recognition   | `speech` (on-device Apple Speech)                    |
| Speech synthesis     | `avspeechsynthesizer-rs` (local AVSpeechSynthesizer) |
| Database             | SQLite                                               |
| CLI argument parsing | `clap`                                               |
| Primary platform     | macOS Golden Gate 27.2                               |

Speech processing is local on macOS. The current study flow plays each Spanish prompt, recognizes the German response, and plays the German correction after an incorrect or silent answer.

The application will run locally without requiring a remote backend.

## Scope

Development will be incremental, starting with a minimum viable product (MVP) and introducing additional capabilities through subsequent iterations.

The initial focus is vocabulary learning, particularly German vocabulary.

The architecture should allow future improvements without introducing unnecessary complexity during the MVP phase.

### Initial Goals

- Provide a CLI interface for managing and starting study sessions.
- Store vocabulary and study progress locally.
- Present Spanish translations through local speech synthesis.
- Recognize spoken German answers on-device.
- Speak the expected German answer as corrective feedback after an incorrect or silent response.
- Evaluate responses, show the next review date in local calendar format, and advance through study sessions.
- Support spoken `next` to grade the current card `Again` and spoken `exit` to leave its current card ungraded.
- Minimize or eliminate keyboard interaction during active study sessions.
- Accept new word for studying in CSV format.

### Future Possibilities

Potential future improvements include:

- Support for additional languages.
- Verb conjugation exercises.
- Vocabulary categories.
- More advanced spaced repetition strategies.
- Configurable study sessions.

These are potential directions, not committed MVP requirements.

## Constraints

- Study sessions must be hands-free.
- The application must operate locally.
- SQLite will be used for persistent storage.
- macOS is the initial target platform.
- Prefer native macOS capabilities when appropriate.
- Avoid unnecessary dependencies and architectural complexity.

## Out of Scope (Initial MVP)

The following are not priorities for the initial version:

- Graphical user interface.
- Cloud synchronization.
- Mobile applications.
- User accounts or authentication.
- Multi-user support.
- Remote backend services.
- Export and/or import vocabularies
- Accept new words in differents format, not only CSV.

## Development Approach

The project will follow an incremental development process.

Individual features, implementation requirements, and acceptance criteria will be documented separately.
