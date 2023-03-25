use std::string::FromUtf8Error;

use base64::{engine::general_purpose, Engine};
use serde::{Deserialize, Serialize};
use si_data_nats::NatsError;
use si_data_pg::PgError;
use telemetry::prelude::*;
use thiserror::Error;

use crate::func::argument::FuncArgumentError;
use crate::{
    impl_standard_model, pk, standard_model, standard_model_accessor, standard_model_accessor_ro,
    DalContext, FuncBinding, FuncDescriptionContents, HistoryEventError, StandardModel,
    StandardModelError, Tenancy, Timestamp, Visibility,
};

use self::backend::{FuncBackendKind, FuncBackendResponseType};

pub mod argument;
pub mod backend;
pub mod binding;
pub mod binding_return_value;
pub mod description;
pub mod execution;
pub mod identity;

#[derive(Error, Debug)]
pub enum FuncError {
    #[error("error serializing/deserializing json: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("pg error: {0}")]
    Pg(#[from] PgError),
    #[error("nats txn error: {0}")]
    Nats(#[from] NatsError),
    #[error("history event error: {0}")]
    HistoryEvent(#[from] HistoryEventError),
    #[error("standard model error: {0}")]
    StandardModelError(#[from] StandardModelError),
    #[error("error decoding code_base64: {0}")]
    Decode(#[from] base64::DecodeError),
    #[error("utf8 encoding error: {0}")]
    FromUtf8(#[from] FromUtf8Error),
    #[error("func argument error: {0}")]
    FuncArgument(#[from] FuncArgumentError),
    #[error("func binding error: {0}")]
    FuncBinding(String),

    #[error("could not find func by id: {0}")]
    NotFound(FuncId),
    #[error("could not find func by name: {0}")]
    NotFoundByName(String),
    #[error("contents ({0}) response type does not match func response type: {1}")]
    ResponseTypeMismatch(FuncDescriptionContents, FuncBackendResponseType),

    /// Could not find [`FuncArgument`](crate::FuncArgument) corresponding to the identity [`Func`].
    #[error("identity func argument not found")]
    IdentityFuncArgumentNotFound,
    /// Could not find the identity [`Func`].
    #[error("identity func not found")]
    IdentityFuncNotFound,
    /// When attempting to find the identity [`Func`], there were too many [`Funcs`](Func) returned.
    #[error("too many funcs found when looking for identity func")]
    TooManyFuncsFoundForIdentity,
}

pub type FuncResult<T> = Result<T, FuncError>;

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct FuncMetadataView {
    pub display_name: String,
    pub description: Option<String>,
    pub link: Option<String>,
}

pk!(FuncPk);
pk!(FuncId);

/// A `Func` is the declaration of the existence of a function. It has a name,
/// and corresponds to a given function backend (and its associated return types).
///
/// `handler` is the name of the entry point into the code in `code_base64`.
/// For example, if we had a code block of
/// `function myValidator(actual, expected) { return true; }` in `code_base64`,
/// the `handler` value should be `myValidator`.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pk: FuncPk,
    id: FuncId,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    link: Option<String>,
    hidden: bool,
    builtin: bool,
    backend_kind: FuncBackendKind,
    backend_response_type: FuncBackendResponseType,
    handler: Option<String>,
    code_base64: Option<String>,
    code_sha256: String,
    #[serde(flatten)]
    tenancy: Tenancy,
    #[serde(flatten)]
    timestamp: Timestamp,
    #[serde(flatten)]
    visibility: Visibility,
}

impl_standard_model! {
    model: Func,
    pk: FuncPk,
    id: FuncId,
    table_name: "funcs",
    history_event_label_base: "function",
    history_event_message_name: "Function"
}

impl Func {
    #[instrument(skip_all)]
    pub async fn new(
        ctx: &DalContext,
        name: impl AsRef<str>,
        backend_kind: FuncBackendKind,
        backend_response_type: FuncBackendResponseType,
    ) -> FuncResult<Self> {
        let name = name.as_ref();
        let row = ctx
            .txns()
            .pg()
            .query_one(
                "SELECT object FROM func_create_v1($1, $2, $3, $4, $5)",
                &[
                    ctx.tenancy(),
                    ctx.visibility(),
                    &name,
                    &backend_kind.as_ref(),
                    &backend_response_type.as_ref(),
                ],
            )
            .await?;
        let object = standard_model::finish_create_from_row(ctx, row).await?;
        Ok(object)
    }

    #[allow(clippy::result_large_err)]
    pub fn code_plaintext(&self) -> FuncResult<Option<String>> {
        Ok(match self.code_base64() {
            Some(base64_code) => Some(String::from_utf8(
                general_purpose::STANDARD_NO_PAD.decode(base64_code)?,
            )?),
            None => None,
        })
    }

    pub async fn set_code_plaintext(
        &mut self,
        ctx: &DalContext,
        code: Option<&'_ str>,
    ) -> FuncResult<()> {
        self.set_code_base64(
            ctx,
            code.as_ref()
                .map(|code| general_purpose::STANDARD_NO_PAD.encode(code)),
        )
        .await
    }

    pub fn metadata_view(&self) -> FuncMetadataView {
        FuncMetadataView {
            display_name: self.display_name().unwrap_or_else(|| self.name()).into(),
            description: self.description().map(Into::into),
            link: self.description().map(Into::into),
        }
    }

    pub async fn for_binding(ctx: &DalContext, func_binding: &FuncBinding) -> FuncResult<Self> {
        let row = ctx
            .txns()
            .pg()
            .query_one(
                "SELECT row_to_json(funcs.*) AS object
                FROM funcs_v1($1, $2) AS funcs
                INNER JOIN func_binding_belongs_to_func_v1($1, $2) AS func_binding_belongs_to_func
                    ON funcs.id = func_binding_belongs_to_func.belongs_to_id
                WHERE func_binding_belongs_to_func.object_id = $3",
                &[ctx.tenancy(), ctx.visibility(), func_binding.id()],
            )
            .await?;
        let object = standard_model::finish_create_from_row(ctx, row).await?;
        Ok(object)
    }

    standard_model_accessor!(name, String, FuncResult);
    standard_model_accessor!(display_name, Option<String>, FuncResult);
    standard_model_accessor!(description, Option<String>, FuncResult);
    standard_model_accessor!(link, Option<String>, FuncResult);
    standard_model_accessor!(hidden, bool, FuncResult);
    standard_model_accessor!(builtin, bool, FuncResult);
    standard_model_accessor!(backend_kind, Enum(FuncBackendKind), FuncResult);
    standard_model_accessor!(
        backend_response_type,
        Enum(FuncBackendResponseType),
        FuncResult
    );
    standard_model_accessor!(handler, Option<String>, FuncResult);
    standard_model_accessor!(code_base64, Option<String>, FuncResult);
    standard_model_accessor_ro!(code_sha256, String);
}
