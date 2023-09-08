use std::collections::HashMap;

use anyhow::{ensure, Context, Result};
use reqwest::{header, Method};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const NOTION_API_BASE: &str = "https://www.notion.so/api/v3";

#[derive(Debug)]
pub struct Notion {
    token_v2: String,
}

impl Notion {
    pub fn new(token_v2: String) -> Notion {
        Notion { token_v2 }
    }

    pub async fn get_page_data(&self, page_id: &str) -> Result<PageDataResponse> {
        let page_id = to_dashed_id(page_id).context("convert to dashed id")?;
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
        page_id: &str,
        chunk_number: usize,
        limit: usize,
        cursor: Option<Cursor>,
    ) -> Result<LoadPageChunkResponse> {
        let page_id = to_dashed_id(page_id).context("convert to dashed id")?;
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
    ) -> Result<GetUploadFileUrlResponse> {
        let req = GetUploadFileUrlRequest {
            bucket: "secure".to_string(),
            content_type,
            name,
        };
        self.request(Method::POST, "/getUploadFileUrl", &req).await
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
            .json(&body)
            .send()
            .await
            .context("request")?;
        ensure!(res.status().is_success(), res.status());
        let res = res.json::<R>().await.context("parse json")?;
        Ok(res)
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
    pub created_by_id: String,
    pub created_by_table: String,
    pub created_time: usize,
    pub file_ids: Option<Vec<String>>,
    pub id: String,
    pub space_id: String,
    pub r#type: String,

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
pub struct GetUploadFileUrlRequest {
    pub bucket: String,
    pub content_type: String,
    pub name: String,
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
