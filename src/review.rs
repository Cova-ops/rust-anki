use chrono::{DateTime, Duration, Utc};
use eyre::{Context, Result};
use fsrs::{FSRS, MemoryState};

/// Desired retention used by the FSRS scheduler.
const DESIRED_RETENTION: f32 = 0.95;

/// A grade produced by the study flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Again,
    Good,
}

impl Grade {
    /// Returns the value stored in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Again => "again",
            Self::Good => "good",
        }
    }

    /// Parses a grade stored in the database.
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "again" => Some(Self::Again),
            "good" => Some(Self::Good),
            _ => None,
        }
    }
}

/// The FSRS memory state saved for a vocabulary entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReviewState {
    pub stability: f64,
    pub difficulty: f64,
}

/// The result of scheduling a graded review.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScheduledReview {
    pub state: ReviewState,
    pub due_at: DateTime<Utc>,
}

/// Calculates the next memory state and due time for a graded review.
pub fn schedule(
    previous: Option<ReviewState>,
    last_reviewed_at: Option<DateTime<Utc>>,
    grade: Grade,
    reviewed_at: DateTime<Utc>,
) -> Result<ScheduledReview> {
    let fsrs = FSRS::default();
    let elapsed_days = match (previous, last_reviewed_at) {
        (Some(_), Some(last_reviewed_at)) => {
            (reviewed_at - last_reviewed_at).num_days().max(0) as u32
        }
        _ => 0,
    };
    let previous = previous.map(|state| MemoryState {
        stability: state.stability as f32,
        difficulty: state.difficulty as f32,
    });

    let next_states = fsrs
        .next_states(previous, DESIRED_RETENTION, elapsed_days)
        .wrap_err("failed to calculate the next review state")?;
    let next = match grade {
        Grade::Again => next_states.again,
        Grade::Good => next_states.good,
    };

    let interval_days = next.interval.round().max(1.0) as i64;
    let due_at = reviewed_at + Duration::days(interval_days);

    Ok(ScheduledReview {
        state: ReviewState {
            stability: f64::from(next.memory.stability),
            difficulty: f64::from(next.memory.difficulty),
        },
        due_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn first_good_review_schedules_the_entry_in_the_future() -> Result<()> {
        let now = timestamp("2026-01-01T12:00:00Z");

        let scheduled = schedule(None, None, Grade::Good, now)?;

        assert!(scheduled.state.stability > 0.0);
        assert!(scheduled.state.difficulty > 0.0);
        assert!(scheduled.due_at > now);

        Ok(())
    }

    #[test]
    fn previous_state_and_elapsed_days_are_used() -> Result<()> {
        let first = timestamp("2026-01-01T12:00:00Z");
        let previous = schedule(None, None, Grade::Good, first)?;
        let second = first + Duration::days(3);

        let scheduled = schedule(Some(previous.state), Some(first), Grade::Good, second)?;

        assert!(scheduled.state.stability > previous.state.stability);
        assert!(scheduled.due_at > second);

        Ok(())
    }

    #[test]
    fn grades_round_trip_through_the_database_values() {
        assert_eq!(Grade::Again.as_str(), "again");
        assert_eq!(Grade::Good.as_str(), "good");
        assert_eq!(Grade::from_db("again"), Some(Grade::Again));
        assert_eq!(Grade::from_db("good"), Some(Grade::Good));
        assert_eq!(Grade::from_db("easy"), None);
    }
}
