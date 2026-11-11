//! Pure transcript normalization, answer matching, and spoken commands.

pub const EXIT_COMMAND: &str = "exit";
pub const NEXT_COMMAND: &str = "next";

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

pub fn matches_expected(tokens: &[String], expected: &[String]) -> bool {
    !expected.is_empty()
        && tokens.len() >= expected.len()
        && tokens
            .windows(expected.len())
            .any(|window| window == expected)
}

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
