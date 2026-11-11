use chrono::Utc;
use eyre::{Context, Result};
use rusqlite::Connection;

use crate::speech::{MacRecognizer, SystemClock};
use crate::storage::{reviews, vocabulary};
use crate::study::{self, StudyEntry};
use crate::synthesis::MacSynthesizer;

pub(crate) fn run_study(connection: &mut Connection) -> Result<()> {
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
