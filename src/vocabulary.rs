//! Vocabulary concepts shared by import, study, and storage.
use chrono::{DateTime, Utc};

/// Grammatical category of a vocabulary entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartOfSpeech {
    Noun,
    Verb,
    Adjective,
    Adverb,
    Pronoun,
    Article,
    Preposition,
    Conjunction,
    Numeral,
    Particle,
    Interjection,
}

impl PartOfSpeech {
    /// Returns the value stored in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Noun => "noun",
            Self::Verb => "verb",
            Self::Adjective => "adjective",
            Self::Adverb => "adverb",
            Self::Pronoun => "pronoun",
            Self::Article => "article",
            Self::Preposition => "preposition",
            Self::Conjunction => "conjunction",
            Self::Numeral => "numeral",
            Self::Particle => "particle",
            Self::Interjection => "interjection",
        }
    }
    /// Parses a part-of-speech label, ignoring case.
    pub fn from_label(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "noun" => Some(Self::Noun),
            "verb" => Some(Self::Verb),
            "adjective" => Some(Self::Adjective),
            "adverb" => Some(Self::Adverb),
            "pronoun" => Some(Self::Pronoun),
            "article" => Some(Self::Article),
            "preposition" => Some(Self::Preposition),
            "conjunction" => Some(Self::Conjunction),
            "numeral" => Some(Self::Numeral),
            "particle" => Some(Self::Particle),
            "interjection" => Some(Self::Interjection),
            _ => None,
        }
    }
}

/// Grammatical gender of a vocabulary entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Male,
    Female,
    Neutral,
}

impl Gender {
    /// Returns the value stored in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Male => "Male",
            Self::Female => "Female",
            Self::Neutral => "Neutral",
        }
    }
    /// Returns the German definite article for this gender.
    pub const fn german_article(self) -> &'static str {
        match self {
            Self::Male => "der",
            Self::Female => "die",
            Self::Neutral => "das",
        }
    }
    /// Parses a gender label from a CSV file, ignoring case.
    pub fn from_csv_label(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "masculine" | "male" => Some(Self::Male),
            "feminine" | "female" => Some(Self::Female),
            "neuter" | "neutral" => Some(Self::Neutral),
            _ => None,
        }
    }
}

/// A vocabulary entry that has not been persisted yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewVocabularyEntry {
    pub word_study: String,
    pub word_translation: String,
    pub part_of_speech: PartOfSpeech,
    pub gender: Option<Gender>,
}

/// A vocabulary entry stored in the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabularyEntry {
    pub id: i32,
    pub word_study: String,
    pub word_translation: String,
    pub part_of_speech: PartOfSpeech,
    pub gender: Option<Gender>,
    pub updated_at: DateTime<Utc>,
}
