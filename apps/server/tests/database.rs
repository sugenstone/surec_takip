use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use platform_server::{
    migrations::{self, MIGRATOR},
    router,
};
use sqlx::PgPool;
use tower::ServiceExt;

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
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 1);
    let response = router(pool)
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
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'citext')")
            .fetch_one(&pool)
            .await?;
    assert!(!exists);
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
    sqlx::query("CREATE TABLE migration_safety_probe (email citext)")
        .execute(&pool)
        .await?;
    assert!(migrations::revert_last(&pool).await.is_err());
    migrations::verify(&pool).await?;
    let _: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_safety_probe")
        .fetch_one(&pool)
        .await?;
    Ok(())
}
