mod macos;
pub use crate::audio::{Clock, Recognizer, SystemClock, Transcript, TranscriptUpdate};
pub use crate::listening::{LISTEN_TIMEOUT, ListenOutcome, listen, listen_with_updates};
pub use macos::MacRecognizer;

/// German locale used for on-device recognition.
pub const ANSWER_LOCALE: &str = "de-DE";

pub use crate::answer::{
    EXIT_COMMAND, NEXT_COMMAND, is_exit_command, is_next_command, matches_expected,
    normalize_tokens,
};

#[cfg(test)]
mod tests {
    use eyre::Result;
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::time::Duration;

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
    fn matching_update_at_deadline_is_not_graded() -> Result<()> {
        let clock = FakeClock::new();
        let mut recognizer =
            FakeRecognizer::new(clock.clone(), vec![(LISTEN_TIMEOUT, "der Hund", false)]);
        let outcome = listen(&mut recognizer, &clock, "der Hund")?;
        assert_eq!(outcome, ListenOutcome::TimedOut);
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
