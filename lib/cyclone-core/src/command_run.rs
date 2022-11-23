use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRunRequest {
    pub execution_id: String,
    pub handler: String,
    pub code_base64: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum ResourceStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRunResultSuccess {
    pub execution_id: String,
    pub value: Option<serde_json::Value>,
    pub status: ResourceStatus,
    pub message: Option<String>,
    // Collects the error if the function throws
    pub error: Option<String>,
}
