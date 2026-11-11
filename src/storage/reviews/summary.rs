use chrono::{DateTime, Utc};
use eyre::{Context, Result};
use rusqlite::{Connection, OptionalExtension};

use super::parse_timestamp;

/// Review counts and the current schedule for a vocabulary entry.
pub struct ReviewSummary {
    pub total: i64,
    pub good: i64,
    pub again: i64,
    pub schedule: Option<ReviewScheduleSummary>,
}

/// Stored schedule details used when displaying a review summary.
pub struct ReviewScheduleSummary {
    pub due_at: DateTime<Utc>,
    pub last_reviewed_at: DateTime<Utc>,
    pub stability: f64,
    pub difficulty: f64,
    pub pending_retry: bool,
}

/// Loads review counts and the optional schedule for an entry.
pub fn review_summary(connection: &Connection, entry_id: i32) -> Result<ReviewSummary> {
    let (total, good, again) = connection.query_row(
        "SELECT COUNT(*), COUNT(CASE WHEN grade = 'good' THEN 1 END), COUNT(CASE WHEN grade = 'again' THEN 1 END) FROM review_events WHERE vocabulary_entry_id = ?1",
        [entry_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).wrap_err("failed to load review counts")?;
    let schedule = connection.query_row(
        "SELECT due_at, stability, difficulty, last_reviewed_at, pending_retry FROM review_schedule WHERE vocabulary_entry_id = ?1",
        [entry_id], |row| Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?, row.get::<_, String>(3)?, row.get(4)?)),
    ).optional().wrap_err("failed to load review schedule")?
        .map(|(due, stability, difficulty, last, pending_retry)| -> Result<_> {
            Ok(ReviewScheduleSummary { due_at: parse_timestamp(&due)?, last_reviewed_at: parse_timestamp(&last)?, stability, difficulty, pending_retry })
        }).transpose()?;
    Ok(ReviewSummary {
        total,
        good,
        again,
        schedule,
    })
}
