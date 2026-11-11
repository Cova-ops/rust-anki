use std::io::{BufRead, Write};
use std::path::PathBuf;

use chrono::Utc;
use clap::{Parser, Subcommand};
use eyre::{Context, Result};
use rusqlite::Connection;

use crate::import;
use crate::speech::{MacRecognizer, SystemClock};
use crate::storage::{reviews, vocabulary};
use crate::study::{self, StudyEntry};
use crate::synthesis::MacSynthesizer;

#[derive(Debug, Parser)]
#[command(name = "rust-anki", about = "Vocabulary study CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Import vocabulary data.
    Upload {
        #[command(subcommand)]
        target: UploadTarget,
    },
    /// Study vocabulary.
    Study {
        #[command(subcommand)]
        target: StudyTarget,
    },
    /// Edit an existing vocabulary entry.
    Edit {
        #[command(subcommand)]
        target: EditTarget,
    },
}

#[derive(Debug, Subcommand)]
pub enum EditTarget {
    /// Edit a vocabulary entry by its study word.
    Vocabulary { word_study: String },
}

#[derive(Debug, Subcommand)]
pub enum UploadTarget {
    /// Import vocabulary from a CSV file.
    Vocabulary {
        /// Path to the UTF-8 CSV file.
        csv_path: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum StudyTarget {
    /// Study vocabulary using typed answers.
    Vocabulary,
}

pub fn run(cli: Cli, connection: &mut Connection) -> Result<()> {
    match cli.command {
        Command::Upload {
            target: UploadTarget::Vocabulary { csv_path },
        } => {
            let imported = import::import_vocabulary(connection, &csv_path)?;
            println!("Imported {imported} vocabulary entries.");
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
    }
}

fn read_line(input: &mut impl BufRead, output: &mut impl Write, prompt: &str) -> Result<String> {
    write!(output, "{prompt}")?;
    output.flush()?;
    let mut line = String::new();
    eyre::ensure!(
        input.read_line(&mut line)? != 0,
        "input ended; no changes were saved"
    );
    Ok(line.trim().to_owned())
}

fn wants_edit(
    input: &mut impl BufRead,
    output: &mut impl Write,
    field: &str,
    current: &str,
) -> Result<bool> {
    loop {
        let answer = read_line(input, output, &format!("{field}: {current}. Edit? [y/N] "))?;
        match answer.to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "" | "n" | "no" => return Ok(false),
            _ => writeln!(output, "Please enter y or n.")?,
        }
    }
}

fn edit_vocabulary(
    connection: &Connection,
    word: &str,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<()> {
    let mut entry = vocabulary::find_by_word(connection, word)?
        .ok_or_else(|| eyre::eyre!("vocabulary word {word:?} not found"))?;
    let original = entry.clone();
    if wants_edit(input, output, "word_translation", &entry.word_translation)? {
        loop {
            let value = read_line(input, output, "New translation: ")?;
            if !value.is_empty() {
                entry.word_translation = value;
                break;
            }
            writeln!(output, "Translation cannot be empty.")?;
        }
    }
    if wants_edit(
        input,
        output,
        "part_of_speech",
        entry.part_of_speech.as_str(),
    )? {
        loop {
            let value = read_line(input, output, "New part of speech: ")?;
            if let Some(part) = vocabulary::PartOfSpeech::from_label(&value) {
                entry.part_of_speech = part;
                break;
            }
            writeln!(output, "Invalid part of speech.")?;
        }
    }
    if wants_edit(
        input,
        output,
        "gender",
        entry.gender.map_or("none", vocabulary::Gender::as_str),
    )? {
        loop {
            let value = read_line(
                input,
                output,
                "New gender (masculine, feminine, neuter, or none): ",
            )?;
            if value.is_empty() || value.eq_ignore_ascii_case("none") {
                entry.gender = None;
                break;
            }
            if let Some(gender) = vocabulary::Gender::from_csv_label(&value) {
                entry.gender = Some(gender);
                break;
            }
            writeln!(output, "Invalid gender.")?;
        }
    }
    if entry != original {
        vocabulary::update_editable(connection, &entry)?;
        writeln!(output, "Vocabulary entry updated.")?;
    } else {
        writeln!(output, "No changes made.")?;
    }
    Ok(())
}

fn run_study(connection: &mut Connection) -> Result<()> {
    let entry_ids = reviews::select_all_study_entry_ids(connection, Utc::now())?;

    if entry_ids.is_empty() {
        println!("There is nothing to review right now.");
        return Ok(());
    }

    let entries = entry_ids
        .iter()
        .map(|id| vocabulary::find_by_id(connection, *id))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .map(|entry| StudyEntry {
            id: entry.id,
            word_study: entry.word_study,
            word_translation: entry.word_translation,
            article: entry
                .gender
                .map(|gender| gender.german_article().to_owned()),
        })
        .collect::<Vec<_>>();

    if entries.is_empty() {
        println!("There is nothing to review right now.");
        return Ok(());
    }

    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let mut recognizer = MacRecognizer::new();
    let mut synthesizer = MacSynthesizer::new()?;

    study::run_with_synthesizer(
        &entries,
        &mut output,
        &mut recognizer,
        &mut synthesizer,
        SystemClock::start,
        |entry_id, grade| reviews::record_review(connection, entry_id, grade, Utc::now()),
    )
    .wrap_err("study session failed")
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

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
