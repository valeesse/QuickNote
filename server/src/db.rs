use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

#[derive(Clone)]
pub struct DbPool {
    pool: PgPool,
}

impl DbPool {
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        sqlx::query(include_str!("../migrations/001_initial.sql"))
            .execute(&self.pool)
            .await?;
        sqlx::query(include_str!("../migrations/002_sync.sql"))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub fn inner(&self) -> &PgPool {
        &self.pool
    }
}
