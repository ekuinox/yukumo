use anyhow::{Context as _, Result};
use chrono::NaiveDateTime;
use sqlx::{
    postgres::{PgPool, PgPoolOptions},
    prelude::*,
};

#[derive(FromRow, Debug)]
pub struct FileRow {
    /// ファイル名
    pub file_name: String,
    /// Notion のダウンロードするために必要な URL
    pub file_url: String,
    /// Notion のスペースの ID
    pub space_id: String,
    /// Notion のファイルが紐づいているブロックの ID
    pub block_id: String,
    /// 元ファイルの絶対パス
    pub origin_file_path: String,
    /// 作成日時
    pub created_at: NaiveDateTime,
}

impl FileRow {
    pub async fn query(pool: &PgPool, prefix: &str) -> Result<Vec<FileRow>> {
        let files = sqlx::query_as(r#"SELECT * FROM files WHERE starts_with(file_name, $1)"#)
            .bind(prefix)
            .fetch_all(pool)
            .await
            .context("Failed to select files")?;
        Ok(files)
    }

    pub async fn find_one(pool: &PgPool, file_name: &str) -> Result<FileRow> {
        let row = sqlx::query_as(r#"SELECT * FROM files WHERE file_name = $1"#)
            .bind(file_name)
            .fetch_one(pool)
            .await
            .context("Failed to get file")?;
        Ok(row)
    }

    pub async fn is_exists(pool: &PgPool, file_name: &str) -> Result<bool> {
        let (exists,): (bool,) =
            sqlx::query_as(r#"SELECT EXISTS (SELECT * FROM files WHERE file_name = $1)"#)
                .bind(file_name)
                .fetch_one(pool)
                .await
                .context("Failed to count files")?;
        Ok(exists)
    }

    pub async fn insert(&self, pool: &PgPool) -> Result<()> {
        let _ = sqlx::query(
            r#"
        INSERT INTO files (file_name, file_url, space_id, block_id, origin_file_path, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        )
        .bind(&self.file_name)
        .bind(&self.file_url)
        .bind(&self.space_id)
        .bind(&self.block_id)
        .bind(&self.origin_file_path)
        .bind(self.created_at)
        .execute(pool)
        .await
        .context("Failed to insert row")?;
        Ok(())
    }
}

pub async fn create_pool(host: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(host)
        .await
        .with_context(|| format!("Failed to connect {host}"))?;
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("Failed to run migration")?;
    Ok(pool)
}
