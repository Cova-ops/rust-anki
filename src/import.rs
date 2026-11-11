use std::collections::HashSet;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use eyre::{Context, Result, eyre};
use rusqlite::Connection;

use crate::storage::vocabulary;
use crate::vocabulary::{NewVocabularyEntry, VocabularyEntry};

mod record;

const HEADER: [&str; 4] = ["word_study", "word_translation", "part_of_speech", "gender"];

/// A CSV entry that conflicts with an existing database entry.
#[derive(Debug)]
pub struct DuplicateVocabulary {
    pub source: std::path::PathBuf,
    pub line: u64,
    pub incoming: NewVocabularyEntry,
    pub existing: VocabularyEntry,
}

impl fmt::Display for DuplicateVocabulary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: line {}: duplicate word_study {:?} already exists in the database; the entire import was rejected",
            self.source.display(),
            self.line,
            self.incoming.word_study
        )
    }
}

impl std::error::Error for DuplicateVocabulary {}

/// User decision for a conflicting vocabulary entry.
pub enum DuplicateDecision {
    Replace,
    Omit,
}

/// Counts of successfully processed CSV entries.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ImportSummary {
    pub imported: usize,
    pub replaced: usize,
    pub omitted: usize,
}

/// Imports vocabulary from a UTF-8 CSV file.
pub fn import_vocabulary(connection: &mut Connection, path: &Path) -> Result<usize> {
    let file =
        File::open(path).wrap_err_with(|| format!("failed to open CSV file {}", path.display()))?;

    import_reader(connection, BufReader::new(file), path)
}

/// Imports a CSV, asking the caller how to resolve differing existing entries.
pub fn import_vocabulary_with_resolver<F>(
    connection: &mut Connection,
    path: &Path,
    mut resolve: F,
) -> Result<ImportSummary>
where
    F: FnMut(&DuplicateVocabulary) -> Result<DuplicateDecision>,
{
    let file =
        File::open(path).wrap_err_with(|| format!("failed to open CSV file {}", path.display()))?;
    import_reader_with_resolver(connection, BufReader::new(file), path, &mut resolve)
}

fn import_reader<R: Read>(connection: &mut Connection, reader: R, source: &Path) -> Result<usize> {
    import_reader_with_resolver(connection, reader, source, &mut |duplicate| {
        Err(DuplicateVocabulary {
            source: duplicate.source.clone(),
            line: duplicate.line,
            incoming: duplicate.incoming.clone(),
            existing: duplicate.existing.clone(),
        }
        .into())
    })
    .map(|summary| summary.imported)
}

fn import_reader_with_resolver<R: Read, F>(
    connection: &mut Connection,
    reader: R,
    source: &Path,
    resolve: &mut F,
) -> Result<ImportSummary>
where
    F: FnMut(&DuplicateVocabulary) -> Result<DuplicateDecision>,
{
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
        parsed_entries.push(record::parse_record(&record, source, &mut seen)?);
    }

    let mut actions = Vec::new();
    let mut summary = ImportSummary::default();
    for parsed in parsed_entries {
        match vocabulary::find_by_word(connection, &parsed.entry.word_study)? {
            None => actions.push(vocabulary::ImportAction::Insert(parsed.entry)),
            Some(existing)
                if existing.word_study == parsed.entry.word_study
                    && existing.word_translation == parsed.entry.word_translation
                    && existing.part_of_speech == parsed.entry.part_of_speech
                    && existing.gender == parsed.entry.gender =>
            {
                summary.omitted += 1;
            }
            Some(existing) => {
                let duplicate = DuplicateVocabulary {
                    source: source.to_owned(),
                    line: parsed.line,
                    incoming: parsed.entry,
                    existing,
                };
                match resolve(&duplicate)? {
                    DuplicateDecision::Replace => {
                        summary.replaced += 1;
                        actions.push(vocabulary::ImportAction::Replace {
                            incoming: duplicate.incoming,
                            original: duplicate.existing,
                        });
                    }
                    DuplicateDecision::Omit => {
                        summary.omitted += 1;
                    }
                }
            }
        }
    }
    summary.imported = vocabulary::apply_import(connection, &actions)?;
    Ok(summary)
}

struct ParsedEntry {
    line: u64,
    entry: NewVocabularyEntry,
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
    use crate::storage::vocabulary::{Gender, PartOfSpeech};

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

    #[test]
    fn duplicate_in_database_contains_both_versions_and_rolls_back() -> Result<()> {
        let mut db = connect_in_memory()?;
        import_csv(
            &mut db,
            "word_study,word_translation,part_of_speech,gender\nHund,perro,noun,masculine\n",
        )?;
        let error = import_csv(&mut db, "word_study,word_translation,part_of_speech,gender\nHaus,casa,noun,neuter\nhund,can,verb,\n").unwrap_err();
        let duplicate = error
            .downcast_ref::<DuplicateVocabulary>()
            .expect("duplicate details");
        assert_eq!(duplicate.line, 3);
        assert_eq!(duplicate.existing.word_study, "Hund");
        assert_eq!(duplicate.existing.word_translation, "perro");
        assert_eq!(duplicate.incoming.word_study, "hund");
        assert_eq!(duplicate.incoming.word_translation, "can");
        assert_eq!(vocabulary_count(&db)?, 1);
        Ok(())
    }

    #[test]
    fn identical_existing_entries_are_skipped_and_new_entries_are_imported() -> Result<()> {
        let mut db = connect_in_memory()?;
        let header = "word_study,word_translation,part_of_speech,gender\n";
        let initial = format!("{header}Hund,perro,noun,masculine\n");
        assert_eq!(import_csv(&mut db, &initial)?, 1);
        let before = vocabulary::find_by_word(&db, "Hund")?.unwrap();
        let repeated = format!("{header}Hund,perro,noun,masculine\nKatze,gato,noun,feminine\n");
        assert_eq!(import_csv(&mut db, &repeated)?, 1);
        assert_eq!(vocabulary::find_by_word(&db, "Hund")?.unwrap(), before);
        assert_eq!(vocabulary_count(&db)?, 2);
        assert_eq!(import_csv(&mut db, &repeated)?, 0);
        let summary = import_reader_with_resolver(
            &mut db,
            Cursor::new(&repeated),
            Path::new("test.csv"),
            &mut |_| panic!("identical entries must not prompt"),
        )?;
        assert_eq!(
            summary,
            ImportSummary {
                imported: 0,
                replaced: 0,
                omitted: 2
            }
        );
        Ok(())
    }

    #[test]
    fn replace_and_omit_preserve_reviews_and_import_other_entries() -> Result<()> {
        use crate::review::Grade;
        use crate::storage::reviews;
        let mut db = connect_in_memory()?;
        import_csv(
            &mut db,
            "word_study,word_translation,part_of_speech,gender\nHund,perro,noun,masculine\nKatze,gato,noun,feminine\n",
        )?;
        let dog = vocabulary::find_by_word(&db, "Hund")?.unwrap();
        reviews::record_review(&mut db, dog.id, Grade::Good, chrono::Utc::now())?;
        let schedule: String = db.query_row(
            "SELECT due_at FROM review_schedule WHERE vocabulary_entry_id = ?1",
            [dog.id],
            |row| row.get(0),
        )?;
        let csv = "word_study,word_translation,part_of_speech,gender\nhund,can,noun,masculine\nKatze,felino,noun,feminine\nHaus,casa,noun,neuter\n";
        let mut resolver = |duplicate: &DuplicateVocabulary| {
            Ok(if duplicate.incoming.word_study == "hund" {
                DuplicateDecision::Replace
            } else {
                DuplicateDecision::Omit
            })
        };
        assert_eq!(
            import_reader_with_resolver(
                &mut db,
                Cursor::new(csv),
                Path::new("test.csv"),
                &mut resolver
            )?,
            ImportSummary {
                imported: 1,
                replaced: 1,
                omitted: 1
            }
        );
        let updated = vocabulary::find_by_word(&db, "hund")?.unwrap();
        assert_eq!(updated.id, dog.id);
        assert_eq!(updated.word_study, "hund");
        assert_eq!(updated.word_translation, "can");
        assert!(updated.updated_at >= dog.updated_at);
        assert_eq!(
            vocabulary::find_by_word(&db, "Katze")?
                .unwrap()
                .word_translation,
            "gato"
        );
        assert_eq!(
            db.query_row(
                "SELECT due_at FROM review_schedule WHERE vocabulary_entry_id = ?1",
                [dog.id],
                |row| row.get::<_, String>(0)
            )?,
            schedule
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM review_events WHERE vocabulary_entry_id = ?1",
                [dog.id],
                |row| row.get::<_, i64>(0)
            )?,
            1
        );
        Ok(())
    }

    #[test]
    fn cancelled_resolution_saves_no_changes() -> Result<()> {
        let mut db = connect_in_memory()?;
        import_csv(
            &mut db,
            "word_study,word_translation,part_of_speech,gender\nHund,perro,noun,masculine\n",
        )?;
        let csv = "word_study,word_translation,part_of_speech,gender\nHaus,casa,noun,neuter\nHund,can,noun,masculine\n";
        let error = import_reader_with_resolver(
            &mut db,
            Cursor::new(csv),
            Path::new("test.csv"),
            &mut |_| Err(eyre!("cancelled")),
        );
        assert!(error.is_err());
        assert_eq!(vocabulary_count(&db)?, 1);
        Ok(())
    }

    #[test]
    fn case_only_difference_is_a_conflict_not_an_exact_duplicate() -> Result<()> {
        let mut db = connect_in_memory()?;
        import_csv(
            &mut db,
            "word_study,word_translation,part_of_speech,gender\nHund,perro,noun,masculine\n",
        )?;
        let error = import_csv(
            &mut db,
            "word_study,word_translation,part_of_speech,gender\nhund,perro,noun,masculine\n",
        )
        .unwrap_err();
        assert!(error.downcast_ref::<DuplicateVocabulary>().is_some());
        assert_eq!(vocabulary_count(&db)?, 1);
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
