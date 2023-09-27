use anyhow::{Context as _, Result};
use sqlx::{prelude::*, Pool};

#[cfg(feature = "postgres")]
pub type Db = sqlx::Postgres;

#[cfg(feature = "sqlite")]
pub type Db = sqlx::Sqlite;

#[cfg(feature = "postgres")]
pub type DatabasePoolOptions = sqlx::postgres::PgPoolOptions;

#[cfg(feature = "sqlite")]
pub type DatabasePoolOptions = sqlx::sqlite::SqlitePoolOptions;

#[derive(FromRow, Debug)]
pub struct FileRow {
    pub file_url: String,
    pub space_id: String,
    pub block_id: String,
    pub file_name: String,
}

impl FileRow {
    pub async fn query(pool: &Pool<Db>, name: &str) -> Result<Vec<FileRow>> {
        let name = format!("%{name}%");
        let files = sqlx::query_as("SELECT * FROM files WHERE file_name LIKE ?")
            .bind(name)
            .fetch_all(pool)
            .await
            .context("Failed to select files")?;
        Ok(files)
    }

    pub async fn find_one(pool: &Pool<Db>, file_name: &str) -> Result<FileRow> {
        let row = sqlx::query_as("SELECT * FROM files WHERE file_name = ?")
            .bind(&file_name)
            .fetch_one(pool)
            .await
            .context("Failed to get file")?;
        Ok(row)
    }

    pub async fn insert(&self, pool: &Pool<Db>) -> Result<()> {
        let _ = sqlx::query(
            r#"
        INSERT INTO files (file_url, space_id, block_id, file_name)
        VALUES ($1, $2, $3, $4)
        "#,
        )
        .bind(&self.file_url)
        .bind(&self.space_id)
        .bind(&self.block_id)
        .bind(&self.file_name)
        .execute(pool)
        .await
        .context("Failed to insert row")?;
        Ok(())
    }
}

pub async fn create_pool(host: &str) -> Result<Pool<Db>> {
    let pool = DatabasePoolOptions::new()
        .max_connections(5)
        .connect(&host)
        .await
        .with_context(|| format!("Failed to connect {host}"))?;
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("Failed to run migration")?;
    Ok(pool)
}
