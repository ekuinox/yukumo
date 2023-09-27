mod config;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use futures::{Stream, StreamExt};
use home::home_dir;
use indicatif::ProgressBar;
use notionfs::{
    attach_file_to_block, create_new_block, get_file_by_signed_url, get_signed_file_urls,
    get_signed_put_file,
    notion::{client::Notion, types::PageDataResponse},
    put_to_signed_url, to_dashed_id, Body,
};
use sqlx::FromRow;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::config::Config;

#[derive(Parser)]
enum Subcommand {
    Put {
        source: PathBuf,
    },
    Query {
        name: String,
    },
    Get {
        file_name: String,
        #[clap(short, long, default_value = "./")]
        out_dir: PathBuf,
    },
}

#[derive(Parser)]
struct Cli {
    #[clap(short, long)]
    config: Option<PathBuf>,

    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let path = cli.config.unwrap_or_else(|| {
        home_dir()
            .expect("Failed to get homedir")
            .join("Yukumo.toml")
    });

    let config =
        Config::open(&path).with_context(|| format!("Failed to open config = {path:?}"))?;

    env_logger::init();

    log::info!("Config path = {path:?}");

    let pool = DatabasePoolOptions::new()
        .max_connections(5)
        .connect(&config.database.host)
        .await
        .with_context(|| format!("Failed to connect {}", config.database.host))?;

    sqlx::migrate!()
        .run(&pool)
        .await
        .context("Failed to migrate database.")?;

    match cli.subcommand {
        Subcommand::Put { source } => {
            let path = source;
            let client = Notion::new(config.notion.token_v2, config.notion.user_agent);
            let page_id =
                to_dashed_id(&config.notion.page_id).context("Failed to convert dashed id")?;
            let PageDataResponse {
                owner_user_id,
                page_id,
                space_id,
                ..
            } = client
                .get_page_data(page_id)
                .await
                .with_context(|| format!("Failed to get notion page {}", config.notion.page_id))?;

            log::debug!("page_id = {page_id}");
            log::debug!("space_id = {space_id}");
            log::debug!(
                "owner_user_id = {}",
                owner_user_id.as_ref().map(String::as_str).unwrap_or("")
            );

            // 最初にブロックを作っとかないといけないっぽい
            let new_block_id = create_new_block(&client, &space_id, &page_id).await?;

            // 署名付きアップロードURLを取得して
            let (url, signed_get_url, signed_put_url, name, mime, content_length) =
                get_signed_put_file(&client, &path, &new_block_id, &space_id).await?;

            log::info!("block_id = {new_block_id}");
            log::info!("space_id = {space_id}");
            log::info!("url = {url}");
            log::info!("signed_get_url = {signed_get_url}");
            log::debug!("signed_put_url = {signed_put_url}");

            let file = File::open(&path)
                .await
                .context("Failed to open input file")?;

            let pb = ProgressBar::new(content_length);
            let stream = create_upload_stream(file, pb);

            put_to_signed_url(
                &signed_put_url,
                content_length,
                &mime,
                Body::wrap_stream(stream),
            )
            .await?;

            // ブロックにファイルをくっつける
            attach_file_to_block(
                &client,
                &new_block_id,
                &space_id,
                &url,
                &name,
                content_length,
            )
            .await?;

            let row = FileRow {
                file_url: url,
                space_id,
                block_id: new_block_id,
                file_name: name,
            };

            let _ = sqlx::query(
                r#"
            INSERT INTO files (file_url, space_id, block_id, file_name)
            VALUES ($1, $2, $3, $4)
            "#,
            )
            .bind(&row.file_url)
            .bind(&row.space_id)
            .bind(&row.block_id)
            .bind(&row.file_name)
            .execute(&pool)
            .await
            .context("Failed to insert row")?;
        }
        Subcommand::Query { name } => {
            let name = format!("%{name}%");
            let files: Vec<FileRow> = sqlx::query_as("SELECT * FROM files WHERE file_name LIKE ?")
                .bind(&name)
                .fetch_all(&pool)
                .await
                .context("Failed to select files")?;
            dbg!(&files);
        }
        Subcommand::Get { file_name, out_dir } => {
            let FileRow {
                file_url,
                space_id,
                block_id,
                file_name,
            } = sqlx::query_as("SELECT * FROM files WHERE file_name = ?")
                .bind(&file_name)
                .fetch_one(&pool)
                .await
                .context("Failed to get file")?;

            let client = Notion::new(config.notion.token_v2, config.notion.user_agent);
            log::debug!("UserAgent = {}", client.user_agent());

            let signed_urls =
                get_signed_file_urls(&client, &[(&file_url, &block_id, &space_id)]).await?;

            if !out_dir.exists() {
                tokio::fs::create_dir_all(&out_dir).await?;
            }

            for url in signed_urls {
                let res = get_file_by_signed_url(&url, &config.notion.file_token).await?;
                let path = out_dir.join(&file_name);
                let bytes = res.bytes().await?;
                tokio::fs::write(&path, bytes).await?;
                log::info!("Saved {path:?}");

                log::info!("- {url}");
            }
        }
    }

    Ok(())
}

#[cfg(feature = "postgres")]
pub type DatabasePoolOptions = sqlx::postgres::PgPoolOptions;

#[cfg(feature = "sqlite")]
pub type DatabasePoolOptions = sqlx::sqlite::SqlitePoolOptions;

fn create_upload_stream(
    file: File,
    pb: ProgressBar,
) -> impl Stream<Item = anyhow::Result<bytes::Bytes>> + 'static {
    async_stream::try_stream! {
        let mut stream = ReaderStream::new(file);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            pb.inc(chunk.len() as u64);
            yield chunk;
        }
        pb.finish();
    }
}

#[derive(FromRow, Debug)]
pub struct FileRow {
    pub file_url: String,
    pub space_id: String,
    pub block_id: String,
    pub file_name: String,
}
