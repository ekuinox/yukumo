use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use dotenv::dotenv;
use futures::{Stream, StreamExt};
use indicatif::ProgressBar;
use notionfs::{
    attach_file_to_block, create_new_block, get_file_stem, get_signed_put_file,
    notion::{client::Notion, types::PageDataResponse},
    put_to_signed_url, to_dashed_id,
};
use reqwest::Body;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

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

    let client = Notion::new(token_v2, user_agent);
    log::debug!("UserAgent = {}", client.user_agent());

    let page_id = to_dashed_id(&page_id).context("parse page id")?;

    // ページから spaceId を取り出す
    let PageDataResponse {
        owner_user_id,
        page_id,
        space_id,
        ..
    } = client.get_page_data(page_id).await.context("get page")?;

    log::debug!("page_id = {page_id}");
    log::debug!("space_id = {space_id}");
    log::debug!(
        "owner_user_id = {}",
        owner_user_id.as_ref().map(String::as_str).unwrap_or("")
    );

    // 最初にブロックを作っとかないといけないっぽい
    let new_block_id = create_new_block(&client, &space_id, &page_id).await?;

    let name = get_file_stem(&path)?;

    // 署名付きアップロードURLを取得して
    let (url, signed_get_url, signed_put_url, mime, content_length) =
        get_signed_put_file(&client, &path, &name, &new_block_id, &space_id).await?;

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
