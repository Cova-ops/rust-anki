use chrono::{DateTime, Utc};
use eyre::{Context, Result};
use rusqlite::{Connection, params};

use super::format_timestamp;

/// Number of entries presented by the legacy limited selection.
pub const SESSION_SIZE: usize = 5;

const SELECT_DUE_IDS: &str = "
SELECT vocabulary_entry_id FROM review_schedule
WHERE due_at <= ?1 ORDER BY due_at ASC, vocabulary_entry_id ASC LIMIT ?2
";
const SELECT_NEW_IDS: &str = "
SELECT vocabulary_entries.id FROM vocabulary_entries
LEFT JOIN review_schedule ON review_schedule.vocabulary_entry_id = vocabulary_entries.id
WHERE review_schedule.vocabulary_entry_id IS NULL ORDER BY vocabulary_entries.id ASC LIMIT ?1
";

/// Selects due entries first, oldest due date first, then never-reviewed entries.
pub fn select_study_entry_ids(
    connection: &Connection,
    now: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<i32>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let now = format_timestamp(now);
    let mut ids = Vec::with_capacity(limit);
    {
        let mut statement = connection
            .prepare(SELECT_DUE_IDS)
            .wrap_err("failed to prepare the due vocabulary query")?;
        let rows = statement
            .query_map(params![now, limit as i64], |row| row.get::<_, i32>(0))
            .wrap_err("failed to select due vocabulary entries")?;
        for row in rows {
            ids.push(row.wrap_err("failed to read a due vocabulary entry")?);
        }
    }
    if ids.len() < limit {
        let remaining = (limit - ids.len()) as i64;
        let mut statement = connection
            .prepare(SELECT_NEW_IDS)
            .wrap_err("failed to prepare the new vocabulary query")?;
        let rows = statement
            .query_map([remaining], |row| row.get::<_, i32>(0))
            .wrap_err("failed to select new vocabulary entries")?;
        for row in rows {
            ids.push(row.wrap_err("failed to read a new vocabulary entry")?);
        }
    }
    Ok(ids)
}

/// Selects every new entry first, then every due scheduled entry.
pub fn select_all_study_entry_ids(connection: &Connection, now: DateTime<Utc>) -> Result<Vec<i32>> {
    let now = format_timestamp(now);
    let mut ids = Vec::new();
    let mut new_entries = connection.prepare(SELECT_NEW_IDS)?;
    for row in new_entries.query_map([i64::MAX], |row| row.get(0))? {
        ids.push(row?);
    }
    let mut due_entries = connection.prepare(SELECT_DUE_IDS)?;
    for row in due_entries.query_map(params![now, i64::MAX], |row| row.get(0))? {
        ids.push(row?);
    }
    Ok(ids)
}
