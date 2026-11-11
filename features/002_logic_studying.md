---
status: review
---
# Vocabulary Import and Text-Based Study

## 1. Import vocabulary from a CSV file

### Acceptance criteria

- Add the `csv` crate and use it to read the file.
- Support `cargo run -- upload vocabulary <csv_path>`.
- Require a UTF-8 CSV with the following header in this order. Each record must contain four fields. Leave `gender` empty when it does not apply and store it as SQL `NULL`.
  ```
  word_study,word_translation,part_of_speech,gender
  Hund,perro,noun,masculine
  Katze,gato,noun,feminine
  laufen,correr,verb,
  lesen,leer,verb,
  ```
- Reject missing or unreadable files, incorrect headers, malformed records, blank words or translations, and invalid `part_of_speech` or `gender` values. Include the file path and, when available, the record or line number in the error.
- Reject duplicate `word_study` values within the CSV or already present in `vocabulary_entries`, comparing case-insensitively using Unicode lowercase after trimming CSV fields. Report the conflicting word and explain that the entire import was rejected. Recheck inside the write transaction to prevent concurrent uploads from introducing duplicates.
- Add a new migration that creates a unique index on `vocabulary_entries.word_study`. If an existing database contains duplicates, fail the migration with a diagnostic so the user can resolve them.
- Validate the CSV before writing. Use the existing bulk insert operation to save the vocabulary in a single transaction. If any insert fails, roll back the entire import. Treat a uniqueness violation during insertion as a duplicate error even if the earlier validation passed.
- On success, report how many entries were imported.

### Tests

- Use `test_vocabulary_002.csv` to verify a successful import, including an empty `gender` field stored as `NULL`.
- Verify that an invalid row, a duplicate within the file, and a duplicate already in the database each leave the database unchanged.

## 2. Study vocabulary through the terminal

This iteration validates the study flow with typed answers. Speech recognition and synthesis will be integrated in a later feature.

### Acceptance criteria

- Support `cargo run -- study vocabulary`.
- For this iteration, load the entries with IDs `1` through `5` and present the available entries in ascending ID order. Skip missing IDs. If none of the five entries exist, tell the user to import vocabulary and end the session.
- Display `word_translation` with its first character capitalized and ask the user for its translation.
    ```
    Please translate this word: {{word_translation}}
    >
    ```
- When the answer has a gender, the expected answer should include the German definite article: `der` for masculine, `die` for feminine, and `das` for neuter. For example, `gato` the user input should be: `die Katze`. Entries without a gender expect `word_study` alone.
- Trim the user's input and compare it with the expected answer after converting both to lowercase. Preserve accents and punctuation in the comparison.
- If the answer is correct, advance to the next entry. If it is incorrect, tell the user and prompt for the same entry again.
- Recognize `exit` after trimming and lowercasing the input, before grading the answer. End the session when the user enters `exit`, the input stream closes, or all loaded entries have been answered correctly.
- Print `That's all for today! :)` when an active session ends normally.

### Implementation note

Keep the study loop and answer comparison separate from CLI argument parsing and database access. Introduce a shared trait when a second exercise type shows which behavior needs to be reused.
### Tests

- Verify that an incorrect answer keeps the current entry and that a correct answer advances to the next one.
- Verify that leading and trailing whitespace and differences in case are accepted.
- Verify that the displayed translation capitalizes only its first character and preserves accents.
- Verify that gendered entries require the article and accept it in any case.

Feature 004 supersedes the typed-answer loop in this section: `study vocabulary` now displays the capitalized translation and listens for the spoken German study word, including its article when applicable. The import requirements in section 1 still apply.
