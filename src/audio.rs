//! Audio capture interfaces shared by session coordination and adapters.
use eyre::Result;
use std::time::{Duration, Instant};

mod transcript;
pub use transcript::Transcript;

/// One transcript update from the recognizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptUpdate {
    pub transcript: String,
    pub is_final: bool,
}

/// A live speech recognizer that can be started and stopped per entry.
pub trait Recognizer {
    fn start(&mut self, locale: &str) -> Result<()>;
    fn next_update(&mut self, timeout: Duration) -> Result<Option<TranscriptUpdate>>;
    fn stop(&mut self);
}

/// Monotonic clock used to enforce the listening deadline.
pub trait Clock {
    fn elapsed(&self) -> Duration;
}

/// Clock backed by the operating system's monotonic clock.
#[derive(Debug)]
pub struct SystemClock {
    started_at: Instant,
}
impl SystemClock {
    pub fn start() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}
impl Clock for SystemClock {
    fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}
