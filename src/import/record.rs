use std::collections::HashSet;
use std::path::Path;

use eyre::Result;

use super::{HEADER, ParsedEntry, line_error};
use crate::vocabulary::{Gender, NewVocabularyEntry, PartOfSpeech};

pub(super) fn parse_record(
    record: &csv::StringRecord,
    source: &Path,
    seen: &mut HashSet<String>,
) -> Result<ParsedEntry> {
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
    let gender =
        if gender.is_empty() {
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
    Ok(ParsedEntry {
        line,
        entry: NewVocabularyEntry {
            word_study: word_study.to_owned(),
            word_translation: word_translation.to_owned(),
            part_of_speech,
            gender,
        },
    })
}
