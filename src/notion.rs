use std::collections::{HashMap, HashSet};

use anyhow::{bail, ensure, Context, Result};
use reqwest::{header, Method};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

const NOTION_API_BASE: &str = "https://www.notion.so/api/v3";
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Safari/537.36";

#[derive(Debug)]
pub struct Notion {
    user_agent: Option<String>,
    token_v2: String,
}

impl Notion {
    pub fn new(token_v2: String, user_agent: Option<String>) -> Notion {
        Notion {
            user_agent,
            token_v2,
        }
    }

    pub async fn get_page_data(&self, page_id: String) -> Result<PageDataResponse> {
        let req = PageDataRequest {
            r#type: "block-space".to_string(),
            block_id: page_id,
            name: "page".to_string(),
            save_parent: false,
            show_move_to: false,
        };
        self.request(Method::POST, "/getPublicPageData", &req).await
    }

    pub async fn load_page_chunk_request(
        &self,
        page_id: String,
        chunk_number: usize,
        limit: usize,
        cursor: Option<Cursor>,
    ) -> Result<LoadPageChunkResponse> {
        let req = LoadPageChunkRequest {
            page_id,
            chunk_number,
            limit,
            cursor,
            vertical_columns: false,
        };
        self.request(Method::POST, "/loadPageChunk", &req).await
    }

    pub async fn get_upload_file_url(
        &self,
        name: String,
        content_type: String,
        content_length: usize,
        block_id: String,
        space_id: String,
    ) -> Result<GetUploadFileUrlResponse> {
        let req = GetUploadFileUrlRequest {
            bucket: "secure".to_string(),
            content_type,
            name,
            content_length,
            record: GetUploadFileUrlRequestRecord {
                id: block_id,
                space_id,
                table: "block".to_string(),
            },
        };
        self.request(Method::POST, "/getUploadFileUrl", &req).await
    }

    pub async fn save_transactions(&self, transactions: Vec<Transaction>) -> Result<()> {
        let req = SaveTransactionRequest {
            request_id: Uuid::new_v4().to_string(),
            transactions,
        };
        let _: serde_json::Value = self
            .request(Method::POST, "/saveTransactions", &req)
            .await?;
        Ok(())
    }

    pub async fn get_signed_file_urls(
        &self,
        req: &GetSignedFileUrlsRequest,
    ) -> Result<GetSignedFileUrlsResponse> {
        self.request(Method::POST, "/getSignedFileUrls", req).await
    }

    pub async fn request<R: DeserializeOwned>(
        &self,
        method: Method,
        resource: &str,
        body: &impl Serialize,
    ) -> Result<R> {
        let client = reqwest::Client::builder().build().context("build client")?;
        let res = client
            .request(method, format!("{NOTION_API_BASE}{resource}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, format!("token_v2={}", self.token_v2))
            .header(header::USER_AGENT, self.user_agent())
            .json(&body)
            .send()
            .await
            .context("request")?;
        let status = res.status();
        let text = res.text().await;
        if !status.is_success() {
            let text = text.unwrap_or_default();
            bail!("{status} {text}");
        }
        let text = text.context("parse text error")?;
        let res =
            serde_json::from_str::<R>(&text).with_context(|| format!("parse json {text:?}"))?;
        Ok(res)
    }

    pub fn user_agent(&self) -> &str {
        self.user_agent
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or(DEFAULT_USER_AGENT)
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Stack {
    pub id: String,
    pub index: usize,
    pub table: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    pub stack: Vec<Vec<Stack>>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct BlockValue {
    pub alive: bool,
    pub id: String,

    #[serde(flatten)]
    pub rest: serde_json::Value,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub role: String,
    pub value: BlockValue,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecordMap {
    #[serde(rename = "block")]
    pub blocks: HashMap<String, Block>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PageDataRequest {
    pub r#type: String,
    pub block_id: String,
    pub name: String,
    pub save_parent: bool,
    pub show_move_to: bool,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PageDataResponse {
    pub beta_enabled: bool,
    pub can_join_space: bool,
    pub owner_user_id: String,
    pub page_id: String,
    pub public_access_role: String,
    pub require_login: bool,
    pub space_domain: String,
    pub space_id: String,
    pub space_name: String,
    pub user_has_explicit_access: bool,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LoadPageChunkRequest {
    pub page_id: String,
    pub chunk_number: usize,
    pub limit: usize,
    pub cursor: Option<Cursor>,
    pub vertical_columns: bool,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LoadPageChunkResponse {
    pub cursor: Cursor,
    pub record_map: RecordMap,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetUploadFileUrlRequestRecord {
    pub id: String,
    pub space_id: String,
    pub table: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetUploadFileUrlRequest {
    pub bucket: String,
    pub content_type: String,
    pub name: String,
    pub content_length: usize,
    pub record: GetUploadFileUrlRequestRecord,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetUploadFileUrlResponse {
    pub url: String,
    pub signed_get_url: String,
    pub signed_put_url: String,
    #[serde(flatten)]
    pub rest: serde_json::Value,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OperationPointer {
    pub table: String,
    pub id: String,
    pub space_id: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum OperationCommand {
    Set,
    Update,
    ListAfter,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub pointer: OperationPointer,
    pub path: HashSet<String>,
    pub command: OperationCommand,
    pub args: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub id: String,
    pub space_id: String,
    pub operations: Vec<Operation>,
    pub debug: HashMap<String, String>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SaveTransactionRequest {
    pub request_id: String,
    pub transactions: Vec<Transaction>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetSignedFileUrlsRequestUrl {
    pub permission_record: OperationPointer,
    pub url: String,
    pub use_s3_url: bool,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetSignedFileUrlsRequest {
    pub urls: Vec<GetSignedFileUrlsRequestUrl>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetSignedFileUrlsResponse {
    pub signed_urls: Vec<String>,
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
