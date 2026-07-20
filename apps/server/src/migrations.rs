use sqlx::PgPool;

const MIGRATIONS: &[(&str, &str)] = &[
    ("001_initial", include_str!("../migrations/001_initial.sql")),
    ("002_sync", include_str!("../migrations/002_sync.sql")),
    ("003_billing", include_str!("../migrations/003_billing.sql")),
    (
        "004_audit_and_access",
        include_str!("../migrations/004_audit_and_access.sql"),
    ),
    (
        "005_free_history_7_days",
        include_str!("../migrations/005_free_history_7_days.sql"),
    ),
    (
        "006_yjs_collaboration",
        include_str!("../migrations/006_yjs_collaboration.sql"),
    ),
    ("007_tags", include_str!("../migrations/007_tags.sql")),
    (
        "008_search_projection",
        include_str!("../migrations/008_search_projection.sql"),
    ),
    (
        "009_yjs_bootstrap_lease",
        include_str!("../migrations/009_yjs_bootstrap_lease.sql"),
    ),
    (
        "010_yjs_delivery_ack",
        include_str!("../migrations/010_yjs_delivery_ack.sql"),
    ),
];

pub async fn run(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(731945120)")
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS quicknote_schema_migrations (
         version TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
    )
    .execute(&mut *transaction)
    .await?;
    for (version, source) in MIGRATIONS {
        let applied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM quicknote_schema_migrations WHERE version=$1)",
        )
        .bind(version)
        .fetch_one(&mut *transaction)
        .await?;
        if applied {
            continue;
        }
        sqlx::raw_sql(source).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO quicknote_schema_migrations(version) VALUES ($1)")
            .bind(version)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await
}
