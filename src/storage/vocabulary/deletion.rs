use eyre::{Context, Result};
use rusqlite::Connection;

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
