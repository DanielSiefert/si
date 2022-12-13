use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ComponentView;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolverFunctionRequest {
    pub execution_id: String,
    pub handler: String,
    pub component: ResolverFunctionComponent,
    pub response_type: ResolverFunctionResponseType,
    pub code_base64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResolverFunctionComponent {
    pub data: ComponentView,
    pub parents: Vec<ComponentView>,
    // TODO: add widget data here (for example select's options)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Default)]
// Should be kept in sync with dal::func::backend::FuncBackendResponseType
pub enum ResolverFunctionResponseType {
    Array,
    Boolean,
    Identity,
    Integer,
    Map,
    PropObject,
    Qualification,
    CodeGeneration,
    Confirmation,
    String,
    #[default]
    Unset,
    Json,
    Validation,
    Workflow,
    Command,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolverFunctionResultSuccess {
    pub execution_id: String,
    pub data: Value,
    pub unset: bool,
    pub timestamp: u64,
}
