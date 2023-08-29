use std::collections::HashMap;

use anyhow::{ensure, Context, Result};
use const_format::formatcp;
use reqwest::header;
use serde::{Deserialize, Serialize};

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
        let client = reqwest::Client::builder().build()?;
        let req = PageDataRequest {
            r#type: "block-space".to_string(),
            block_id: page_id,
            name: "page".to_string(),
            save_parent: false,
            show_move_to: false,
        };
        let res = client
            .post(formatcp!("{NOTION_API_BASE}/getPublicPageData"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, format!("token_v2={}", self.token_v2))
            .json(&req)
            .send()
            .await
            .context("request")?;
        ensure!(res.status().is_success(), res.status());
        let res = res.json::<PageDataResponse>().await.context("parse json")?;
        Ok(res)
    }

    pub async fn load_page_chunk_request(
        &self,
        page_id: &str,
        chunk_number: usize,
        limit: usize,
        cursor: Option<Cursor>,
    ) -> Result<LoadPageChunkResponse> {
        let page_id = to_dashed_id(page_id).context("convert to dashed id")?;
        let client = reqwest::Client::builder().build()?;
        let req = LoadPageChunkRequest {
            page_id,
            chunk_number,
            limit,
            cursor,
            vertical_columns: false,
        };
        let res = client
            .post(formatcp!("{NOTION_API_BASE}/loadPageChunk"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, format!("token_v2={}", self.token_v2))
            .json(&req)
            .send()
            .await
            .context("request")?;
        ensure!(res.status().is_success(), res.status());
        let res = res
            .json::<LoadPageChunkResponse>()
            .await
            .context("parse json")?;
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
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub role: String,
    pub value: serde_json::Value,
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
