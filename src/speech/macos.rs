use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use ::speech::live::LiveRecognition;
use ::speech::recognizer::SpeechRecognizer;
use eyre::{Result, WrapErr, bail, eyre};

use super::{Recognizer, TranscriptUpdate};

/// On-device macOS speech recognition adapter.
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
