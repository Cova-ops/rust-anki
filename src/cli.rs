use eyre::Result;
use rusqlite::Connection;

use crate::import;

mod args;
mod output;
mod vocabulary;
use crate::app::study_session::run_study;
pub use args::{Cli, Command, DeleteTarget, EditTarget, StudyTarget, UploadTarget};
use vocabulary::{delete_vocabulary, edit_vocabulary};

pub fn run(cli: Cli, connection: &mut Connection) -> Result<()> {
    match cli.command {
        Command::Upload {
            target: UploadTarget::Vocabulary { csv_path },
        } => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let mut input = stdin.lock();
            let mut output = stdout.lock();
            let summary =
                import::import_vocabulary_with_resolver(connection, &csv_path, |duplicate| {
                    resolve_duplicate(&mut input, &mut output, duplicate)
                })?;
            drop(output);
            println!("Imported {} vocabulary entries.", summary.imported);
            println!("Replaced {} vocabulary entries.", summary.replaced);
            println!("Omitted {} vocabulary entries.", summary.omitted);
            Ok(())
        }
        Command::Study {
            target: StudyTarget::Vocabulary,
        } => run_study(connection),
        Command::Edit {
            target: EditTarget::Vocabulary { word_study },
        } => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            edit_vocabulary(
                connection,
                &word_study,
                &mut stdin.lock(),
                &mut stdout.lock(),
            )
        }
        Command::Delete {
            target: DeleteTarget::Vocabulary { word_study },
        } => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            delete_vocabulary(
                connection,
                &word_study,
                &mut stdin.lock(),
                &mut stdout.lock(),
            )
        }
    }
}

fn resolve_duplicate(
    input: &mut impl std::io::BufRead,
    output: &mut impl std::io::Write,
    duplicate: &import::DuplicateVocabulary,
) -> Result<import::DuplicateDecision> {
    output::write_duplicate_comparison(output, duplicate)?;
    loop {
        write!(output, "Replace or omit? [r/o] ")?;
        output.flush()?;
        let mut line = String::new();
        eyre::ensure!(
            input.read_line(&mut line)? != 0,
            "input ended; no changes were saved"
        );
        match line.trim().to_ascii_lowercase().as_str() {
            "r" => return Ok(import::DuplicateDecision::Replace),
            "o" => return Ok(import::DuplicateDecision::Omit),
            _ => writeln!(output, "Please enter r or o.")?,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::{reviews, vocabulary};
    use chrono::Utc;
    use clap::Parser;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn duplicate_prompt_requires_explicit_choice() -> Result<()> {
        let duplicate = import::DuplicateVocabulary {
            source: "test.csv".into(),
            line: 2,
            incoming: crate::vocabulary::NewVocabularyEntry {
                word_study: "Hund".into(),
                word_translation: "can".into(),
                part_of_speech: crate::vocabulary::PartOfSpeech::Noun,
                gender: None,
            },
            existing: crate::vocabulary::VocabularyEntry {
                id: 1,
                word_study: "Hund".into(),
                word_translation: "perro".into(),
                part_of_speech: crate::vocabulary::PartOfSpeech::Noun,
                gender: None,
                updated_at: Utc::now(),
            },
        };
        let mut output = Vec::new();
        let decision = resolve_duplicate(
            &mut std::io::Cursor::new(b"\ninvalid\nr\n"),
            &mut output,
            &duplicate,
        )?;
        assert!(matches!(decision, import::DuplicateDecision::Replace));
        let text = String::from_utf8(output)?;
        assert_eq!(text.matches("Replace or omit? [r/o]").count(), 3);
        assert_eq!(text.matches("Please enter r or o.").count(), 2);
        Ok(())
    }

    #[test]
    fn deletion_requires_confirmation_and_removes_only_selected_reviews() -> Result<()> {
        use crate::review::Grade;
        use crate::storage::{
            connect_in_memory,
            vocabulary::{NewVocabularyEntry, PartOfSpeech},
        };
        let mut db = connect_in_memory()?;
        let entries = ["Hund", "Katze"].map(|word| NewVocabularyEntry {
            word_study: word.into(),
            word_translation: "translation".into(),
            part_of_speech: PartOfSpeech::Noun,
            gender: None,
        });
        vocabulary::insert_many(&mut db, &entries)?;
        let dog = vocabulary::find_by_word(&db, "Hund")?.unwrap();
        let cat = vocabulary::find_by_word(&db, "Katze")?.unwrap();
        reviews::record_review(&mut db, dog.id, Grade::Good, Utc::now())?;
        reviews::record_review(&mut db, cat.id, Grade::Again, Utc::now())?;
        let mut output = Vec::new();
        delete_vocabulary(
            &mut db,
            "hund",
            &mut std::io::Cursor::new(b"\n"),
            &mut output,
        )?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("Study word") && text.contains("Hund"));
        assert!(text.contains("1 (Good: 1, Again: 0)"));
        assert!(text.contains("Next review (local)"));
        assert!(text.starts_with("\n\n+-"));
        assert!(vocabulary::find_by_id(&db, dog.id)?.is_some());
        delete_vocabulary(
            &mut db,
            "HUND",
            &mut std::io::Cursor::new(b"yes\n"),
            &mut Vec::new(),
        )?;
        assert!(vocabulary::find_by_id(&db, dog.id)?.is_none());
        assert!(vocabulary::find_by_id(&db, cat.id)?.is_some());
        for table in ["review_events", "review_schedule"] {
            let remaining: i64 = db.query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE vocabulary_entry_id = ?1"),
                [dog.id],
                |r| r.get(0),
            )?;
            assert_eq!(remaining, 0);
            let remaining: i64 = db.query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE vocabulary_entry_id = ?1"),
                [cat.id],
                |r| r.get(0),
            )?;
            assert_eq!(remaining, 1);
        }
        Ok(())
    }

    #[test]
    fn deletion_rolls_back_when_a_delete_fails() -> Result<()> {
        use crate::review::Grade;
        use crate::storage::{
            connect_in_memory,
            vocabulary::{NewVocabularyEntry, PartOfSpeech},
        };
        let mut db = connect_in_memory()?;
        vocabulary::insert_many(
            &mut db,
            &[NewVocabularyEntry {
                word_study: "Hund".into(),
                word_translation: "perro".into(),
                part_of_speech: PartOfSpeech::Noun,
                gender: None,
            }],
        )?;
        let entry = vocabulary::find_by_word(&db, "Hund")?.unwrap();
        reviews::record_review(&mut db, entry.id, Grade::Good, Utc::now())?;
        db.execute_batch("CREATE TRIGGER prevent_deletion BEFORE DELETE ON vocabulary_entries BEGIN SELECT RAISE(ABORT, 'blocked'); END;")?;
        assert!(
            delete_vocabulary(
                &mut db,
                "Hund",
                &mut std::io::Cursor::new(b"y\n"),
                &mut Vec::new()
            )
            .is_err()
        );
        for table in ["vocabulary_entries", "review_events", "review_schedule"] {
            assert_eq!(
                db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
        }
        Ok(())
    }

    #[test]
    fn blank_answers_skip_fields_and_lookup_ignores_case() -> Result<()> {
        use crate::storage::{
            connect_in_memory,
            vocabulary::{NewVocabularyEntry, PartOfSpeech},
        };
        let mut db = connect_in_memory()?;
        vocabulary::insert_many(
            &mut db,
            &[NewVocabularyEntry {
                word_study: "Äpfel".into(),
                word_translation: "manzanas".into(),
                part_of_speech: PartOfSpeech::Noun,
                gender: None,
            }],
        )?;
        let before = vocabulary::find_by_word(&db, "Äpfel")?.unwrap();
        let mut output = Vec::new();
        edit_vocabulary(
            &db,
            "äPFEL",
            &mut std::io::Cursor::new(b"\n\n\n"),
            &mut output,
        )?;
        assert_eq!(vocabulary::find_by_word(&db, "äpfel")?.unwrap(), before);
        assert!(String::from_utf8(output)?.contains("No changes made."));
        Ok(())
    }

    #[test]
    fn ambiguous_case_insensitive_lookup_does_not_edit() -> Result<()> {
        let db = crate::storage::connect_in_memory()?;
        db.execute_batch("INSERT INTO vocabulary_entries (word_study, word_translation, part_of_speech) VALUES ('Hund', 'perro', 'noun'), ('hund', 'perro', 'noun');")?;
        let error = edit_vocabulary(
            &db,
            "HUND",
            &mut std::io::Cursor::new(b"y\nx\n"),
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("multiple vocabulary entries"));
        Ok(())
    }

    #[test]
    fn edits_fields_without_affecting_review_data() -> Result<()> {
        use crate::review::Grade;
        use crate::storage::{
            connect_in_memory,
            vocabulary::{Gender, NewVocabularyEntry, PartOfSpeech},
        };
        let mut db = connect_in_memory()?;
        vocabulary::insert_many(
            &mut db,
            &[NewVocabularyEntry {
                word_study: "Hund".into(),
                word_translation: "perro".into(),
                part_of_speech: PartOfSpeech::Noun,
                gender: Some(Gender::Male),
            }],
        )?;
        let before = vocabulary::find_by_word(&db, "Hund")?.unwrap();
        reviews::record_review(&mut db, before.id, Grade::Good, Utc::now())?;
        let schedule: (String, f64, f64) = db.query_row(
            "SELECT due_at, stability, difficulty FROM review_schedule",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let mut input = std::io::Cursor::new(b"y\ncan\ny\nverb\ny\nnone\n");
        edit_vocabulary(&db, "Hund", &mut input, &mut Vec::new())?;
        let after = vocabulary::find_by_word(&db, "Hund")?.unwrap();
        assert_eq!(after.word_study, "Hund");
        assert_eq!(after.word_translation, "can");
        assert_eq!(after.part_of_speech, PartOfSpeech::Verb);
        assert_eq!(after.gender, None);
        assert!(after.updated_at >= before.updated_at);
        let actual = db.query_row(
            "SELECT due_at, stability, difficulty FROM review_schedule",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
        )?;
        assert_eq!(schedule, actual);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM review_events", [], |row| row
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    }

    #[test]
    fn interrupted_edit_does_not_save() -> Result<()> {
        use crate::storage::{
            connect_in_memory,
            vocabulary::{NewVocabularyEntry, PartOfSpeech},
        };
        let mut db = connect_in_memory()?;
        vocabulary::insert_many(
            &mut db,
            &[NewVocabularyEntry {
                word_study: "Hund".into(),
                word_translation: "perro".into(),
                part_of_speech: PartOfSpeech::Noun,
                gender: None,
            }],
        )?;
        let mut input = std::io::Cursor::new(b"y\ncan\n");
        assert!(edit_vocabulary(&db, "Hund", &mut input, &mut Vec::new()).is_err());
        assert_eq!(
            vocabulary::find_by_word(&db, "Hund")?
                .unwrap()
                .word_translation,
            "perro"
        );
        Ok(())
    }

    #[test]
    fn parses_upload_vocabulary() {
        let cli = Cli::parse_from(["rust-anki", "upload", "vocabulary", "words.csv"]);

        match cli.command {
            Command::Upload {
                target: UploadTarget::Vocabulary { csv_path },
            } => assert_eq!(csv_path, PathBuf::from("words.csv")),
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn parses_study_vocabulary() {
        let cli = Cli::parse_from(["rust-anki", "study", "vocabulary"]);

        assert!(matches!(
            cli.command,
            Command::Study {
                target: StudyTarget::Vocabulary
            }
        ));
    }
}
