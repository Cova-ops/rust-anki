use chrono::{DateTime, SecondsFormat, Utc};
use eyre::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::review::{self, Grade, ReviewState};

mod selection;
mod summary;
pub use selection::{SESSION_SIZE, select_all_study_entry_ids, select_study_entry_ids};
pub use summary::{ReviewScheduleSummary, ReviewSummary, review_summary};

const SELECT_SCHEDULE: &str = "
SELECT stability, difficulty, last_reviewed_at
FROM review_schedule
WHERE vocabulary_entry_id = ?1
";

const INSERT_EVENT: &str = "
INSERT INTO review_events (vocabulary_entry_id, reviewed_at, grade)
VALUES (?1, ?2, ?3)
";

const UPSERT_SCHEDULE: &str = "
INSERT INTO review_schedule
    (vocabulary_entry_id, due_at, stability, difficulty, last_reviewed_at)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT (vocabulary_entry_id) DO UPDATE SET
    due_at = excluded.due_at,
    stability = excluded.stability,
    difficulty = excluded.difficulty,
    last_reviewed_at = excluded.last_reviewed_at
";

/// Saves one review event and updates the entry's schedule in a single transaction.
pub fn record_review(
    connection: &mut Connection,
    vocabulary_entry_id: i32,
    grade: Grade,
    reviewed_at: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    let transaction = connection
        .transaction()
        .wrap_err("failed to start the review transaction")?;
    let previous = previous_schedule(&transaction, vocabulary_entry_id)?;
    let scheduled = review::schedule(
        previous.map(|schedule| schedule.state),
        previous.map(|schedule| schedule.last_reviewed_at),
        grade,
        reviewed_at,
    )?;

    let reviewed_at = format_timestamp(reviewed_at);
    let due_at = format_timestamp(scheduled.due_at);

    transaction
        .execute(
            INSERT_EVENT,
            params![vocabulary_entry_id, reviewed_at, grade.as_str()],
        )
        .wrap_err_with(|| {
            format!("failed to save the review event for entry {vocabulary_entry_id}")
        })?;
    transaction
        .execute(
            UPSERT_SCHEDULE,
            params![
                vocabulary_entry_id,
                due_at,
                scheduled.state.stability,
                scheduled.state.difficulty,
                reviewed_at,
            ],
        )
        .wrap_err_with(|| {
            format!("failed to save the review schedule for entry {vocabulary_entry_id}")
        })?;

    transaction
        .commit()
        .wrap_err("failed to commit the review transaction")?;

    Ok(scheduled.due_at)
}

#[derive(Debug, Clone, Copy)]
struct StoredSchedule {
    state: ReviewState,
    last_reviewed_at: DateTime<Utc>,
}

fn previous_schedule(
    connection: &Connection,
    vocabulary_entry_id: i32,
) -> Result<Option<StoredSchedule>> {
    let stored = connection
        .query_row(SELECT_SCHEDULE, [vocabulary_entry_id], |row| {
            Ok((
                row.get::<_, f64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .optional()
        .wrap_err_with(|| {
            format!("failed to read the review schedule for entry {vocabulary_entry_id}")
        })?;

    stored
        .map(|(stability, difficulty, last_reviewed_at)| {
            Ok(StoredSchedule {
                state: ReviewState {
                    stability,
                    difficulty,
                },
                last_reviewed_at: parse_timestamp(&last_reviewed_at)?,
            })
        })
        .transpose()
}

fn format_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .wrap_err_with(|| format!("invalid review timestamp {value:?}"))
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::storage::connect_in_memory;
    use crate::storage::vocabulary::{self, NewVocabularyEntry, PartOfSpeech};

    fn timestamp(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn seed_vocabulary(connection: &mut Connection, count: usize) -> Result<Vec<i32>> {
        let entries = (1..=count)
            .map(|index| NewVocabularyEntry {
                word_study: format!("word{index}"),
                word_translation: format!("translation{index}"),
                part_of_speech: PartOfSpeech::Noun,
                gender: None,
            })
            .collect::<Vec<_>>();
        vocabulary::insert_many(connection, &entries)?;

        let mut statement =
            connection.prepare("SELECT id FROM vocabulary_entries ORDER BY id ASC")?;
        let ids = statement
            .query_map([], |row| row.get::<_, i32>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(ids)
    }

    fn insert_schedule(
        connection: &Connection,
        vocabulary_entry_id: i32,
        due_at: DateTime<Utc>,
        last_reviewed_at: DateTime<Utc>,
    ) -> Result<()> {
        connection.execute(
            "INSERT INTO review_schedule \
             (vocabulary_entry_id, due_at, stability, difficulty, last_reviewed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                vocabulary_entry_id,
                format_timestamp(due_at),
                1.0_f64,
                5.0_f64,
                format_timestamp(last_reviewed_at),
            ],
        )?;

        Ok(())
    }

    fn row_count(connection: &Connection, table: &str) -> Result<i64> {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .wrap_err_with(|| format!("failed to count rows in {table}"))
    }

    #[test]
    fn due_boundary_excludes_entries_before_their_due_time() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let now = timestamp("2026-01-01T12:00:00Z");
        let ids = seed_vocabulary(&mut connection, 1)?;
        insert_schedule(
            &connection,
            ids[0],
            now + Duration::hours(1),
            now - Duration::days(1),
        )?;

        assert!(select_study_entry_ids(&connection, now, SESSION_SIZE)?.is_empty());
        assert_eq!(
            select_study_entry_ids(&connection, now + Duration::hours(1), SESSION_SIZE)?,
            ids
        );

        Ok(())
    }

    #[test]
    fn due_entries_come_first_and_a_session_has_at_most_five_entries() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let now = timestamp("2026-01-01T12:00:00Z");
        let ids = seed_vocabulary(&mut connection, 8)?;
        insert_schedule(
            &connection,
            ids[7],
            now - Duration::hours(2),
            now - Duration::days(1),
        )?;
        insert_schedule(
            &connection,
            ids[6],
            now - Duration::hours(1),
            now - Duration::days(1),
        )?;

        let selected = select_study_entry_ids(&connection, now, SESSION_SIZE)?;

        assert_eq!(selected, vec![ids[7], ids[6], ids[0], ids[1], ids[2]]);
        assert_eq!(selected.len(), SESSION_SIZE);

        Ok(())
    }

    #[test]
    fn both_grades_are_persisted_and_the_saved_schedule_is_reused() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let now = timestamp("2026-01-01T12:00:00Z");
        let ids = seed_vocabulary(&mut connection, 2)?;

        record_review(&mut connection, ids[0], Grade::Good, now)?;
        record_review(&mut connection, ids[1], Grade::Again, now)?;

        let events = {
            let mut statement = connection.prepare(
                "SELECT vocabulary_entry_id, reviewed_at, grade FROM review_events ORDER BY id ASC",
            )?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i32>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        assert_eq!(
            events,
            vec![
                (ids[0], format_timestamp(now), "good".to_owned()),
                (ids[1], format_timestamp(now), "again".to_owned()),
            ]
        );

        let schedules = {
            let mut statement =
                connection.prepare("SELECT vocabulary_entry_id, due_at FROM review_schedule")?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        assert_eq!(schedules.len(), 2);
        for (_, due_at) in schedules {
            assert!(parse_timestamp(&due_at)? > now);
        }

        assert!(select_study_entry_ids(&connection, now, SESSION_SIZE)?.is_empty());

        Ok(())
    }

    #[test]
    fn a_second_review_updates_the_existing_schedule_row() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let now = timestamp("2026-01-01T12:00:00Z");
        let ids = seed_vocabulary(&mut connection, 1)?;

        record_review(&mut connection, ids[0], Grade::Good, now)?;
        let later = now + Duration::days(1);
        record_review(&mut connection, ids[0], Grade::Again, later)?;

        assert_eq!(row_count(&connection, "review_events")?, 2);
        assert_eq!(row_count(&connection, "review_schedule")?, 1);

        let last_reviewed_at: String = connection.query_row(
            "SELECT last_reviewed_at FROM review_schedule WHERE vocabulary_entry_id = ?1",
            [ids[0]],
            |row| row.get(0),
        )?;
        assert_eq!(last_reviewed_at, format_timestamp(later));

        Ok(())
    }

    #[test]
    fn failed_schedule_write_rolls_back_the_review_event() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let now = timestamp("2026-01-01T12:00:00Z");
        let ids = seed_vocabulary(&mut connection, 1)?;
        connection.execute_batch(
            "CREATE TRIGGER fail_schedule \
             BEFORE INSERT ON review_schedule \
             BEGIN SELECT RAISE(ABORT, 'forced schedule failure'); END;",
        )?;

        let error = record_review(&mut connection, ids[0], Grade::Good, now).unwrap_err();

        assert!(
            error
                .chain()
                .any(|cause| cause.to_string().contains("forced schedule failure")),
            "unexpected error: {error:?}"
        );
        assert_eq!(row_count(&connection, "review_events")?, 0);
        assert_eq!(row_count(&connection, "review_schedule")?, 0);

        Ok(())
    }
}
