use std::time::Duration;

use avspeechsynthesizer::prelude::{
    SpeechBoundary, SpeechSynthesisVoice, SpeechSynthesizer, SpeechUtterance,
};
use eyre::{Result, WrapErr, bail};

pub const PROMPT_LOCALE: &str = "es-MX";
pub const FEEDBACK_LOCALE: &str = "de-DE";

pub trait Synthesizer {
    fn speak_and_wait(&mut self, text: &str, locale: &str) -> Result<()>;
    fn stop(&mut self);
}

pub struct MacSynthesizer {
    inner: SpeechSynthesizer,
}

impl MacSynthesizer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: SpeechSynthesizer::new().wrap_err("failed to create speech synthesizer")?,
        })
    }
}

impl Synthesizer for MacSynthesizer {
    fn speak_and_wait(&mut self, text: &str, locale: &str) -> Result<()> {
        let voice = SpeechSynthesisVoice::voice_with_language(locale)
            .wrap_err_with(|| format!("failed to find a {locale} voice"))?
            .ok_or_else(|| eyre::eyre!("no {locale} voice is installed"))?;
        let utterance = SpeechUtterance::new(text).with_voice(voice);
        self.inner
            .speak(&utterance)
            .wrap_err("failed to start speech playback")?;

        // The framework delivers playback on the owning thread's run loop.
        let mut started = false;
        for _ in 0..600 {
            self.inner.pump_run_loop(Duration::from_millis(50));
            started |= self.inner.is_speaking();
            if started && !self.inner.is_speaking() {
                return Ok(());
            }
        }
        self.stop();
        bail!("speech playback did not finish")
    }

    fn stop(&mut self) {
        let _ = self.inner.stop_speaking(SpeechBoundary::Immediate);
    }
}
