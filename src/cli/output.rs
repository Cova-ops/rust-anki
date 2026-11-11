use std::io::Write;

use crate::storage::reviews::ReviewSummary;
use crate::vocabulary::{Gender, VocabularyEntry};
use chrono::{DateTime, Local, Utc};
use eyre::Result;

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
