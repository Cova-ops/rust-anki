use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use eyre::{Result, WrapErr, bail, eyre};

use ::speech::live::LiveRecognition;
use ::speech::recognizer::SpeechRecognizer;

/// German locale used for on-device recognition.
pub const ANSWER_LOCALE: &str = "de-DE";

/// Spoken command that ends the active study session.
pub const EXIT_COMMAND: &str = "exit";
pub const NEXT_COMMAND: &str = "next";

/// How long microphone capture stays open for one entry.
pub const LISTEN_TIMEOUT: Duration = Duration::from_secs(20);

/// One transcript update from the recognizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptUpdate {
    pub transcript: String,
    pub is_final: bool,
}

/// A live speech recognizer that can be started and stopped per entry.
pub trait Recognizer {
    /// Starts microphone capture for `locale`.
    fn start(&mut self, locale: &str) -> Result<()>;

    /// Waits up to `timeout` for the next transcript update.
    ///
    /// Returns `Ok(None)` when the timeout expires without an update.
    fn next_update(&mut self, timeout: Duration) -> Result<Option<TranscriptUpdate>>;

    /// Stops capture and releases the microphone.
    fn stop(&mut self);
}

/// Monotonic clock used to enforce the listening deadline.
pub trait Clock {
    /// Time elapsed since microphone capture began.
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

/// The result of listening for one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenOutcome {
    Matched,
    TimedOut,
    Exited,
    Next,
}

/// The three states of the on-device recognizer.
pub struct MacRecognizer {
    recognizer: Option<SpeechRecognizer>,
    session: Option<LiveRecognition>,
    updates: Option<Receiver<::speech::live::LiveUpdate>>,
}

impl MacRecognizer {
    pub fn new() -> Self {
        Self {
            recognizer: None,
            session: None,
            updates: None,
        }
    }
}

impl Default for MacRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Recognizer for MacRecognizer {
    fn start(&mut self, locale: &str) -> Result<()> {
        self.stop();
        ensure_authorization()?;

        let recognizer = SpeechRecognizer::with_locale(locale)
            .wrap_err_with(|| format!("failed to create a speech recognizer for {locale}"))?;
        if !recognizer.is_available() {
            bail!("speech recognition is unavailable for {locale}");
        }
        if !recognizer
            .supports_on_device_recognition()
            .wrap_err_with(|| format!("failed to check on-device recognition for {locale}"))?
        {
            bail!("on-device speech recognition is unavailable for {locale}");
        }

        let (sender, receiver) = mpsc::channel();
        let session = LiveRecognition::start(&recognizer, move |update| {
            let _ = sender.send(update);
        })
        .map_err(|error| eyre!("failed to start microphone capture: {error}"))?;

        self.recognizer = Some(recognizer);
        self.session = Some(session);
        self.updates = Some(receiver);
        Ok(())
    }

    fn next_update(&mut self, timeout: Duration) -> Result<Option<TranscriptUpdate>> {
        let updates = self
            .updates
            .as_ref()
            .ok_or_else(|| eyre!("microphone capture has not started"))?;

        match updates.recv_timeout(timeout) {
            Ok(update) => Ok(Some(TranscriptUpdate {
                transcript: update.transcript,
                is_final: update.is_final,
            })),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => {
                Err(eyre!("speech recognition stopped unexpectedly"))
            }
        }
    }

    fn stop(&mut self) {
        if let Some(session) = self.session.take() {
            session.cancel();
        }
        self.recognizer = None;
        self.updates = None;
    }
}

fn ensure_authorization() -> Result<()> {
    let status = SpeechRecognizer::authorization_status();
    if status.is_authorized() {
        return Ok(());
    }

    let status = SpeechRecognizer::request_authorization()
        .wrap_err("failed to request speech recognition permission")?;
    if !status.is_authorized() {
        bail!("speech recognition permission was not granted ({status:?})");
    }

    Ok(())
}

/// Accumulates finalized transcript segments and the current provisional text.
#[derive(Debug, Default)]
pub struct Transcript {
    finalized: Vec<String>,
    provisional: String,
}

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, update: &TranscriptUpdate) {
        if update.is_final {
            let transcript = update.transcript.trim();
            if !transcript.is_empty() {
                self.finalized.push(transcript.to_owned());
            }
            self.provisional.clear();
        } else {
            self.provisional = update.transcript.clone();
        }
    }

    pub fn text(&self) -> String {
        let mut text = self.finalized.join(" ");
        let provisional = self.provisional.trim();

        if !provisional.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(provisional);
        }

        text
    }
}

/// Normalizes text into lowercase tokens of letters and digits.
pub fn normalize_tokens(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Returns whether `tokens` contains `expected` as consecutive whole tokens.
pub fn matches_expected(tokens: &[String], expected: &[String]) -> bool {
    if expected.is_empty() || tokens.len() < expected.len() {
        return false;
    }

    tokens
        .windows(expected.len())
        .any(|window| window == expected)
}

/// Listens for the expected answer until it matches or the deadline expires.
///
/// Capture is always stopped before returning, including on errors.
pub fn listen<R, C>(recognizer: &mut R, clock: &C, expected: &str) -> Result<ListenOutcome>
where
    R: Recognizer + ?Sized,
    C: Clock + ?Sized,
{
    listen_with_updates(recognizer, clock, expected, |_| Ok(()))
}

/// Listens for the expected answer until it matches or the deadline expires,
/// reporting each current transcript to `on_update`.
///
/// Capture is always stopped before returning, including on errors.
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
    let outcome = (|| {
        loop {
            let elapsed = clock.elapsed();
            if elapsed >= LISTEN_TIMEOUT {
                return Ok(ListenOutcome::TimedOut);
            }

            match recognizer.next_update(LISTEN_TIMEOUT - elapsed)? {
                Some(update) => {
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
        }
    })();

    recognizer.stop();
    outcome
}

/// Returns whether a transcript contains the global spoken exit command as a
/// whole token.
pub fn is_exit_command(transcript: &str) -> bool {
    normalize_tokens(transcript)
        .iter()
        .any(|token| token == EXIT_COMMAND)
}

pub fn is_next_command(transcript: &str) -> bool {
    normalize_tokens(transcript)
        .iter()
        .any(|token| token == NEXT_COMMAND)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use super::*;

    #[derive(Clone, Debug)]
    struct FakeClock(Rc<Cell<Duration>>);

    impl FakeClock {
        fn new() -> Self {
            Self(Rc::new(Cell::new(Duration::ZERO)))
        }

        fn elapsed_duration(&self) -> Duration {
            self.0.get()
        }
    }

    impl Clock for FakeClock {
        fn elapsed(&self) -> Duration {
            self.elapsed_duration()
        }
    }

    struct FakeRecognizer {
        clock: FakeClock,
        updates: VecDeque<(Duration, TranscriptUpdate)>,
        stopped: bool,
    }

    impl FakeRecognizer {
        fn new(clock: FakeClock, updates: Vec<(Duration, &str, bool)>) -> Self {
            Self {
                clock,
                updates: updates
                    .into_iter()
                    .map(|(at, transcript, is_final)| {
                        (
                            at,
                            TranscriptUpdate {
                                transcript: transcript.to_owned(),
                                is_final,
                            },
                        )
                    })
                    .collect(),
                stopped: false,
            }
        }
    }

    impl Recognizer for FakeRecognizer {
        fn start(&mut self, _locale: &str) -> Result<()> {
            self.clock.0.set(Duration::ZERO);
            self.stopped = false;
            Ok(())
        }

        fn next_update(&mut self, timeout: Duration) -> Result<Option<TranscriptUpdate>> {
            let now = self.clock.0.get();

            match self.updates.front() {
                Some((at, _)) if *at <= now + timeout => {
                    let (at, update) = self.updates.pop_front().unwrap();
                    self.clock.0.set(at);
                    Ok(Some(update))
                }
                _ => {
                    self.clock.0.set(now + timeout);
                    Ok(None)
                }
            }
        }

        fn stop(&mut self) {
            self.stopped = true;
        }
    }

    fn timestamped(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    #[test]
    fn normalization_replaces_punctuation_and_preserves_accents_and_digits() {
        assert_eq!(normalize_tokens("¡PAN!"), vec!["pan".to_owned()]);
        assert_eq!(
            normalize_tokens("¿Buenos,días?"),
            vec!["buenos".to_owned(), "días".to_owned()]
        );
        assert_eq!(
            normalize_tokens("creo-que es (pan) 42."),
            vec![
                "creo".to_owned(),
                "que".to_owned(),
                "es".to_owned(),
                "pan".to_owned(),
                "42".to_owned(),
            ]
        );
    }

    #[test]
    fn matching_uses_whole_tokens_in_order() {
        assert!(matches_expected(
            &normalize_tokens("creo que es pan"),
            &normalize_tokens("pan")
        ));
        assert!(!matches_expected(
            &normalize_tokens("pantalla"),
            &normalize_tokens("pan")
        ));
        assert!(matches_expected(
            &normalize_tokens("¿Buenos,días?"),
            &normalize_tokens("buenos días")
        ));
        assert!(!matches_expected(
            &normalize_tokens("buenos y largos días"),
            &normalize_tokens("buenos días")
        ));
        assert!(!matches_expected(
            &normalize_tokens("dias"),
            &normalize_tokens("días")
        ));
    }

    #[test]
    fn exit_command_matches_a_whole_token_anywhere_in_the_update() {
        assert!(is_exit_command("EXIT!"));
        assert!(is_exit_command("Exit Ex"));
        assert!(is_exit_command("Katze exit"));
        assert!(!is_exit_command("Exitoxid"));
        assert!(!is_exit_command("exiting"));
    }

    #[test]
    fn next_command_matches_a_whole_token_anywhere_in_the_update() {
        assert!(is_next_command("NEXT!"));
        assert!(is_next_command("Nächsten nächsten Next"));
        assert!(is_next_command("please next card"));
        assert!(!is_next_command("Nächsten nächsten"));
        assert!(!is_next_command("nexting"));
    }

    #[test]
    fn transcript_replaces_provisional_text_and_keeps_final_segments() {
        let mut transcript = Transcript::new();

        transcript.apply(&TranscriptUpdate {
            transcript: "buenos".to_owned(),
            is_final: false,
        });
        assert_eq!(transcript.text(), "buenos");

        transcript.apply(&TranscriptUpdate {
            transcript: "buenos dias".to_owned(),
            is_final: true,
        });
        assert_eq!(transcript.text(), "buenos dias");

        transcript.apply(&TranscriptUpdate {
            transcript: "muchas".to_owned(),
            is_final: false,
        });
        assert_eq!(transcript.text(), "buenos dias muchas");

        transcript.apply(&TranscriptUpdate {
            transcript: "muchas gracias".to_owned(),
            is_final: false,
        });
        assert_eq!(transcript.text(), "buenos dias muchas gracias");
    }

    #[test]
    fn a_match_at_second_four_stops_capture_immediately() -> Result<()> {
        let clock = FakeClock::new();
        let mut recognizer = FakeRecognizer::new(
            clock.clone(),
            vec![
                (timestamped(2), "creo que", false),
                (timestamped(4), "creo que es pan", false),
                (timestamped(6), "pan", false),
            ],
        );

        let outcome = listen(&mut recognizer, &clock, "pan")?;

        assert_eq!(outcome, ListenOutcome::Matched);
        assert_eq!(clock.elapsed_duration(), timestamped(4));
        assert!(recognizer.stopped);
        assert_eq!(recognizer.updates.len(), 1);

        Ok(())
    }

    #[test]
    fn listening_reports_the_current_transcript_as_it_changes() -> Result<()> {
        let clock = FakeClock::new();
        let mut recognizer = FakeRecognizer::new(
            clock.clone(),
            vec![
                (timestamped(1), "der", false),
                (timestamped(2), "der Hund", false),
            ],
        );
        let mut reported = Vec::new();

        let outcome = listen_with_updates(&mut recognizer, &clock, "der Hund", |transcript| {
            reported.push(transcript.to_owned());
            Ok(())
        })?;

        assert_eq!(outcome, ListenOutcome::Matched);
        assert_eq!(reported, ["der", "der Hund"]);
        Ok(())
    }

    #[test]
    fn silence_times_out_at_twenty_seconds() -> Result<()> {
        let clock = FakeClock::new();
        let mut recognizer = FakeRecognizer::new(clock.clone(), vec![]);

        let outcome = listen(&mut recognizer, &clock, "pan")?;

        assert_eq!(outcome, ListenOutcome::TimedOut);
        assert_eq!(clock.elapsed_duration(), LISTEN_TIMEOUT);
        assert!(recognizer.stopped);

        Ok(())
    }

    #[test]
    fn a_nonmatching_transcript_times_out_at_the_deadline() -> Result<()> {
        let clock = FakeClock::new();
        let mut recognizer =
            FakeRecognizer::new(clock.clone(), vec![(timestamped(5), "pantalla", false)]);

        let outcome = listen(&mut recognizer, &clock, "pan")?;

        assert_eq!(outcome, ListenOutcome::TimedOut);
        assert_eq!(clock.elapsed_duration(), LISTEN_TIMEOUT);

        Ok(())
    }

    #[test]
    fn an_empty_expected_answer_is_rejected_and_stops_capture() {
        let clock = FakeClock::new();
        let mut recognizer = FakeRecognizer::new(clock.clone(), vec![]);

        let error = listen(&mut recognizer, &clock, "¡!?").unwrap_err();

        assert!(error.to_string().contains("invalid vocabulary entry"));
        assert!(recognizer.stopped);
    }
}
