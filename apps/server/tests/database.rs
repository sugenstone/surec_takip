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
    migrations::revert_last(&pool).await?;
    let organizations_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'organizations')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        !organizations_exists,
        "revert must remove the organizations migration"
    );
    let users_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'users')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        users_exists,
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
    // A dependent object on organizations must block rollback instead of
    // being dropped.
    sqlx::query(
        "CREATE TABLE migration_safety_probe (organization_id uuid REFERENCES organizations (id))",
    )
    .execute(&pool)
    .await?;
    assert!(migrations::revert_last(&pool).await.is_err());
    migrations::verify(&pool).await?;
    let _: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_safety_probe")
        .fetch_one(&pool)
        .await?;
    Ok(())
}
