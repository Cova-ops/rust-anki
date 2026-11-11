use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use eyre::{Context, Result, eyre};
use rusqlite::Connection;

use crate::storage::vocabulary::{self, Gender, NewVocabularyEntry, PartOfSpeech};

const HEADER: [&str; 4] = ["word_study", "word_translation", "part_of_speech", "gender"];

/// Imports vocabulary from a UTF-8 CSV file.
pub fn import_vocabulary(connection: &mut Connection, path: &Path) -> Result<usize> {
    let file =
        File::open(path).wrap_err_with(|| format!("failed to open CSV file {}", path.display()))?;

    import_reader(connection, BufReader::new(file), path)
}

fn import_reader<R: Read>(connection: &mut Connection, reader: R, source: &Path) -> Result<usize> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .trim(csv::Trim::All)
        .from_reader(reader);

    let mut records = reader.records();

    let header = records
        .next()
        .transpose()
        .map_err(|error| csv_error(source, &error))?
        .ok_or_else(|| eyre!("{}: the CSV file is empty", source.display()))?;

    if header.len() != HEADER.len()
        || header
            .iter()
            .zip(HEADER)
            .any(|(actual, expected)| actual != expected)
    {
        return Err(line_error(
            source,
            header.position().map_or(1, |position| position.line()),
            format!("invalid header; expected {}", HEADER.join(",")),
        ));
    }

    let mut parsed_entries = Vec::new();
    let mut seen = HashSet::new();

    for record in records {
        let record = record.map_err(|error| csv_error(source, &error))?;
        let line = record.position().map_or(0, |position| position.line());

        if record.len() != HEADER.len() {
            return Err(line_error(
                source,
                line,
                format!("expected {} fields, found {}", HEADER.len(), record.len()),
            ));
        }

        let word_study = record[0].trim();
        let word_translation = record[1].trim();
        let part_of_speech = record[2].trim();
        let gender = record[3].trim();

        if word_study.is_empty() {
            return Err(line_error(source, line, "word_study must not be blank"));
        }
        if word_translation.is_empty() {
            return Err(line_error(
                source,
                line,
                "word_translation must not be blank",
            ));
        }

        let part_of_speech = PartOfSpeech::from_label(part_of_speech).ok_or_else(|| {
            line_error(
                source,
                line,
                format!("invalid part_of_speech value {part_of_speech:?}"),
            )
        })?;

        let gender = if gender.is_empty() {
            None
        } else {
            Some(Gender::from_csv_label(gender).ok_or_else(|| {
                line_error(source, line, format!("invalid gender value {gender:?}"))
            })?)
        };

        if !seen.insert(word_study.to_lowercase()) {
            return Err(line_error(
                source,
                line,
                format!("duplicate word_study {word_study:?}; the entire import was rejected"),
            ));
        }

        parsed_entries.push(ParsedEntry {
            line,
            entry: NewVocabularyEntry {
                word_study: word_study.to_owned(),
                word_translation: word_translation.to_owned(),
                part_of_speech,
                gender,
            },
        });
    }

    for parsed in &parsed_entries {
        if word_study_exists(connection, &parsed.entry.word_study)? {
            return Err(line_error(
                source,
                parsed.line,
                format!(
                    "duplicate word_study {:?} already exists in the database; the entire import was rejected",
                    parsed.entry.word_study
                ),
            ));
        }
    }

    let entries = parsed_entries
        .into_iter()
        .map(|parsed| parsed.entry)
        .collect::<Vec<_>>();

    match vocabulary::insert_many(connection, &entries) {
        Ok(()) => Ok(entries.len()),
        Err(error) if is_unique_violation(&error) => {
            let message = match conflicting_word_study(connection, &entries)? {
                Some(word_study) => format!(
                    "{}: duplicate word_study {word_study:?}; the entire import was rejected",
                    source.display()
                ),
                None => format!(
                    "{}: duplicate word_study value; the entire import was rejected",
                    source.display()
                ),
            };

            Err(eyre!("{message}"))
        }
        Err(error) => Err(error),
    }
}

struct ParsedEntry {
    line: u64,
    entry: NewVocabularyEntry,
}

fn word_study_exists(connection: &Connection, word_study: &str) -> Result<bool> {
    let mut statement = connection.prepare("SELECT word_study FROM vocabulary_entries")?;
    let mut rows = statement.query([])?;
    let target = word_study.to_lowercase();
    while let Some(row) = rows.next()? {
        let existing: String = row.get(0)?;
        if existing.to_lowercase() == target {
            return Ok(true);
        }
    }
    Ok(false)
}

fn conflicting_word_study(
    connection: &Connection,
    entries: &[NewVocabularyEntry],
) -> Result<Option<String>> {
    for entry in entries {
        if word_study_exists(connection, &entry.word_study)? {
            return Ok(Some(entry.word_study.clone()));
        }
    }

    Ok(None)
}

fn is_unique_violation(error: &eyre::Report) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<rusqlite::Error>()
            .is_some_and(|error| match error {
                rusqlite::Error::SqliteFailure(_, Some(message)) => {
                    message.contains("UNIQUE constraint failed")
                }
                _ => false,
            })
    })
}

fn csv_error(source: &Path, error: &csv::Error) -> eyre::Report {
    match error.position() {
        Some(position) => line_error(source, position.line(), error),
        None => eyre!("{}: {error}", source.display()),
    }
}

fn line_error(source: &Path, line: u64, message: impl std::fmt::Display) -> eyre::Report {
    eyre!("{}: line {line}: {message}", source.display())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::storage::connect_in_memory;

    #[test]
    fn rejects_case_insensitive_duplicates_in_file_and_database() -> Result<()> {
        let mut db = connect_in_memory()?;
        let source = Path::new("words.csv");
        let duplicate = "word_study,word_translation,part_of_speech,gender\nHund,perro,noun,\nhund,perro,noun,\n";
        assert!(import_reader(&mut db, Cursor::new(duplicate), source).is_err());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM vocabulary_entries", [], |row| row
                .get::<_, i64>(0))?,
            0
        );
        let first = "word_study,word_translation,part_of_speech,gender\nÄpfel,manzanas,noun,\n";
        import_reader(&mut db, Cursor::new(first), source)?;
        let duplicate = "word_study,word_translation,part_of_speech,gender\nHaus,casa,noun,\näPFEL,manzanas,noun,\n";
        assert!(import_reader(&mut db, Cursor::new(duplicate), source).is_err());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM vocabulary_entries", [], |row| row
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    }

    fn import_csv(connection: &mut Connection, contents: &str) -> Result<usize> {
        import_reader(
            connection,
            Cursor::new(contents.as_bytes()),
            Path::new("test.csv"),
        )
    }

    fn vocabulary_count(connection: &Connection) -> Result<i64> {
        connection
            .query_row("SELECT COUNT(*) FROM vocabulary_entries", [], |row| {
                row.get(0)
            })
            .wrap_err("failed to count vocabulary entries")
    }

    #[test]
    fn imports_the_fixture_and_stores_empty_gender_as_null() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test_vocabulary_002.csv");

        let imported = import_vocabulary(&mut connection, &path)?;

        assert_eq!(imported, 20);
        assert_eq!(vocabulary_count(&connection)?, 20);

        let hund_gender: Option<String> = connection.query_row(
            "SELECT gender FROM vocabulary_entries WHERE word_study = 'Hund'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(hund_gender.as_deref(), Some("Male"));

        let laufen_gender: Option<String> = connection.query_row(
            "SELECT gender FROM vocabulary_entries WHERE word_study = 'laufen'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(laufen_gender, None);

        Ok(())
    }

    #[test]
    fn invalid_row_leaves_database_unchanged() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let csv = "word_study,word_translation,part_of_speech,gender\n\
                   Hund,perro,noun,masculine\n\
                   Katze,gato,invalid,\n";

        let error = import_csv(&mut connection, csv).unwrap_err();

        assert!(error.to_string().contains("line 3"));
        assert_eq!(vocabulary_count(&connection)?, 0);

        Ok(())
    }

    #[test]
    fn duplicate_within_file_leaves_database_unchanged() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let csv = "word_study,word_translation,part_of_speech,gender\n\
                   Hund,perro,noun,masculine\n\
                   Hund,can,noun,masculine\n";

        let error = import_csv(&mut connection, csv).unwrap_err();

        assert!(error.to_string().contains("Hund"));
        assert!(error.to_string().contains("entire import was rejected"));
        assert_eq!(vocabulary_count(&connection)?, 0);

        Ok(())
    }

    #[test]
    fn duplicate_in_database_leaves_database_unchanged() -> Result<()> {
        let mut connection = connect_in_memory()?;
        vocabulary::insert_many(
            &mut connection,
            &[NewVocabularyEntry {
                word_study: "Hund".to_owned(),
                word_translation: "perro".to_owned(),
                part_of_speech: PartOfSpeech::Noun,
                gender: Some(Gender::Male),
            }],
        )?;
        let csv = "word_study,word_translation,part_of_speech,gender\n\
                   Hund,can,noun,masculine\n";

        let error = import_csv(&mut connection, csv).unwrap_err();

        assert!(error.to_string().contains("Hund"));
        assert!(error.to_string().contains("already exists"));
        assert_eq!(vocabulary_count(&connection)?, 1);

        Ok(())
    }

    #[test]
    fn rejects_an_invalid_header() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let csv = "word_study,translation,part_of_speech,gender\nHund,perro,noun,masculine\n";

        let error = import_csv(&mut connection, csv).unwrap_err();

        assert!(error.to_string().contains("invalid header"));
        assert_eq!(vocabulary_count(&connection)?, 0);

        Ok(())
    }
}
