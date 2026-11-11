use std::path::Path;

use eyre::{Context, Result};
use rusqlite::Connection;

mod migrations;
pub mod reviews;
pub mod vocabulary;

pub use vocabulary::{Gender, NewVocabularyEntry, PartOfSpeech, VocabularyEntry};

/// Local SQLite database file, resolved against the current working directory.
pub const DB_FILE: &str = "rust_anki.db";

/// Opens a connection to the SQLite database at `path` and applies migrations.
pub fn connect(path: impl AsRef<Path>) -> Result<Connection> {
    let path = path.as_ref();

    let mut connection = Connection::open(path)
        .wrap_err_with(|| format!("failed to open the database at {}", path.display()))?;
    migrations::run(&mut connection)?;

    Ok(connection)
}

/// Opens the project's local database.
pub fn connect_default() -> Result<Connection> {
    connect(DB_FILE)
}

/// Opens an in-memory database for tests.
#[cfg(test)]
pub fn connect_in_memory() -> Result<Connection> {
    let mut connection =
        Connection::open_in_memory().wrap_err("failed to open an in-memory database")?;
    migrations::run(&mut connection)?;

    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_connection_is_usable() -> Result<()> {
        let connection = connect_in_memory()?;

        let value: i64 = connection.query_row("SELECT 1", [], |row| row.get(0))?;
        assert_eq!(value, 1);

        Ok(())
    }
}
