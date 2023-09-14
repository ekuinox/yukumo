mod notion;

use std::path::PathBuf;

use anyhow::{bail, ensure, Context, Result};
use clap::Parser;
use dotenv::dotenv;
use reqwest::{header, Body};
use serde_json::json;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};
use uuid::Uuid;

use crate::notion::{
    to_dashed_id, GetSignedFileUrlsRequest, GetSignedFileUrlsRequestUrl, GetSignedFileUrlsResponse,
    GetUploadFileUrlResponse, LoadPageChunkResponse, Notion, Operation, OperationCommand,
    OperationPointer, PageDataResponse, Transaction,
};

#[derive(Parser, Debug)]
pub enum Cli {
    Upload {
        #[clap(short, long, env = "NOTION_PAGE_ID")]
        page_id: String,

        #[clap(short, long, env = "NOTION_TOKEN_V2")]
        token_v2: String,

        #[clap(short, long, env = "USER_AGENT")]
        user_agent: Option<String>,

        path: PathBuf,
    },
    Download {
        #[clap(short, long, env = "NOTION_PAGE_ID")]
        page_id: String,

        #[clap(short, long, env = "NOTION_TOKEN_V2")]
        token_v2: String,

        #[clap(short, long, env = "NOTION_FILE_TOKEN")]
        file_token: String,

        #[clap(short, long, env = "USER_AGENT")]
        user_agent: Option<String>,

        path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenv();

    let cli = Cli::parse();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    match cli {
        Cli::Upload {
            page_id,
            token_v2,
            user_agent,
            path,
        } => upload(page_id, token_v2, user_agent, path).await,
        Cli::Download {
            page_id,
            token_v2,
            file_token,
            user_agent,
            path,
        } => download(page_id, token_v2, file_token, user_agent, path).await,
    }
}

async fn download(
    page_id: String,
    token_v2: String,
    file_token: String,
    user_agent: Option<String>,
    path: PathBuf,
) -> Result<()> {
    let client = Notion::new(token_v2, user_agent);
    dbg!(client.user_agent());

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

    // ページのブロックを読みだす
    let LoadPageChunkResponse { record_map, .. } = client
        .load_page_chunk_request(page_id.clone(), 0, 50, None)
        .await
        .context("load page chunk")?;

    let urls = record_map
        .blocks
        .into_iter()
        .flat_map(|(id, block)| {
            block.value.rest.get("properties").and_then(|properties| {
                properties
                    .get("source")
                    .and_then(|source| {
                        source.get(0).and_then(|source| {
                            source
                                .get(0)
                                .and_then(|o| o.as_str().map(|s| s.to_string()))
                        })
                    })
                    .map(|url| (id, url))
            })
        })
        .map(|(id, url)| GetSignedFileUrlsRequestUrl {
            url,
            permission_record: OperationPointer {
                table: "block".to_string(),
                id,
                space_id: space_id.clone(),
            },
            use_s3_url: false,
        })
        .collect::<Vec<_>>();

    if urls.is_empty() {
        log::info!("Not found file urls");
        return Ok(());
    }

    let GetSignedFileUrlsResponse { signed_urls } = client
        .get_signed_file_urls(&GetSignedFileUrlsRequest { urls })
        .await
        .context("get signed urls")?;

    if !path.exists() {
        tokio::fs::create_dir_all(&path).await?;
    }

    let file_token = format!("file_token={file_token}");
    for url in signed_urls {
        let res = reqwest::Client::builder()
            .build()?
            .get(&url)
            .header(header::COOKIE, &file_token)
            .send()
            .await?;
        ensure!(res.status().is_success());
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

async fn upload(
    page_id: String,
    token_v2: String,
    user_agent: Option<String>,
    path: PathBuf,
) -> Result<()> {
    let client = Notion::new(token_v2, user_agent);
    dbg!(client.user_agent());

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

    // ページのブロックを読みだす
    let LoadPageChunkResponse { record_map, .. } = client
        .load_page_chunk_request(page_id.clone(), 0, 50, None)
        .await
        .context("load page chunk")?;

    log::trace!("record_map: {record_map:#?}");

    // 挿入先の末尾の blockId が欲しい
    // これ元が HashMap なせいで挿入先が末尾に限られない
    let after = record_map
        .blocks
        .into_iter()
        .last()
        .filter(|(_, b)| b.value.alive)
        .map(|(id, _)| id);
    let Some(after) = after else {
        bail!("blocks are empty");
    };
    log::debug!("after = {after}");

    // 最初にブロックを作っとかないといけないっぽい
    let new_block_id = Uuid::new_v4().to_string();
    let new_block_pointer = OperationPointer {
        table: "block".to_string(),
        id: new_block_id.clone(),
        space_id: space_id.clone(),
    };
    log::debug!("new_block_id = {new_block_id}");

    client
        .save_transactions(vec![Transaction {
            id: Uuid::new_v4().to_string(),
            space_id: space_id.clone(),
            debug: [("userAction", "ListItemBlock.handleNativeDrop")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            operations: vec![
                Operation {
                    pointer: new_block_pointer.clone(),
                    path: Default::default(),
                    command: OperationCommand::Set,
                    args: [
                        ("type", json!("embed")),
                        ("space_id", json!(space_id.clone())),
                        ("id", json!(new_block_id.clone())),
                        ("version", json!(1)),
                    ]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                },
                // この辺多分なくても動作するっぽい
                // `created_by_user_id` は操作できないと怒られることもある
                // Operation {
                //     pointer: new_block_pointer.clone(),
                //     path: Default::default(),
                //     command: OperationCommand::Update,
                //     args: [
                //         ("created_by_user_id", json!(owner_user_id.clone())),
                //         ("created_by_table", json!("notion_user")),
                //         ("created_time", json!(Utc::now().timestamp_millis())),
                //         ("last_edited_time", json!(Utc::now().timestamp_millis())),
                //         ("last_edited_by_id", json!(owner_user_id.clone())),
                //         ("last_edited_by_table", json!("notion_user")),
                //     ]
                //     .into_iter()
                //     .map(|(k, v)| (k.to_string(), v))
                //     .collect(),
                // },
                Operation {
                    pointer: new_block_pointer.clone(),
                    path: Default::default(),
                    command: OperationCommand::Update,
                    args: [
                        ("parent_id", json!(page_id.clone())),
                        ("parent_table", json!("block")),
                        ("alive", json!(true)),
                    ]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                },
                Operation {
                    pointer: OperationPointer {
                        table: "block".to_string(),
                        id: page_id.clone(),
                        space_id: space_id.clone(),
                    },
                    path: ["content"].into_iter().map(ToString::to_string).collect(),
                    command: OperationCommand::ListAfter,
                    args: [
                        ("after", json!(after.clone())),
                        ("id", json!(new_block_id.clone())),
                    ]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                },
                // Operation {
                //     pointer: new_block_pointer.clone(),
                //     path: Default::default(),
                //     command: OperationCommand::Update,
                //     args: [("last_edited_time", json!(Utc::now().timestamp_millis()))]
                //         .into_iter()
                //         .map(|(k, v)| (k.to_string(), v))
                //         .collect(),
                // },
            ],
        }])
        .await
        .context("create new block")?;
    log::debug!("created new block");

    // でそれのフォーマットとかを定めて
    // error => Unsaved transactions: User does not have edit access to record
    // 前のブロック操作でページへのアクセス権を剥奪されちゃってる なんで？
    client
        .save_transactions(vec![Transaction {
            id: Uuid::new_v4().to_string(),
            space_id: space_id.clone(),
            debug: [("userAction", "embedBlockActions.initializeFormat")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            operations: vec![
                Operation {
                    pointer: new_block_pointer.clone(),
                    path: ["format"].into_iter().map(ToString::to_string).collect(),
                    command: OperationCommand::Update,
                    args: [
                        ("block_width", json!(120)),
                        ("block_height", serde_json::Value::Null),
                        ("block_preserve_scale", json!(true)),
                        ("block_full_width", json!(false)),
                        ("block_page_width", json!(false)),
                        // ("block_aspect_ratio", json!(0.63232421875)),
                    ]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                },
                // Operation {
                //     pointer: new_block_pointer.clone(),
                //     path: Default::default(),
                //     command: OperationCommand::Update,
                //     args: [("last_edited_time", json!(Utc::now().timestamp_millis()))]
                //         .into_iter()
                //         .map(|(k, v)| (k.to_string(), v))
                //         .collect(),
                // },
            ],
        }])
        .await
        .context("format new block")?;
    log::debug!("formated new block");

    // ファイルをアップロードして
    let content_length = tokio::fs::metadata(&path)
        .await
        .context("get metadata")?
        .len();
    let mime = mime_guess::from_path(&path);
    let mime = mime.first_or_text_plain().to_string();
    let name = path.file_name().unwrap().to_str().unwrap().to_string();
    let GetUploadFileUrlResponse {
        signed_get_url,
        signed_put_url,
        url,
        ..
    } = client
        .get_upload_file_url(
            name.clone(),
            mime.clone(),
            content_length as usize,
            new_block_id.clone(),
            space_id.clone(),
        )
        .await
        .context("get upload file url")?;

    log::info!("signed_get_url = {signed_get_url}");
    log::debug!("signed_put_url = {signed_put_url}");
    log::debug!("url = {url}");

    {
        let client = reqwest::Client::builder().gzip(true).build()?;
        let stream = FramedRead::new(
            File::open(&path).await.context("open file")?,
            BytesCodec::new(),
        );

        let res = client
            .put(&signed_put_url)
            .header(
                header::CONTENT_LENGTH,
                std::fs::metadata(&path).unwrap().len(),
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

        log::debug!("Put s3 OK");
    }

    // ブロックにファイルをくっつける
    client
        .save_transactions(vec![Transaction {
            id: Uuid::new_v4().to_string(),
            space_id: space_id.clone(),
            debug: [("userAction", "embedBlockActions.initializeEmbedBlock")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            operations: vec![Operation {
                pointer: new_block_pointer.clone(),
                path: ["properties"]
                    .into_iter()
                    .map(ToString::to_string)
                    .collect(),
                command: OperationCommand::Update,
                args: [
                    ("source", json!([[url.clone()]])),
                    ("title", json!([[name.clone()]])),
                    ("size", json!([[size_to_text(content_length as usize)]])),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            }],
        }])
        .await
        .context("insert file to block")?;

    let GetSignedFileUrlsResponse { signed_urls } = client
        .get_signed_file_urls(&GetSignedFileUrlsRequest {
            urls: vec![GetSignedFileUrlsRequestUrl {
                permission_record: new_block_pointer,
                url,
                use_s3_url: false,
            }],
        })
        .await
        .context("get signed files urls")?;

    println!("{signed_urls:?}");

    Ok(())
}

fn size_to_text(bytes: usize) -> String {
    const UNIT: usize = 1000;
    if bytes < UNIT {
        format!("{bytes}B")
    } else if bytes < UNIT.pow(2) {
        format!("{:.1}KB", bytes as f64 / UNIT as f64)
    } else if bytes < UNIT.pow(3) {
        format!("{:.1}MB", bytes as f64 / UNIT.pow(2) as f64)
    } else if bytes < UNIT.pow(4) {
        format!("{:.1}GB", bytes as f64 / UNIT.pow(3) as f64)
    } else {
        format!("{:.1}TB", bytes as f64 / UNIT.pow(4) as f64)
    }
}
