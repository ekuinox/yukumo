pub mod notion;

use std::path::Path;

use anyhow::{bail, ensure, Context, Result};
use reqwest::header;
use serde_json::json;
use uuid::Uuid;

use crate::notion::{
    client::Notion,
    types::{
        GetSignedFileUrlsRequest, GetSignedFileUrlsRequestUrl, GetSignedFileUrlsResponse,
        GetUploadFileUrlResponse, Operation, OperationCommand, OperationPointer, Transaction,
    },
};

pub use reqwest::{Body, Response};

/// 署名付きURLを取得する
/// urls の各要素は `(url, block_id, space_id)` であること
pub async fn get_signed_file_urls(
    client: &Notion,
    urls: &[(&str, &str, &str)],
) -> Result<Vec<String>> {
    let urls = urls
        .into_iter()
        .map(|(url, block_id, space_id)| GetSignedFileUrlsRequestUrl {
            url: url.to_string(),
            use_s3_url: false,
            permission_record: OperationPointer {
                table: "block".to_string(),
                id: block_id.to_string(),
                space_id: space_id.to_string(),
            },
        })
        .collect();
    let GetSignedFileUrlsResponse { signed_urls } = client
        .get_signed_file_urls(&GetSignedFileUrlsRequest { urls })
        .await
        .context("Failed to get signed urls")?;

    Ok(signed_urls)
}

/// 署名付きURLを使ってファイルを取得する
pub async fn get_file_by_signed_url(url: &str, file_token: &str) -> Result<Response> {
    let res = reqwest::Client::builder()
        .build()?
        .get(url)
        .header(header::COOKIE, format!("file_token={file_token}"))
        .send()
        .await?;
    ensure!(res.status().is_success());
    Ok(res)
}

/// 新しいブロックを生成する
pub async fn create_new_block(client: &Notion, space_id: &str, page_id: &str) -> Result<String> {
    let new_block_id = Uuid::new_v4().to_string();
    let new_block_pointer = OperationPointer {
        table: "block".to_string(),
        id: new_block_id.clone(),
        space_id: space_id.to_string(),
    };
    log::debug!("new_block_id = {new_block_id}");

    client
        .save_transactions(vec![Transaction {
            id: Uuid::new_v4().to_string(),
            space_id: space_id.to_string(),
            debug: Default::default(),
            operations: vec![
                Operation {
                    pointer: new_block_pointer.clone(),
                    path: Default::default(),
                    command: OperationCommand::Set,
                    args: [
                        ("type".to_string(), json!("embed")),
                        ("space_id".to_string(), json!(space_id.clone())),
                        ("id".to_string(), json!(new_block_id.clone())),
                        ("version".to_string(), json!(1)),
                    ]
                    .into(),
                },
                Operation {
                    pointer: new_block_pointer.clone(),
                    path: Default::default(),
                    command: OperationCommand::Update,
                    args: [
                        ("parent_id".to_string(), json!(page_id.to_string())),
                        ("parent_table".to_string(), json!("block")),
                        ("alive".to_string(), json!(true)),
                    ]
                    .into(),
                },
                Operation {
                    pointer: OperationPointer {
                        table: "block".to_string(),
                        id: page_id.to_string(),
                        space_id: space_id.to_string(),
                    },
                    path: ["content".to_string()].into(),
                    command: OperationCommand::ListAfter,
                    args: [("id".to_string(), json!(new_block_id.clone()))].into(),
                },
            ],
        }])
        .await
        .context("Failed to create new block")?;
    log::debug!("New block {new_block_id} created.");

    client
        .save_transactions(vec![Transaction {
            id: Uuid::new_v4().to_string(),
            space_id: space_id.to_string(),
            debug: Default::default(),
            operations: vec![Operation {
                pointer: new_block_pointer.clone(),
                path: ["format".to_string()].into(),
                command: OperationCommand::Update,
                args: [
                    ("block_width".to_string(), json!(120)),
                    ("block_height".to_string(), serde_json::Value::Null),
                    ("block_preserve_scale".to_string(), json!(true)),
                    ("block_full_width".to_string(), json!(false)),
                    ("block_page_width".to_string(), json!(false)),
                ]
                .into(),
            }],
        }])
        .await
        .context("Failed to format new block")?;
    log::debug!("New block {new_block_id} formatted.");

    Ok(new_block_id)
}

/// 署名付きURLを取得する
/// `(url, signed_get_url, signed_put_url, name, mime, content_length)` をタプルで返す
pub async fn get_signed_put_file(
    client: &Notion,
    path: &Path,
    block_id: &str,
    space_id: &str,
) -> Result<(String, String, String, String, String, u64)> {
    // ファイルをアップロードして
    let content_length = tokio::fs::metadata(path)
        .await
        .context("Failed to get metadata")?
        .len();
    let mime = mime_guess::from_path(&path);
    let mime = mime.first_or_text_plain().to_string();
    let Some(name) = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(ToString::to_string)
    else {
        bail!("Failed to get file name");
    };
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
            block_id.to_string(),
            space_id.to_string(),
        )
        .await
        .context("Failed to get upload file url")?;

    Ok((
        url,
        signed_get_url,
        signed_put_url,
        name,
        mime,
        content_length,
    ))
}

/// 署名付きURLを使ってファイルをアップロードする
pub async fn put_to_signed_url(
    signed_put_url: &str,
    content_length: u64,
    mime: &str,
    body: impl Into<Body>,
) -> Result<()> {
    {
        let client = reqwest::Client::builder().gzip(true).build()?;
        let res = client
            .put(signed_put_url)
            .header(header::CONTENT_LENGTH, content_length)
            .header(header::CONTENT_TYPE, mime)
            .body(body)
            .send()
            .await
            .with_context(|| format!("Failed to request {signed_put_url}"))?;
        ensure!(
            res.status().is_success(),
            "{} {:?}",
            res.status(),
            res.text().await
        );

        log::debug!("Put signed url");
    }

    Ok(())
}

/// ブロックに対してファイルをアタッチする
pub async fn attach_file_to_block(
    client: &Notion,
    block_id: &str,
    space_id: &str,
    file_url: &str,
    file_name: &str,
    content_length: u64,
) -> Result<()> {
    let new_block_pointer = OperationPointer {
        table: "block".to_string(),
        id: block_id.to_string(),
        space_id: space_id.to_string(),
    };

    client
        .save_transactions(vec![Transaction {
            id: Uuid::new_v4().to_string(),
            space_id: space_id.to_string(),
            debug: Default::default(),
            operations: vec![Operation {
                pointer: new_block_pointer.clone(),
                path: ["properties".to_string()].into(),
                command: OperationCommand::Update,
                args: [
                    ("source".to_string(), json!([[file_url.to_string()]])),
                    ("title".to_string(), json!([[file_name.to_string()]])),
                    (
                        "size".to_string(),
                        json!([[size_to_text(content_length as usize)]]),
                    ),
                ]
                .into(),
            }],
        }])
        .await
        .context("Failed to insert file to block")?;

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
