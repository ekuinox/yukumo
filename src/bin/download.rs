use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use dotenv::dotenv;
use notionfs::download_with_url;

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

    download_with_url(
        url, block_id, space_id, token_v2, file_token, user_agent, path,
    )
    .await
}
