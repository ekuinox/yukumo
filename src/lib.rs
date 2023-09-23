pub mod notion;

use std::path::PathBuf;

use anyhow::{ensure, Context, Result};
use futures::{Stream, StreamExt};
use indicatif::ProgressBar;
use reqwest::{header, Body};
use serde_json::json;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::notion::{
    client::Notion,
    types::{
        GetSignedFileUrlsRequest, GetSignedFileUrlsRequestUrl, GetSignedFileUrlsResponse,
        GetUploadFileUrlResponse, Operation, OperationCommand, OperationPointer, PageDataResponse,
        Transaction,
    },
};

/// URL を元にファイルをダウンロードする
pub async fn download_with_url(
    url: String,
    block_id: String,
    space_id: String,
    token_v2: String,
    file_token: String,
    user_agent: Option<String>,
    path: PathBuf,
) -> Result<()> {
    let client = Notion::new(token_v2, user_agent);
    dbg!(client.user_agent());

    let GetSignedFileUrlsResponse { signed_urls } = client
        .get_signed_file_urls(&GetSignedFileUrlsRequest {
            urls: vec![GetSignedFileUrlsRequestUrl {
                url,
                use_s3_url: false,
                permission_record: OperationPointer {
                    table: "block".to_string(),
                    id: block_id,
                    space_id,
                },
            }],
        })
        .await
        .context("Failed to get signed urls")?;

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

pub async fn upload(
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
                    args: [("id", json!(new_block_id.clone()))]
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

    log::info!("block_id = {new_block_id}");
    log::info!("space_id = {space_id}");
    log::info!("url = {url}");
    log::info!("signed_get_url = {signed_get_url}");
    log::debug!("signed_put_url = {signed_put_url}");

    {
        let client = reqwest::Client::builder().gzip(true).build()?;
        let file = File::open(&path)
            .await
            .context("Failed to open input file")?;

        let pb = ProgressBar::new(content_length);
        let stream = create_upload_stream(file, pb);

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

/// id をダッシュでつなげたやつにする
pub fn to_dashed_id(id: &str) -> Result<String> {
    let id = id.replace('-', "");
    ensure!(id.len() == 32);

    let a = &id[0..8];
    let b = &id[8..12];
    let c = &id[12..16];
    let d = &id[16..20];
    let e = &id[20..];
    Ok(format!("{a}-{b}-{c}-{d}-{e}"))
}

#[test]
fn test_to_dashed_id() {
    const ID: &str = "2131b10cebf64938a1277089ff02dbe4";
    assert_eq!(
        to_dashed_id(ID).ok(),
        Some("2131b10c-ebf6-4938-a127-7089ff02dbe4".to_string())
    );
}
