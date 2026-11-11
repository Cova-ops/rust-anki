use super::TranscriptUpdate;

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
