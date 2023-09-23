use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

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
    pub owner_user_id: Option<String>,
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
