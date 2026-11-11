use std::collections::HashSet;
use std::fmt;

use chrono::{DateTime, Utc};
use eyre::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Row, params};

const PART_OF_SPEECH_COLUMN: usize = 3;
const GENDER_COLUMN: usize = 4;
const UPDATED_AT_COLUMN: usize = 5;

const SELECT_ENTRY: &str = "SELECT id, word_study, word_translation, part_of_speech, gender, updated_at \
     FROM vocabulary_entries WHERE id = ?1";

const INSERT_ENTRY: &str = "INSERT INTO vocabulary_entries \
     (word_study, word_translation, part_of_speech, gender) \
     VALUES (?1, ?2, ?3, ?4)";

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

    fn from_db(value: String) -> rusqlite::Result<Self> {
        match value.as_str() {
            "noun" => Ok(Self::Noun),
            "verb" => Ok(Self::Verb),
            "adjective" => Ok(Self::Adjective),
            "adverb" => Ok(Self::Adverb),
            "pronoun" => Ok(Self::Pronoun),
            "article" => Ok(Self::Article),
            "preposition" => Ok(Self::Preposition),
            "conjunction" => Ok(Self::Conjunction),
            "numeral" => Ok(Self::Numeral),
            "particle" => Ok(Self::Particle),
            "interjection" => Ok(Self::Interjection),
            _ => Err(conversion_error(
                PART_OF_SPEECH_COLUMN,
                format!("invalid part_of_speech value {value:?}"),
            )),
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

    fn from_db(value: String) -> rusqlite::Result<Self> {
        match value.as_str() {
            "Male" => Ok(Self::Male),
            "Female" => Ok(Self::Female),
            "Neutral" => Ok(Self::Neutral),
            _ => Err(conversion_error(
                GENDER_COLUMN,
                format!("invalid gender value {value:?}"),
            )),
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

impl VocabularyEntry {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        let id: i32 = row.get("id")?;
        let word_study: String = row.get("word_study")?;
        let word_translation: String = row.get("word_translation")?;
        let part_of_speech = PartOfSpeech::from_db(row.get("part_of_speech")?)?;
        let gender = row
            .get::<_, Option<String>>("gender")?
            .map(Gender::from_db)
            .transpose()?;
        let updated_at = parse_updated_at(row.get("updated_at")?)?;

        Ok(Self {
            id,
            word_study,
            word_translation,
            part_of_speech,
            gender,
            updated_at,
        })
    }
}

/// Inserts every entry in a single transaction.
pub fn insert_many(connection: &mut Connection, entries: &[NewVocabularyEntry]) -> Result<()> {
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .wrap_err("failed to start the vocabulary insert transaction")?;

    {
        let mut existing = transaction.prepare("SELECT word_study FROM vocabulary_entries")?;
        let mut words = existing
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|word| word.map(|word| word.to_lowercase()))
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        for entry in entries {
            eyre::ensure!(
                words.insert(entry.word_study.to_lowercase()),
                "duplicate word_study {:?}; the entire import was rejected",
                entry.word_study
            );
        }
        let mut statement = transaction
            .prepare(INSERT_ENTRY)
            .wrap_err("failed to prepare the vocabulary insert")?;

        for entry in entries {
            statement
                .execute(params![
                    &entry.word_study,
                    &entry.word_translation,
                    entry.part_of_speech.as_str(),
                    entry.gender.map(Gender::as_str),
                ])
                .wrap_err_with(|| format!("failed to insert word {:?}", entry.word_study))?;
        }
    }

    transaction
        .commit()
        .wrap_err("failed to commit the vocabulary inserts")
}

/// Fetches the vocabulary entry with `id`, if it exists.
pub fn find_by_id(connection: &Connection, id: i32) -> Result<Option<VocabularyEntry>> {
    connection
        .query_row(SELECT_ENTRY, [id], VocabularyEntry::from_row)
        .optional()
        .wrap_err_with(|| format!("failed to fetch vocabulary entry {id}"))
}

/// Checks whether a study word exists, ignoring case.
pub fn word_study_exists(connection: &Connection, word: &str) -> Result<bool> {
    let mut statement = connection.prepare("SELECT word_study FROM vocabulary_entries")?;
    let mut rows = statement.query([])?;
    let target = word.to_lowercase();
    while let Some(row) = rows.next()? {
        let existing: String = row.get(0)?;
        if existing.to_lowercase() == target {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Fetches an entry by its case-insensitive study word.
pub fn find_by_word(connection: &Connection, word: &str) -> Result<Option<VocabularyEntry>> {
    connection.create_collation("unicode_lowercase", |a, b| {
        a.to_lowercase().cmp(&b.to_lowercase())
    })?;
    let mut statement = connection.prepare(
        "SELECT id, word_study, word_translation, part_of_speech, gender, updated_at FROM vocabulary_entries WHERE word_study = ?1 COLLATE unicode_lowercase LIMIT 2",
    )?;
    let mut rows = statement.query([word])?;
    let matched = rows.next()?.map(VocabularyEntry::from_row).transpose()?;
    eyre::ensure!(
        rows.next()?.is_none(),
        "multiple vocabulary entries match {word:?} ignoring case; cannot select one safely"
    );
    Ok(matched)
}

/// Updates editable fields without changing review history or scheduling.
pub fn update_editable(connection: &Connection, entry: &VocabularyEntry) -> Result<()> {
    let changed = connection.execute(
        "UPDATE vocabulary_entries SET word_translation = ?1, part_of_speech = ?2, gender = ?3, updated_at = ?4 WHERE id = ?5",
        params![entry.word_translation, entry.part_of_speech.as_str(), entry.gender.map(Gender::as_str), Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true), entry.id],
    ).wrap_err("failed to update vocabulary entry")?;
    eyre::ensure!(changed == 1, "vocabulary entry no longer exists");
    Ok(())
}

/// Deletes a vocabulary entry and its review data atomically.
pub fn delete_with_reviews(connection: &mut Connection, id: i32) -> Result<()> {
    let transaction = connection
        .transaction()
        .wrap_err("failed to start deletion transaction")?;
    transaction.execute(
        "DELETE FROM review_events WHERE vocabulary_entry_id = ?1",
        [id],
    )?;
    transaction.execute(
        "DELETE FROM review_schedule WHERE vocabulary_entry_id = ?1",
        [id],
    )?;
    let count = transaction.execute("DELETE FROM vocabulary_entries WHERE id = ?1", [id])?;
    eyre::ensure!(count == 1, "vocabulary entry no longer exists");
    transaction
        .commit()
        .wrap_err("failed to commit vocabulary deletion")
}

fn parse_updated_at(value: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| {
            conversion_error(
                UPDATED_AT_COLUMN,
                format!("invalid updated_at value {value:?}: {error}"),
            )
        })
}

#[derive(Debug)]
struct ConversionError(String);

impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ConversionError {}

fn conversion_error(column: usize, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(ConversionError(message)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::connect_in_memory;

    fn new_entry(
        word_study: &str,
        word_translation: &str,
        part_of_speech: PartOfSpeech,
        gender: Option<Gender>,
    ) -> NewVocabularyEntry {
        NewVocabularyEntry {
            word_study: word_study.to_owned(),
            word_translation: word_translation.to_owned(),
            part_of_speech,
            gender,
        }
    }

    #[test]
    fn inserts_many_and_fetches_by_id() -> Result<()> {
        let mut connection = connect_in_memory()?;
        let entries = [
            new_entry(
                "der Hund",
                "the dog",
                PartOfSpeech::Noun,
                Some(Gender::Male),
            ),
            new_entry("laufen", "to run", PartOfSpeech::Verb, None),
        ];

        insert_many(&mut connection, &entries)?;

        let first = find_by_id(&connection, 1)?.expect("first entry should exist");
        assert_eq!(first.word_study, "der Hund");
        assert_eq!(first.word_translation, "the dog");
        assert_eq!(first.part_of_speech, PartOfSpeech::Noun);
        assert_eq!(first.gender, Some(Gender::Male));

        let second = find_by_id(&connection, 2)?.expect("second entry should exist");
        assert_eq!(second.word_study, "laufen");
        assert_eq!(second.part_of_speech, PartOfSpeech::Verb);
        assert_eq!(second.gender, None);

        Ok(())
    }

    #[test]
    fn gender_articles_match_german_grammar() {
        assert_eq!(Gender::Male.german_article(), "der");
        assert_eq!(Gender::Female.german_article(), "die");
        assert_eq!(Gender::Neutral.german_article(), "das");
    }

    #[test]
    fn returns_none_for_unknown_id() -> Result<()> {
        let connection = connect_in_memory()?;

        assert_eq!(find_by_id(&connection, 42)?, None);

        Ok(())
    }

    #[test]
    fn migration_creates_vocabulary_entries_table() -> Result<()> {
        let connection = connect_in_memory()?;

        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'vocabulary_entries'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1);

        Ok(())
    }
}
