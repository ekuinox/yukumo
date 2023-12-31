mod config;
mod database;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use chrono::{Local, Utc};
use clap::Parser;
use futures::{Stream, StreamExt};
use home::home_dir;
use indicatif::ProgressBar;
use notionfs::{
    attach_file_to_block, create_new_block, get_file_by_signed_url, get_file_stem,
    get_signed_file_urls, get_signed_put_file,
    notion::{client::Notion, types::PageDataResponse},
    put_to_signed_url, to_dashed_id, Body,
};
use shadow_rs::shadow;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{
    config::Config,
    database::{create_pool, FileRow},
};

shadow!(meta);

#[derive(Parser)]
#[clap(version = meta::PKG_VERSION, long_version = meta::VERSION)]
struct Cli {
    #[clap(short, long, global = true, env = "YUKUMO_CONFIG")]
    config: Option<PathBuf>,

    #[clap(short, long, global = true)]
    skip_on_failure: bool,

    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[derive(Parser)]
enum Subcommand {
    Put {
        source: PathBuf,

        #[clap(short, long)]
        prefix: Option<String>,

        #[clap(short = 'n', long = "name")]
        file_name: Option<String>,
    },
    Query {
        prefix: String,
    },
    Get {
        file_name: String,

        #[clap(short, long)]
        output: PathBuf,
    },
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

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "yukumo=info");
    }
    env_logger::init();

    log::debug!("Config path = {path:?}");

    match cli.subcommand {
        Subcommand::Put {
            source,
            file_name,
            prefix,
        } => {
            if source.is_file() {
                put(config, source, file_name, prefix).await
            } else if source.is_dir() {
                let dir = source.read_dir().context("Failed to read directory.")?;
                for entry in dir {
                    if let Ok(entry) = entry {
                        if let Err(e) =
                            put(config.clone(), entry.path(), None, prefix.clone()).await
                        {
                            log::error!("Failed to put {}", entry.path().to_string_lossy());
                            log::error!("{e:#?}");
                            if !cli.skip_on_failure {
                                bail!("Aborted by error.");
                            }
                        }
                    }
                }
                Ok(())
            } else {
                bail!("Invalid path: {source:?}");
            }
        }
        Subcommand::Query { prefix } => query(config, prefix).await,
        Subcommand::Get { file_name, output } => get(config, file_name, output).await,
    }
}

async fn get(config: Config, file_name: String, output: PathBuf) -> Result<()> {
    let pool = create_pool(&config.database.host).await?;

    let FileRow {
        file_url,
        space_id,
        block_id,
        ..
    } = FileRow::find_one(&pool, &file_name).await?;

    let client = Notion::new(config.notion.token_v2, config.notion.user_agent);
    log::debug!("UserAgent = {}", client.user_agent());

    let signed_urls = get_signed_file_urls(&client, &[(&file_url, &block_id, &space_id)]).await?;

    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(&parent).await?;
    }

    for url in signed_urls {
        let res = get_file_by_signed_url(&url, &config.notion.file_token).await?;
        let bytes = res.bytes().await?;
        tokio::fs::write(&output, bytes).await?;
        log::info!("Saved {output:?}");

        log::debug!("- {url}");
    }

    Ok(())
}

async fn put(
    config: Config,
    source: PathBuf,
    name: Option<String>,
    prefix: Option<String>,
) -> Result<()> {
    let pool = create_pool(&config.database.host).await?;

    let client = Notion::new(config.notion.token_v2, config.notion.user_agent);
    let page_id = to_dashed_id(&config.notion.page_id).context("Failed to convert dashed id")?;
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
    log::debug!("owner_user_id = {}", owner_user_id.as_deref().unwrap_or(""));

    // 最初にブロックを作っとかないといけないっぽい
    let new_block_id = create_new_block(&client, &space_id, &page_id).await?;

    let name = if let Some(name) = name {
        name
    } else {
        get_file_stem(&source)?
    };
    let name = prefix.map(|prefix| prefix + &name).unwrap_or(name);

    if FileRow::is_exists(&pool, &name).await? {
        bail!("file_name ({name}) is already exists.");
    }

    // 署名付きアップロードURLを取得して
    let (url, signed_get_url, signed_put_url, mime, content_length) =
        get_signed_put_file(&client, &source, &name, &new_block_id, &space_id).await?;

    log::info!("block_id = {new_block_id}");
    log::info!("space_id = {space_id}");
    log::info!("url = {url}");
    log::info!("signed_get_url = {signed_get_url}");
    log::debug!("signed_put_url = {signed_put_url}");

    let file = File::open(&source)
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
        origin_file_path: source
            .canonicalize()
            .unwrap_or(source)
            .to_string_lossy()
            .to_string(),
        created_at: Utc::now().naive_utc(),
    };

    row.insert(&pool).await?;

    log::info!(
        "- {}: {} ({})",
        row.file_name,
        row.origin_file_path,
        row.origin_file_path
    );

    Ok(())
}

async fn query(config: Config, prefix: String) -> Result<()> {
    let pool = create_pool(&config.database.host).await?;
    let files = FileRow::query(&pool, &prefix).await?;
    for FileRow {
        file_name,
        origin_file_path,
        created_at,
        ..
    } in files
    {
        log::info!(
            "- {file_name}: {origin_file_path} ({})",
            created_at
                .and_local_timezone(Local)
                .single()
                .unwrap()
                .to_rfc3339()
        );
    }
    Ok(())
}

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
