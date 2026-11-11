use std::io::Write;

use chrono::{DateTime, Local, Utc};
use eyre::{Context, Result};

use crate::audio::{Clock, Recognizer};
use crate::listening::{self, ListenOutcome};
use crate::review::Grade;
use crate::synthesis::{self, Synthesizer};
use crate::{answer, speech};

mod prompt;
#[cfg(test)]
use prompt::capitalize;
use prompt::write_prompt;

/// A vocabulary entry presented during a study session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudyEntry {
    pub id: i32,
    pub word_study: String,
    pub word_translation: String,
    pub article: Option<String>,
}

/// Runs a speech-based study session over the provided entries.
///
/// Each entry is presented at most once. The callback receives every graded
/// answer and must persist it before the session advances.
pub fn run<R, C, F, G>(
    entries: &[StudyEntry],
    output: &mut impl Write,
    recognizer: &mut R,
    clock: G,
    mut on_grade: F,
) -> Result<()>
where
    R: Recognizer + ?Sized,
    C: Clock,
    F: FnMut(i32, Grade) -> Result<()>,
    G: FnMut() -> C,
{
    let mut silent_synthesizer = SilentSynthesizer;
    run_with_synthesizer(
        entries,
        output,
        recognizer,
        &mut silent_synthesizer,
        clock,
        |entry_id, grade| {
            on_grade(entry_id, grade)?;
            Ok(Utc::now())
        },
    )
}

/// Runs a study session with audible prompts and corrective feedback.
pub fn run_with_synthesizer<R, S, C, F, G>(
    entries: &[StudyEntry],
    output: &mut impl Write,
    recognizer: &mut R,
    synthesizer: &mut S,
    mut clock: G,
    mut on_grade: F,
) -> Result<()>
where
    R: Recognizer + ?Sized,
    S: Synthesizer + ?Sized,
    C: Clock,
    F: FnMut(i32, Grade) -> Result<DateTime<Utc>>,
    G: FnMut() -> C,
{
    'entries: for entry in entries {
        let expected = expected_answer(entry);
        if answer::normalize_tokens(&expected).is_empty() {
            writeln!(
                output,
                "Skipping invalid vocabulary entry: the expected answer has no words."
            )?;
            continue;
        }

        write_prompt(output, entry)?;
        synthesizer
            .speak_and_wait(&entry.word_translation, synthesis::PROMPT_LOCALE)
            .wrap_err("failed to play the Spanish prompt")?;
        recognizer
            .start(speech::ANSWER_LOCALE)
            .wrap_err("failed to start speech recognition")?;
        let entry_clock = clock();
        if let Err(error) = writeln!(output, "Listening...").and_then(|()| output.flush()) {
            recognizer.stop();
            return Err(error.into());
        }
        match listening::listen_with_updates(recognizer, &entry_clock, &expected, |transcript| {
            writeln!(output, "Heard: {transcript}")?;
            output.flush()?;
            Ok(())
        })? {
            ListenOutcome::Matched => {
                let due_at = on_grade(entry.id, Grade::Good)?;
                writeln!(output, "Correct!")?;
                writeln!(
                    output,
                    "Next review: {}",
                    due_at.with_timezone(&Local).format("%Y-%m-%d")
                )?;
            }
            ListenOutcome::TimedOut | ListenOutcome::Next => {
                on_grade(entry.id, Grade::Again)?;
                writeln!(output, "Incorrect. Expected answer: {expected}")?;
                synthesizer
                    .speak_and_wait(&expected, synthesis::FEEDBACK_LOCALE)
                    .wrap_err("failed to play the German correction")?;
            }
            ListenOutcome::Exited => break 'entries,
        }
    }

    writeln!(output, "That's all for today! :)")?;
    Ok(())
}

struct SilentSynthesizer;

impl Synthesizer for SilentSynthesizer {
    fn speak_and_wait(&mut self, _text: &str, _locale: &str) -> Result<()> {
        Ok(())
    }

    fn stop(&mut self) {}
}

fn expected_answer(entry: &StudyEntry) -> String {
    match &entry.article {
        Some(article) => format!("{article} {}", entry.word_study),
        None => entry.word_study.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::time::Duration;

    use eyre::eyre;

    use super::*;
    use crate::speech::TranscriptUpdate;

    #[derive(Clone, Debug)]
    struct FakeClock(Rc<Cell<Duration>>);

    impl Clock for FakeClock {
        fn elapsed(&self) -> Duration {
            self.0.get()
        }
    }

    #[derive(Debug)]
    struct FakeRecognizer {
        scripts: VecDeque<Vec<(Duration, &'static str, bool)>>,
        current: VecDeque<(Duration, TranscriptUpdate)>,
        clock: Rc<Cell<Duration>>,
        start_error: Option<&'static str>,
        stopped: bool,
    }

    impl FakeRecognizer {
        fn new(
            clock: Rc<Cell<Duration>>,
            scripts: Vec<Vec<(Duration, &'static str, bool)>>,
        ) -> Self {
            Self {
                scripts: scripts.into(),
                current: VecDeque::new(),
                clock,
                start_error: None,
                stopped: false,
            }
        }

        fn failing(clock: Rc<Cell<Duration>>, message: &'static str) -> Self {
            let mut recognizer = Self::new(clock, Vec::new());
            recognizer.start_error = Some(message);
            recognizer
        }
    }

    impl Recognizer for FakeRecognizer {
        fn start(&mut self, _locale: &str) -> Result<()> {
            if let Some(message) = self.start_error {
                return Err(eyre!(message));
            }

            self.clock.set(Duration::ZERO);
            self.current = self
                .scripts
                .pop_front()
                .unwrap_or_default()
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
                .collect();
            self.stopped = false;
            Ok(())
        }

        fn next_update(&mut self, timeout: Duration) -> Result<Option<TranscriptUpdate>> {
            let now = self.clock.get();

            match self.current.front() {
                Some((at, _)) if *at <= now + timeout => {
                    let (at, update) = self.current.pop_front().unwrap();
                    self.clock.set(at);
                    Ok(Some(update))
                }
                _ => {
                    self.clock.set(now + timeout);
                    Ok(None)
                }
            }
        }

        fn stop(&mut self) {
            self.stopped = true;
        }
    }

    fn entry(
        id: i32,
        word_study: &str,
        word_translation: &str,
        article: Option<&str>,
    ) -> StudyEntry {
        StudyEntry {
            id,
            word_study: word_study.to_owned(),
            word_translation: word_translation.to_owned(),
            article: article.map(str::to_owned),
        }
    }

    fn seconds(value: u64) -> Duration {
        Duration::from_secs(value)
    }

    #[test]
    fn output_failure_after_capture_starts_stops_recognition() {
        struct FailingOutput(usize);
        impl Write for FailingOutput {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.0 += 1;
                if self.0 == 2 {
                    return Err(std::io::Error::other("output failure"));
                }
                Ok(())
            }
        }
        let clock = Rc::new(Cell::new(Duration::ZERO));
        let mut recognizer = FakeRecognizer::new(Rc::clone(&clock), vec![vec![]]);
        let entries = [entry(1, "Hund", "perro", Some("der"))];
        let mut grades = Vec::new();
        let result = run(
            &entries,
            &mut FailingOutput(0),
            &mut recognizer,
            move || FakeClock(Rc::clone(&clock)),
            |id, grade| {
                grades.push((id, grade));
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(recognizer.stopped);
        assert!(grades.is_empty());
    }

    #[test]
    fn matched_answers_are_graded_good_and_advance() -> Result<()> {
        let clock = Rc::new(Cell::new(Duration::ZERO));
        let mut recognizer = FakeRecognizer::new(
            Rc::clone(&clock),
            vec![
                vec![(seconds(4), "der Hund", false)],
                vec![(seconds(2), "die Katze", false)],
            ],
        );
        let entries = [
            entry(1, "Hund", "perro", Some("der")),
            entry(2, "Katze", "gato", Some("die")),
        ];
        let mut output = Vec::new();
        let mut grades = Vec::new();

        run(
            &entries,
            &mut output,
            &mut recognizer,
            {
                let clock = Rc::clone(&clock);
                move || FakeClock(Rc::clone(&clock))
            },
            |id, grade| {
                grades.push((id, grade));
                Ok(())
            },
        )?;

        assert_eq!(grades, vec![(1, Grade::Good), (2, Grade::Good)]);
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Please translate this word: Perro"));
        assert!(output.contains("Please translate this word: Gato"));
        assert!(output.contains("Listening..."));
        assert!(output.contains("Heard: der Hund"));
        assert!(output.contains("Heard: die Katze"));
        assert!(output.contains("Correct!"));
        Ok(())
    }

    #[test]
    fn a_timeout_is_graded_again_and_shows_the_expected_translation() -> Result<()> {
        let clock = Rc::new(Cell::new(Duration::ZERO));
        let mut recognizer = FakeRecognizer::new(Rc::clone(&clock), vec![vec![]]);
        let entries = [entry(1, "Hund", "perro", Some("der"))];
        let mut output = Vec::new();
        let mut grades = Vec::new();

        run(
            &entries,
            &mut output,
            &mut recognizer,
            {
                let clock = Rc::clone(&clock);
                move || FakeClock(Rc::clone(&clock))
            },
            |id, grade| {
                grades.push((id, grade));
                Ok(())
            },
        )?;

        assert_eq!(grades, vec![(1, Grade::Again)]);
        assert_eq!(clock.get(), speech::LISTEN_TIMEOUT);

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Incorrect. Expected answer: der Hund"));
        Ok(())
    }

    #[test]
    fn exit_ends_the_session_without_grading_the_current_entry() -> Result<()> {
        let clock = Rc::new(Cell::new(Duration::ZERO));
        let mut recognizer = FakeRecognizer::new(
            Rc::clone(&clock),
            vec![vec![(seconds(1), "exit", false)], vec![]],
        );
        let entries = [
            entry(1, "Hund", "perro", Some("der")),
            entry(2, "Katze", "gato", Some("die")),
        ];
        let mut output = Vec::new();
        let mut grades = Vec::new();

        run(
            &entries,
            &mut output,
            &mut recognizer,
            {
                let clock = Rc::clone(&clock);
                move || FakeClock(Rc::clone(&clock))
            },
            |id, grade| {
                grades.push((id, grade));
                Ok(())
            },
        )?;

        assert!(grades.is_empty());
        assert!(recognizer.stopped);
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Heard: exit"));
        assert!(output.contains("That's all for today! :)"));
        Ok(())
    }

    #[test]
    fn the_prompt_displays_the_capitalized_translation() -> Result<()> {
        let clock = Rc::new(Cell::new(Duration::ZERO));
        let mut recognizer = FakeRecognizer::new(Rc::clone(&clock), vec![vec![]]);
        let entries = [entry(1, "Hund", "perro", Some("der"))];
        let mut output = Vec::new();

        run(
            &entries,
            &mut output,
            &mut recognizer,
            move || FakeClock(Rc::clone(&clock)),
            |_, _| Ok(()),
        )?;

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Please translate this word: Perro"));
        assert!(!output.contains("Please translate this word: der Hund"));
        Ok(())
    }

    #[test]
    fn an_invalid_expected_answer_is_skipped_without_grading() -> Result<()> {
        let clock = Rc::new(Cell::new(Duration::ZERO));
        let mut recognizer = FakeRecognizer::new(Rc::clone(&clock), Vec::new());
        let entries = [entry(1, "¡!?", "perro", None)];
        let mut output = Vec::new();
        let mut grades = Vec::new();

        run(
            &entries,
            &mut output,
            &mut recognizer,
            move || FakeClock(Rc::clone(&clock)),
            |id, grade| {
                grades.push((id, grade));
                Ok(())
            },
        )?;

        assert!(grades.is_empty());
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Skipping invalid vocabulary entry"));
        Ok(())
    }

    #[test]
    fn a_recognition_error_leaves_the_current_entry_ungraded() -> Result<()> {
        let clock = Rc::new(Cell::new(Duration::ZERO));
        let mut recognizer = FakeRecognizer::failing(Rc::clone(&clock), "microphone unavailable");
        let entries = [entry(1, "Hund", "perro", Some("der"))];
        let mut output = Vec::new();
        let mut grades = Vec::new();

        let error = run(
            &entries,
            &mut output,
            &mut recognizer,
            move || FakeClock(Rc::clone(&clock)),
            |id, grade| {
                grades.push((id, grade));
                Ok(())
            },
        )
        .unwrap_err();

        assert!(
            error
                .chain()
                .any(|cause| cause.to_string().contains("microphone unavailable")),
            "unexpected error: {error:?}"
        );
        assert!(grades.is_empty());
        Ok(())
    }

    #[test]
    fn late_updates_cannot_grade_an_entry_twice() -> Result<()> {
        let clock = Rc::new(Cell::new(Duration::ZERO));
        let mut recognizer = FakeRecognizer::new(
            Rc::clone(&clock),
            vec![vec![
                (seconds(1), "der Hund", false),
                (seconds(3), "die Katze", false),
            ]],
        );
        let entries = [entry(1, "Hund", "perro", Some("der"))];
        let mut output = Vec::new();
        let mut grades = Vec::new();

        run(
            &entries,
            &mut output,
            &mut recognizer,
            move || FakeClock(Rc::clone(&clock)),
            |id, grade| {
                grades.push((id, grade));
                Ok(())
            },
        )?;

        assert_eq!(grades, vec![(1, Grade::Good)]);
        assert_eq!(recognizer.current.len(), 1);
        Ok(())
    }

    #[test]
    fn a_late_update_cannot_grade_the_next_entry() -> Result<()> {
        let clock = Rc::new(Cell::new(Duration::ZERO));
        let mut recognizer = FakeRecognizer::new(
            Rc::clone(&clock),
            vec![
                vec![
                    (seconds(1), "der Hund", false),
                    (seconds(3), "die Katze", false),
                ],
                vec![],
            ],
        );
        let entries = [
            entry(1, "Hund", "perro", Some("der")),
            entry(2, "Katze", "gato", Some("die")),
        ];
        let mut output = Vec::new();
        let mut grades = Vec::new();

        run(
            &entries,
            &mut output,
            &mut recognizer,
            move || FakeClock(Rc::clone(&clock)),
            |id, grade| {
                grades.push((id, grade));
                Ok(())
            },
        )?;

        assert_eq!(grades, vec![(1, Grade::Good), (2, Grade::Again)]);
        Ok(())
    }

    #[test]
    fn capitalize_uppercases_the_first_character_and_keeps_the_rest() {
        assert_eq!(capitalize("perro"), "Perro");
        assert_eq!(capitalize("él"), "Él");
        assert_eq!(capitalize("buenos días"), "Buenos días");
        assert_eq!(capitalize(""), "");
    }
}
