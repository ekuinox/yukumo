mod notion;

use std::path::PathBuf;

use anyhow::{ensure, Context, Result};
use clap::Parser;
use dotenv::dotenv;
use notion::GetUploadFileUrlResponse;
use reqwest::{header, Body};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::notion::Notion;

#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(short, long)]
    pub page_id: String,

    #[clap(short, long, env = "NOTION_TOKEN_V2")]
    pub token_v2: String,

    #[clap(short, long, env = "USER_AGENT")]
    pub user_agent: Option<String>,

    pub path: PathBuf,

    #[clap(long)]
    pub skip_get_page: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenv();

    let cli = Cli::parse();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let client = Notion::new(cli.token_v2, cli.user_agent);
    println!("UA: {}", client.user_agent());
    if !cli.skip_get_page {
        let data = client.get_page_data(&cli.page_id).await?;
        println!("{data:#?}");
    }
    if !cli.skip_get_page {
        let data = client
            .load_page_chunk_request(&cli.page_id, 0, 30, None)
            .await?;
        println!("{data:#?}");
    }
    {
        let mime = mime_guess::from_path(&cli.path);
        let mime = mime.first_or_text_plain().to_string();
        let GetUploadFileUrlResponse {
            signed_get_url,
            signed_put_url,
            url,
            ..
        } = client
            .get_upload_file_url(
                cli.path.file_name().unwrap().to_str().unwrap().to_string(),
                mime.clone(),
            )
            .await
            .context("get upload file url")?;

        println!("signed put url = {signed_put_url}");
        println!("signed get url = {signed_get_url}");
        println!("url = {url}");

        let client = reqwest::Client::builder().gzip(true).build()?;
        let stream = FramedRead::new(
            File::open(&cli.path).await.context("open file")?,
            BytesCodec::new(),
        );

        let res = client
            .put(&signed_put_url)
            .header(
                header::CONTENT_LENGTH,
                std::fs::metadata(&cli.path).unwrap().len(),
            )
            .header(header::CONTENT_TYPE, mime)
            .body(Body::wrap_stream(stream))
            .send()
            .await
            .context("request")?;
        ensure!(
            res.status().is_success(),
            "{} {:?}",
            res.status(),
            res.text().await
        );
    }
    Ok(())
}
