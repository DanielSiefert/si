// TODO(nick,fletcher): destroy this file!!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use veritech_client::{
    BeforeFunction, FunctionResult, ReconciliationRequest, ReconciliationResultSuccess,
};

use crate::func::backend::{ExtractPayload, FuncBackendResult, FuncDispatch, FuncDispatchContext};
use crate::AttributeValueId;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationDiffDomain {
    pub id: AttributeValueId,
    pub value: serde_json::Value,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationDiff {
    pub normalized_resource: Option<serde_json::Value>,
    pub resource: serde_json::Value,
    pub domain: ReconciliationDiffDomain,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct FuncBackendJsReconciliationArgs(HashMap<String, ReconciliationDiff>);

#[derive(Debug)]
pub struct FuncBackendJsReconciliation {
    pub context: FuncDispatchContext,
    pub request: ReconciliationRequest,
}

#[async_trait]
impl FuncDispatch for FuncBackendJsReconciliation {
    type Args = FuncBackendJsReconciliationArgs;
    type Output = ReconciliationResultSuccess;

    fn new(
        context: FuncDispatchContext,
        code_base64: &str,
        handler: &str,
        args: Self::Args,
        before: Vec<BeforeFunction>,
    ) -> Box<Self> {
        let request = ReconciliationRequest {
            execution_id: context.func_run_id.to_string(),
            handler: handler.into(),
            code_base64: code_base64.into(),
            args: serde_json::to_value(args)
                .expect("should be impossible to fail serialization here"),
            before,
        };

        Box::new(Self { context, request })
    }

    /// This private function dispatches the assembled request to veritech for execution.
    /// This is the "last hop" function in the dal before using the veritech client directly.
    async fn dispatch(self: Box<Self>) -> FuncBackendResult<FunctionResult<Self::Output>> {
        let (veritech, output_tx, workspace_id, change_set_id) = self.context.into_inner();
        let value = veritech
            .execute_reconciliation(
                output_tx.clone(),
                &self.request,
                &workspace_id.to_string(),
                &change_set_id.to_string(),
            )
            .await?;
        let value = match value {
            FunctionResult::Failure(failure) => FunctionResult::Success(Self::Output {
                execution_id: failure.execution_id().to_owned(),
                updates: Default::default(),
                actions: Default::default(),
                message: Some(failure.error().message.to_owned()),
            }),
            FunctionResult::Success(value) => FunctionResult::Success(value),
        };

        Ok(value)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ReconciliationResult {
    pub updates: HashMap<AttributeValueId, serde_json::Value>,
    pub actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub message: Option<String>,
}

impl ExtractPayload for ReconciliationResultSuccess {
    type Payload = ReconciliationResult;

    fn extract(self) -> FuncBackendResult<Self::Payload> {
        Ok(ReconciliationResult {
            updates: self
                .updates
                .into_iter()
                .map(|(k, v)| Ok((AttributeValueId::from_str(&k)?, v)))
                .collect::<FuncBackendResult<HashMap<_, _>>>()?,
            actions: self.actions,
            message: self.message,
        })
    }
}
