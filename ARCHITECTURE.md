# Architecture

## Purpose and scope

Build a local Rust CLI for studying vocabulary, especially German, through speech recognition and synthesis. The study session should support hands-free interaction. Use SQLite for local persistence and clap for CLI parsing. Keep macOS-specific speech integration isolated.

This document defines the target architecture and mandatory rules for future changes. It is not an inventory of implemented features. Preserve existing behavior, schema, CLI commands, and selected speech libraries unless the task explicitly changes them. Do not implement planned features merely because they appear as examples here.

## Mandatory code organization rules

1. Every module owns one coherent responsibility. Separate orchestration, business rules, persistence, CLI presentation, and platform integration.
2. Before editing, inspect the affected modules and identify the owner of each new responsibility. Do not default to adding code to the caller or the largest existing file.
3. Extend an existing module only when the change belongs to its responsibility. Otherwise extract a named submodule.
4. Keep dependencies acyclic. Business rules must not depend on clap, SQLite, macOS APIs, or terminal output.
5. Refactor the affected responsibility before adding features to an oversized file. Avoid unrelated repository-wide rewrites.
6. Do not satisfy size limits by compressing formatting, deleting useful documentation, using include!(), or splitting one responsibility into arbitrarily numbered files.
7. Prefer direct functions and concrete types. Introduce traits only for a real boundary, alternative implementation, or useful test seam.
8. Do not add dependencies or a new async runtime solely to reorganize code.

## File and function size policy

Count physical lines after formatting. Review production code and test code separately; tests do not excuse oversized production modules.

| Item                            | Target                                 | Required action                                                                  |
| ------------------------------- | -------------------------------------- | -------------------------------------------------------------------------------- |
| Ordinary production file        | Usually 100–250 lines; smaller is fine | Keep one responsibility                                                          |
| Production file over 300 lines  | Review threshold                       | Inspect cohesion and extract independent responsibilities                        |
| Production file over 400 lines  | Refactor threshold                     | Split the affected responsibility before adding functionality                    |
| Existing file around 500+ lines | Refactoring priority when touched      | Reduce the affected file below 400 lines, or document a concrete exception       |
| main.rs                         | Prefer at most 80 lines                | Move CLI dispatch and workflow logic elsewhere                                   |
| mod.rs                          | Prefer at most 60 lines                | Module declarations, selective exports, and minimal boundary documentation       |
| Function                        | Prefer at most 40–50 lines             | Review distinct phases, nesting, and side effects; extract meaningful operations |

These are project guardrails, not proof of good design. A short file with mixed responsibilities still needs refactoring. Never create empty modules or trivial forwarding layers just to follow the table.

Exceptions are allowed for cohesive code such as platform bindings, declarative tables, or generated code. Before accepting an exception, explain why splitting harms clarity and record the path, reason, and revisit condition in this document. “Already large” and “faster to append” are not valid reasons. Routine splits and justified exceptions do not require user confirmation.

For large inline test suites, use a sibling tests.rs module or integration tests when appropriate. Do not move tests solely to hide oversized production code.

## Responsibilities and dependency direction

| Area    | Owns                                                                       | Must not own                                        |
| ------- | -------------------------------------------------------------------------- | --------------------------------------------------- |
| main    | Process entry, initialization, composition, exit status                    | SQL, session loop, matching rules, speech callbacks |
| cli     | clap definitions, argument validation, terminal presentation               | Study policy, database queries, speech internals    |
| app     | Use-case coordination and wiring of services                               | SQL details, matching algorithms, native bindings   |
| domain  | Vocabulary and review concepts, pure validation and scoring rules          | I/O, platform APIs, clap, database row types        |
| study   | Session state, transitions, answer handling, implemented scheduling policy | SQLite queries, native speech setup, CLI parsing    |
| storage | Connections, migrations, queries, row mapping, transactions                | Session policy, speech behavior, terminal output    |
| speech  | Recognition, synthesis, cancellation, platform resource lifecycle          | Answer correctness, review scoring, SQL             |

Allowed dependency direction:

- main composes cli, app, storage, and speech.
- app coordinates study and the storage/speech adapters.
- study depends on domain and narrowly scoped interfaces when required.
- storage and speech may depend on domain or the interfaces they implement.
- domain does not depend on any application or adapter module.
- Adapters never import app or main.

When study needs external effects, either return events/actions for app to execute or use a small interface owned by the consuming layer. Choose the simplest approach that fits the current implementation; do not build both frameworks.

## Illustrative structure

Create only files with an actual responsibility and implementation. Existing equivalent names can remain. A single file can become a directory module when it grows.

```text
src/
  main.rs
  app.rs
  cli/
    mod.rs
    args.rs
    output.rs
  domain/
    mod.rs
    word.rs
    review.rs
  study/
    mod.rs
    session.rs
    state.rs
    answer.rs
  storage/
    mod.rs
    connection.rs
    migrations.rs
    words.rs
    reviews.rs
  speech/
    mod.rs
    recognition.rs
    synthesis.rs
tests/
```

This tree is a starting point, not the final depth. If recognition.rs, session.rs, or words.rs becomes too large, split inside that feature instead of adding unrelated root-level files.

Example extraction for a large recognition module, only when these responsibilities exist:

```text
speech/
  recognition/
    mod.rs          # Small public API and exports
    recognizer.rs   # Recognition lifecycle
    macos.rs        # Native API integration
    events.rs       # Recognition events
    transcript.rs   # Transcript accumulation/reset
    tests.rs
```

When converting foo.rs to foo/mod.rs, remove the old module file and update declarations. Do not leave conflicting module roots.

## How to split a large file

1. List the responsibilities actually present: state, algorithms, I/O, platform setup, formatting, lifecycle, or mapping.
2. Group code by shared invariants and reason to change. Move related types and their behavior together.
3. Extract the most independent responsibility first, preferably a pure operation or adapter.
4. Give the extracted module a domain-specific name. Avoid utils.rs, helpers.rs, common.rs, manager.rs, or part1.rs as miscellaneous containers.
5. Keep a small public interface. Use private items by default, then pub(super) or pub(crate) only where needed.
6. Keep internal fields private. Expose meaningful operations rather than widening visibility just to make the split compile.
7. Update imports and callers, remove duplicated or obsolete code, and verify behavior.
8. Reassess the remaining file. Moving everything into a new 500-line file is not a completed refactor.

Do not scatter a single type across files through unrelated impl blocks merely to lower line counts. Extract an actual collaborator or responsibility.

## Study-session boundaries

- Session coordination owns the sequence of asking, listening, evaluating, recording, and advancing.
- Answer normalization and comparison are pure operations, independent of audio and persistence.
- Review scoring and scheduling, when implemented, are separate from speech recognition and transcript handling.
- Represent meaningful session transitions explicitly when multiple booleans would permit invalid states.
- Transcript fragments, timeouts, skips, cancellation, and late callbacks must be handled according to existing behavior.
- Keep one owner for session state. Speech callbacks emit data or events rather than mutating study state and writing to SQLite.

Do not turn session.rs or app.rs into a new catch-all module. Extract use cases or transition logic as their responsibilities grow.

## Speech and concurrency

- Recognition and synthesis have separate lifecycles and APIs.
- Keep native macOS objects and their thread-affinity requirements inside the adapter.
- Make start, stop, cancellation, callback cleanup, and resource release explicit.
- Preserve the current concurrency model unless a behavior change requires otherwise.
- Prefer ownership and messages across threads; introduce shared mutable state only when necessary.
- Do not hold locks while invoking speech APIs, doing database I/O, or running callbacks.
- Ensure callbacks from an old question/session cannot accidentally answer the next one; use the existing mechanism or a scoped session identifier when required.
- Do not assume platform objects are Send or Sync; never add unsafe implementations merely to simplify a refactor.

## Persistence

- SQL and row conversion remain in storage.
- Keep vocabulary queries, review queries, connection setup, and migration execution separate when each warrants its own module.
- Expose application-meaningful operations rather than leaking database rows or connections into study rules.
- Define transaction boundaries for operations that must succeed together.
- Preserve schema compatibility and migration history during structural refactoring.
- Add new migrations for schema changes; do not rewrite previously applied migrations.
- Avoid generic repository frameworks unless actual reuse justifies them.

## Errors and public APIs

- Return errors from the layer that can explain them; add context at boundaries.
- Present user-facing errors and select exit status at the application/CLI boundary.
- Avoid unwrap and expect in recoverable runtime paths.
- Keep module APIs narrow; do not re-export every internal item.
- Reuse the existing error strategy unless it prevents clear boundaries.

## Required agent workflow

Before implementation:

1. Read this document and applicable repository instructions.
2. Inspect existing code, tests, module boundaries, and file sizes in the affected area.
3. Identify which modules own the requested behavior and whether an extraction is needed.
4. Proceed with routine architecture decisions without asking for confirmation.

During implementation:

1. Separate structural moves from behavior changes where practical.
2. Refactor oversized affected responsibilities before appending functionality.
3. Preserve interfaces and behavior unless the request requires a change.
4. Keep the scope local; do not rewrite unrelated large files.
5. Update this document only when a real architecture decision changes, not to justify an avoidable violation.

Before completion:

- Run cargo fmt --check, cargo check, and the relevant existing tests.
- Run the repository's configured Clippy/check commands; absent a policy, use cargo clippy --all-targets -- -D warnings.
- Add targeted regression tests when moving stateful logic or changing behavior creates meaningful risk.
- For session/speech refactors, cover relevant cancellation, stale-event, and state-transition behavior using existing test seams where feasible.
- Run a macOS smoke test for native speech behavior when the environment supports it; explicitly report if it could not be performed.
- Review the diff for accidental behavior changes, duplicate logic, broad visibility, cyclic dependencies, and oversized files.
- Report what was split, what behavior was verified, and any documented size exceptions or pre-existing check failures.

Do not claim native integration was tested if only compilation or mocked tests ran. Do not add tests that merely mirror file layout.

## Initial refactoring approach for existing large files

Apply this policy incrementally to the modules involved in the current task. A dedicated cleanup task can address the remaining files one at a time.

For each approximately 500-line file:

1. Separate production logic from test volume during assessment.
2. Identify two or more genuinely independent responsibilities, if present.
3. Extract cohesive submodules and retain a small stable entry point where useful.
4. Verify after each meaningful extraction.
5. Stop once responsibilities and size satisfy this policy; do not keep fragmenting cohesive code.

## Architecture exceptions

None recorded. Add justified exceptions here with the exact file path, rationale, and revisit condition.
