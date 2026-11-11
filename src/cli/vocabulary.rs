use super::output::write_deletion_preview;
use crate::storage::{reviews, vocabulary};
use eyre::Result;
use rusqlite::Connection;
use std::io::{BufRead, Write};

pub(super) fn delete_vocabulary(
    connection: &mut Connection,
    word: &str,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<()> {
    let entry = vocabulary::find_by_word(connection, word)?
        .ok_or_else(|| eyre::eyre!("vocabulary word {word:?} not found"))?;
    let summary = reviews::review_summary(connection, entry.id)?;
    write_deletion_preview(output, &entry, &summary)?;
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

pub(super) fn edit_vocabulary(
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
