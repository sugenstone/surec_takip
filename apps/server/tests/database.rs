use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use platform_server::{
    AppState,
    config::AuthConfig,
    migrations::{self, MIGRATOR},
    router,
};
use sqlx::PgPool;
use tower::ServiceExt;

fn test_state(pool: PgPool) -> AppState {
    AppState::new(pool, AuthConfig::fast_for_tests())
        .unwrap_or_else(|message| panic!("{}", message))
}

#[sqlx::test(migrations = "../../migrations")]
async fn migrated_database_is_ready_and_up_is_idempotent(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    MIGRATOR.run(&pool).await?;
    migrations::verify(&pool).await?;
    let equal: bool =
        sqlx::query_scalar("SELECT 'User@Example.test'::citext = 'user@example.test'::citext")
            .fetch_one(&pool)
            .await?;
    assert!(equal);
    let expected = MIGRATOR
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .count();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, expected as i64);
    let response = router(test_state(pool))
        .oneshot(
            Request::builder()
                .uri("/api/v1/ready")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn migration_can_revert_and_reapply_on_disposable_database(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Newest first: 013 drops assignment columns/indexes/permission, then
    // 012's index, then 011's table. revert_last requires a fully-applied
    // history, so subsequent reverts go through undo() directly.
    migrations::revert_last(&pool).await?;
    let assignee_column: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'processes' AND column_name = 'assignee_user_id')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(!assignee_column, "revert must drop the 013 assignee column");
    let assign_permission: i64 =
        sqlx::query_scalar("SELECT count(*) FROM permissions WHERE key = 'processes:assign'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(assign_permission, 0, "revert must remove processes:assign");
    MIGRATOR.undo(&pool, 20260926100000).await?;
    let index_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'process_executions_completed_idx')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(!index_exists, "revert must drop the 012 supporting index");
    MIGRATOR.undo(&pool, 20260924100000).await?;
    let executions_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'process_executions')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        !executions_exists,
        "revert must remove the process_executions migration"
    );
    let execution_permissions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM permissions WHERE key LIKE 'process_executions:%'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        execution_permissions, 0,
        "revert must remove the execution permission catalog rows"
    );
    let scope_key: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_constraint WHERE conname = 'processes_scope_id_key'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(scope_key, 0, "revert must remove the composite FK target");
    let processes_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'processes')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        processes_exists,
        "revert of the newest migration must keep earlier migrations applied"
    );
    assert!(migrations::verify(&pool).await.is_err());
    MIGRATOR.run(&pool).await?;
    migrations::verify(&pool).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn changed_migration_checksum_is_rejected(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("UPDATE _sqlx_migrations SET checksum = decode('00', 'hex')")
        .execute(&pool)
        .await?;
    assert!(migrations::verify(&pool).await.is_err());
    assert!(MIGRATOR.run(&pool).await.is_err());
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn revert_preserves_dependent_data(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    // A dependent object on an earlier table must block rollback instead of
    // being dropped. 013 and 012 revert cleanly first; the dependent then
    // blocks 011's undo — the failure must reach undo(), not the pre-revert
    // history check.
    sqlx::query(
        "CREATE TABLE migration_safety_probe (execution_id uuid REFERENCES process_executions (id))",
    )
        .execute(&pool)
        .await?;
    migrations::revert_last(&pool).await?;
    assert!(MIGRATOR.undo(&pool, 20260926100000).await.is_ok());
    assert!(MIGRATOR.undo(&pool, 20260924100000).await.is_err());
    MIGRATOR.run(&pool).await?;
    migrations::verify(&pool).await?;
    let _: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_safety_probe")
        .fetch_one(&pool)
        .await?;
    Ok(())
}
