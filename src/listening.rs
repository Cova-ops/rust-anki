//! Answer-session evaluation over a recognition stream.
use crate::answer::{is_exit_command, is_next_command, matches_expected, normalize_tokens};
use crate::audio::{Clock, Recognizer, Transcript};
use eyre::{Result, bail};
use std::time::Duration;

pub const LISTEN_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenOutcome {
    Matched,
    TimedOut,
    Exited,
    Next,
}

pub fn listen<R: Recognizer + ?Sized, C: Clock + ?Sized>(
    recognizer: &mut R,
    clock: &C,
    expected: &str,
) -> Result<ListenOutcome> {
    listen_with_updates(recognizer, clock, expected, |_| Ok(()))
}

pub fn listen_with_updates<R, C, H>(
    recognizer: &mut R,
    clock: &C,
    expected: &str,
    mut on_update: H,
) -> Result<ListenOutcome>
where
    R: Recognizer + ?Sized,
    C: Clock + ?Sized,
    H: FnMut(&str) -> Result<()>,
{
    let expected_tokens = normalize_tokens(expected);
    if expected_tokens.is_empty() {
        recognizer.stop();
        bail!("invalid vocabulary entry: the expected answer has no words");
    }
    let mut transcript = Transcript::new();
    let outcome = (|| loop {
        let elapsed = clock.elapsed();
        if elapsed >= LISTEN_TIMEOUT {
            return Ok(ListenOutcome::TimedOut);
        }
        match recognizer.next_update(LISTEN_TIMEOUT - elapsed)? {
            Some(update) => {
                if clock.elapsed() >= LISTEN_TIMEOUT {
                    return Ok(ListenOutcome::TimedOut);
                }
                transcript.apply(&update);
                let text = transcript.text();
                if !text.is_empty() {
                    on_update(&text)?;
                }
                if is_exit_command(&update.transcript) {
                    return Ok(ListenOutcome::Exited);
                }
                if is_next_command(&update.transcript) {
                    return Ok(ListenOutcome::Next);
                }
                if matches_expected(&normalize_tokens(&text), &expected_tokens) {
                    return Ok(ListenOutcome::Matched);
                }
            }
            None => return Ok(ListenOutcome::TimedOut),
        }
    })();
    recognizer.stop();
    outcome
}
