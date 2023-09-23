use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use dotenv::dotenv;
use notionfs::upload;

#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(short, long, env = "NOTION_PAGE_ID")]
    page_id: String,

    #[clap(short, long, env = "NOTION_TOKEN_V2")]
    token_v2: String,

    #[clap(short, long, env = "USER_AGENT")]
    user_agent: Option<String>,

    path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenv();

    let Cli {
        page_id,
        token_v2,
        user_agent,
        path,
    } = Cli::parse();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    upload(page_id, token_v2, user_agent, path).await
}
