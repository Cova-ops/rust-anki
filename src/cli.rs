use std::io::{BufRead, Write};
use std::path::PathBuf;

use chrono::{DateTime, Local, Utc};
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
    /// Delete a vocabulary entry and its reviews.
    Delete {
        #[command(subcommand)]
        target: DeleteTarget,
    },
}

#[derive(Debug, Subcommand)]
pub enum DeleteTarget {
    /// Delete a vocabulary entry by its study word.
    Vocabulary { word_study: String },
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

fn delete_vocabulary(
    connection: &mut Connection,
    word: &str,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<()> {
    let entry = vocabulary::find_by_word(connection, word)?
        .ok_or_else(|| eyre::eyre!("vocabulary word {word:?} not found"))?;
    let mut details = vec![
        ("ID", entry.id.to_string()),
        ("Study word", entry.word_study.clone()),
        ("Translation", entry.word_translation.clone()),
        ("Part of speech", {
            let value = entry.part_of_speech.as_str();
            let mut chars = value.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        }),
        (
            "Gender",
            entry
                .gender
                .map_or("none", vocabulary::Gender::as_str)
                .to_owned(),
        ),
        ("Updated (local)", format_local_date(entry.updated_at)),
    ];
    let summary = reviews::review_summary(connection, entry.id)?;
    let (total, good, again) = (summary.total, summary.good, summary.again);
    details.push(("Reviews", format!("{total} (Good: {good}, Again: {again})")));
    if let Some(schedule) = summary.schedule {
        details.extend([
            ("Next review (local)", format_local_date(schedule.due_at)),
            (
                "Last review (local)",
                format_local_date(schedule.last_reviewed_at),
            ),
            ("Stability", format!("{:.2}", schedule.stability)),
            ("Difficulty", format!("{:.2}", schedule.difficulty)),
            ("Pending retry", schedule.pending_retry.to_string()),
        ]);
    } else {
        details.push(("Review schedule", "none".to_owned()));
    }
    write_details_table(output, &details)?;
    writeln!(
        output,
        "This permanently deletes this word and all its review history."
    )?;
    loop {
        let answer = read_line(input, output, "Delete? [y/N] ")?;
        match answer.to_ascii_lowercase().as_str() {
            "y" | "yes" => {
                vocabulary::delete_with_reviews(connection, entry.id)?;
                writeln!(output, "Vocabulary entry and its reviews deleted.")?;
                return Ok(());
            }
            "" | "n" | "no" => {
                writeln!(output, "Deletion cancelled.")?;
                return Ok(());
            }
            _ => writeln!(output, "Please enter y or n.")?,
        }
    }
}

fn format_local_date(value: DateTime<Utc>) -> String {
    value
        .with_timezone(&Local)
        .format("%d %b %Y, %H:%M:%S")
        .to_string()
}

fn write_details_table(output: &mut impl Write, rows: &[(&str, String)]) -> Result<()> {
    let label_width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let value_width = rows
        .iter()
        .map(|(_, value)| value.chars().count())
        .max()
        .unwrap_or(0);
    let border = format!(
        "+-{}-+-{}-+",
        "-".repeat(label_width),
        "-".repeat(value_width)
    );
    writeln!(output, "\n\n{border}")?;
    for (label, value) in rows {
        writeln!(
            output,
            "| {}{} | {}{} |",
            label,
            " ".repeat(label_width - label.chars().count()),
            value,
            " ".repeat(value_width - value.chars().count())
        )?;
    }
    writeln!(output, "{border}\n\n")?;
    Ok(())
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
