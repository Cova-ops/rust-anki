use std::io::Write;

use crate::storage::reviews::ReviewSummary;
use crate::vocabulary::{Gender, VocabularyEntry};
use chrono::{DateTime, Local, Utc};
use eyre::Result;

pub(super) fn write_duplicate_comparison(
    output: &mut impl Write,
    duplicate: &crate::import::DuplicateVocabulary,
) -> Result<()> {
    let existing = &duplicate.existing;
    let incoming = &duplicate.incoming;
    let rows = [
        (
            "Field",
            "Existing database".to_owned(),
            "Incoming CSV".to_owned(),
        ),
        (
            "Study word",
            existing.word_study.clone(),
            incoming.word_study.clone(),
        ),
        (
            "Translation",
            existing.word_translation.clone(),
            incoming.word_translation.clone(),
        ),
        (
            "Part of speech",
            existing.part_of_speech.as_str().to_owned(),
            incoming.part_of_speech.as_str().to_owned(),
        ),
        (
            "Gender",
            existing.gender.map_or("none", Gender::as_str).to_owned(),
            incoming.gender.map_or("none", Gender::as_str).to_owned(),
        ),
    ];
    let widths = [
        rows.iter()
            .map(|(label, _, _)| label.chars().count())
            .max()
            .unwrap_or(0),
        rows.iter()
            .map(|(_, value, _)| value.chars().count())
            .max()
            .unwrap_or(0),
        rows.iter()
            .map(|(_, _, value)| value.chars().count())
            .max()
            .unwrap_or(0),
    ];
    let border = format!(
        "+-{}-+-{}-+-{}-+",
        "-".repeat(widths[0]),
        "-".repeat(widths[1]),
        "-".repeat(widths[2])
    );
    writeln!(
        output,
        "\nThis word already exists in the database:\n{border}"
    )?;
    for (index, (field, stored, csv)) in rows.iter().enumerate() {
        writeln!(
            output,
            "| {}{} | {}{} | {}{} |",
            field,
            " ".repeat(widths[0] - field.chars().count()),
            stored,
            " ".repeat(widths[1] - stored.chars().count()),
            csv,
            " ".repeat(widths[2] - csv.chars().count())
        )?;
        if index == 0 {
            writeln!(output, "{border}")?;
        }
    }
    writeln!(output, "{border}\n")?;
    Ok(())
}

pub(super) fn write_deletion_preview(
    output: &mut impl Write,
    entry: &VocabularyEntry,
    summary: &ReviewSummary,
) -> Result<()> {
    let mut details = vec![
        ("ID", entry.id.to_string()),
        ("Study word", entry.word_study.clone()),
        ("Translation", entry.word_translation.clone()),
        ("Part of speech", {
            let mut chars = entry.part_of_speech.as_str().chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        }),
        (
            "Gender",
            entry.gender.map_or("none", Gender::as_str).to_owned(),
        ),
        ("Updated (local)", format_local_date(entry.updated_at)),
        (
            "Reviews",
            format!(
                "{} (Good: {}, Again: {})",
                summary.total, summary.good, summary.again
            ),
        ),
    ];
    if let Some(schedule) = &summary.schedule {
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
    write_details_table(output, &details)
}

pub(super) fn format_local_date(value: DateTime<Utc>) -> String {
    value
        .with_timezone(&Local)
        .format("%d %b %Y, %H:%M:%S")
        .to_string()
}

pub(super) fn write_details_table(output: &mut impl Write, rows: &[(&str, String)]) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::DuplicateVocabulary;
    use crate::vocabulary::{NewVocabularyEntry, PartOfSpeech, VocabularyEntry};

    #[test]
    fn duplicate_table_shows_existing_and_incoming_values() -> Result<()> {
        let duplicate = DuplicateVocabulary {
            source: "test.csv".into(),
            line: 2,
            existing: VocabularyEntry {
                id: 1,
                word_study: "Hund".into(),
                word_translation: "perro".into(),
                part_of_speech: PartOfSpeech::Noun,
                gender: Some(Gender::Male),
                updated_at: Utc::now(),
            },
            incoming: NewVocabularyEntry {
                word_study: "hund".into(),
                word_translation: "can".into(),
                part_of_speech: PartOfSpeech::Verb,
                gender: None,
            },
        };
        let mut output = Vec::new();
        write_duplicate_comparison(&mut output, &duplicate)?;
        let text = String::from_utf8(output)?;
        for value in [
            "Existing database",
            "Incoming CSV",
            "Hund",
            "hund",
            "perro",
            "can",
            "noun",
            "verb",
            "Male",
            "none",
        ] {
            assert!(text.contains(value), "missing {value}");
        }
        Ok(())
    }
}
