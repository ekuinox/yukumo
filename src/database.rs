use anyhow::{Context as _, Result};
use sqlx::{any::{AnyPoolOptions, install_default_drivers}, prelude::*, AnyPool};

#[derive(FromRow, Debug)]
pub struct FileRow {
    pub file_url: String,
    pub space_id: String,
    pub block_id: String,
    pub file_name: String,
}

impl FileRow {
    pub async fn query(pool: &AnyPool, name: &str) -> Result<Vec<FileRow>> {
        let name = format!("%{name}%");
        let files = sqlx::query_as("SELECT * FROM files WHERE file_name LIKE ?")
            .bind(name)
            .fetch_all(pool)
            .await
            .context("Failed to select files")?;
        Ok(files)
    }

    pub async fn find_one(pool: &AnyPool, file_name: &str) -> Result<FileRow> {
        let row = sqlx::query_as("SELECT * FROM files WHERE file_name = ?")
            .bind(&file_name)
            .fetch_one(pool)
            .await
            .context("Failed to get file")?;
        Ok(row)
    }

    pub async fn insert(&self, pool: &AnyPool) -> Result<()> {
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

pub async fn create_pool(host: &str) -> Result<AnyPool> {
    install_default_drivers();
    let pool = AnyPoolOptions::new()
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
