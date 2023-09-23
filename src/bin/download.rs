use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use dotenv::dotenv;
use notionfs::{get_file_by_signed_url, get_signed_file_urls, notion::client::Notion};

#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(short, long, env = "NOTION_TOKEN_V2")]
    token_v2: String,

    #[clap(short, long, env = "NOTION_FILE_TOKEN")]
    file_token: String,

    #[clap(short, long, env = "USER_AGENT")]
    user_agent: Option<String>,

    #[clap(long)]
    url: String,

    #[clap(long)]
    space_id: String,

    #[clap(long)]
    block_id: String,

    path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenv();

    let Cli {
        token_v2,
        file_token,
        user_agent,
        path,
        url,
        space_id,
        block_id,
    } = Cli::parse();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let client = Notion::new(token_v2, user_agent);
    log::debug!("UserAgent = {}", client.user_agent());

    let signed_urls = get_signed_file_urls(&client, &[(&url, &block_id, &space_id)]).await?;

    if !path.exists() {
        tokio::fs::create_dir_all(&path).await?;
    }

    let file_token = format!("file_token={file_token}");
    for url in signed_urls {
        let res = get_file_by_signed_url(&url, &file_token).await?;
        if let Some(s) = res
            .url()
            .path_segments()
            .and_then(|segments| segments.last())
        {
            let path = path.join(s);
            let bytes = res.bytes().await?;
            tokio::fs::write(&path, bytes).await?;
            log::info!("Saved {path:?}");
        }

        log::info!("- {url}");
    }

    Ok(())
}
