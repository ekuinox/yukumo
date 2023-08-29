mod notion;

use anyhow::Result;
use clap::Parser;
use dotenv::dotenv;

use crate::notion::Notion;

#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(short, long)]
    pub page_id: String,

    #[clap(short, long, env = "NOTION_TOKEN_V2")]
    pub token_v2: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenv();

    let cli = Cli::parse();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let client = Notion::new(cli.token_v2);
    {
        let data = client.get_page_data(&cli.page_id).await?;
        println!("{data:#?}");
    }
    {
        let data = client
            .load_page_chunk_request(&cli.page_id, 0, 30, None)
            .await?;
        println!("{data:#?}");
    }
    Ok(())
}
