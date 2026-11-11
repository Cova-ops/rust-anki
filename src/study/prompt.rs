use std::io::Write;

use super::StudyEntry;

pub(super) fn write_prompt<W: Write>(output: &mut W, entry: &StudyEntry) -> std::io::Result<()> {
    writeln!(
        output,
        "Please translate this word: {}",
        capitalize(&entry.word_translation)
    )?;
    output.flush()
}

pub(super) fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}
