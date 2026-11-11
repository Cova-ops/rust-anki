use std::collections::HashSet;

use eyre::{Context, Result};
use rusqlite::{Connection, params};

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub file: &'static str,
    pub sql: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

/// Applies every migration that has not been recorded yet.
pub fn run(connection: &mut Connection) -> Result<()> {
    let applied = applied_versions(connection)?;

    for migration in MIGRATIONS {
        if applied.contains(&migration.version) {
            continue;
        }

        apply(connection, migration)?;
    }

    Ok(())
}

fn applied_versions(connection: &Connection) -> Result<HashSet<i64>> {
    if !table_exists(connection, "schema_migrations")? {
        return Ok(HashSet::new());
    }

    let mut statement = connection
        .prepare("SELECT version FROM schema_migrations")
        .wrap_err("failed to prepare the applied migrations query")?;

    statement
        .query_map([], |row| row.get::<_, i64>(0))
        .wrap_err("failed to query applied migrations")?
        .collect::<std::result::Result<HashSet<_>, _>>()
        .wrap_err("failed to read applied migrations")
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )
        .wrap_err_with(|| format!("failed to check whether table {table} exists"))
}

fn apply(connection: &mut Connection, migration: &Migration) -> Result<()> {
    let transaction = connection
        .transaction()
        .wrap_err_with(|| format!("failed to start migration {}", migration.version))?;

    transaction.execute_batch(migration.sql).wrap_err_with(|| {
        format!(
            "failed to apply migration {} ({})",
            migration.version, migration.file
        )
    })?;
    transaction
        .execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            params![migration.version, migration.name],
        )
        .wrap_err_with(|| format!("failed to record migration {}", migration.version))?;

    transaction
        .commit()
        .wrap_err_with(|| format!("failed to commit migration {}", migration.version))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    #[test]
    fn applies_every_migration_to_an_empty_database() -> Result<()> {
        let mut connection = Connection::open_in_memory()?;

        run(&mut connection)?;

        let applied = {
            let mut statement = connection
                .prepare("SELECT version, name FROM schema_migrations ORDER BY version")?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        let expected = MIGRATIONS
            .iter()
            .map(|migration| (migration.version, migration.name.to_owned()))
            .collect::<Vec<_>>();

        assert_eq!(applied, expected);

        Ok(())
    }

    #[test]
    fn does_not_reapply_migrations() -> Result<()> {
        let mut connection = Connection::open_in_memory()?;

        run(&mut connection)?;
        run(&mut connection)?;

        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })?;

        assert_eq!(count as usize, MIGRATIONS.len());

        Ok(())
    }

    #[test]
    fn embeds_every_migration_file() {
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files = fs::read_dir(migrations_dir)
            .expect("migrations directory should be readable")
            .map(|entry| {
                entry
                    .expect("migration entry should be readable")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|file| file.ends_with(".sql"))
            .collect::<Vec<_>>();
        files.sort();

        let embedded = MIGRATIONS
            .iter()
            .map(|migration| migration.file.to_owned())
            .collect::<Vec<_>>();

        assert_eq!(embedded, files);
    }
}
