use anyhow::{bail, Context, Result};
use reqwest::{header, Method};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use super::types::*;

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
