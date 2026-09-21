use sqlx::{PgPool, migrate::Migrator};

pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

pub async fn verify(pool: &PgPool) -> Result<(), &'static str> {
    let applied = sqlx::query_as::<_, (i64, bool, Vec<u8>)>(
        "SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| "MIGRATION_HISTORY_UNAVAILABLE")?;
    let expected: Vec<_> = MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .collect();
    if applied.len() != expected.len() {
        return Err("MIGRATION_VERSION_MISMATCH");
    }
    for ((version, success, checksum), migration) in applied.iter().zip(expected) {
        if *version != migration.version
            || !success
            || checksum.as_slice() != migration.checksum.as_ref()
        {
            return Err("MIGRATION_CHECKSUM_MISMATCH");
        }
    }
    Ok(())
}

pub async fn revert_last(pool: &PgPool) -> Result<(), &'static str> {
    verify(pool).await?;
    let versions: Vec<_> = MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| m.version)
        .collect();
    let target = versions.iter().rev().nth(1).copied().unwrap_or(0);
    MIGRATOR
        .undo(pool, target)
        .await
        .map_err(|_| "MIGRATION_REVERT_FAILED")
}
