use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use si_data_nats::NatsError;
use si_data_pg::PgError;
use telemetry::prelude::*;
use thiserror::Error;

use crate::validation::prototype::context::ValidationPrototypeContextBuilder;
use crate::{
    func::FuncId,
    impl_standard_model, pk,
    standard_model::{self, objects_from_rows},
    standard_model_accessor, DalContext, HistoryEventError, PropId, SchemaVariantId, StandardModel,
    StandardModelError, SystemId, Timestamp, Visibility, WriteTenancy,
};
use crate::{PropKind, ValidationPrototypeContext};

pub mod context;

#[derive(Error, Debug)]
pub enum ValidationPrototypeError {
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

    #[error("prop for validation prototype context is not of primitive prop kind, found: {0:?}")]
    ContextPropKindIsNotPrimitive(PropKind),
    #[error("for builder {0:?}, the following fields must be set: {1:?}")]
    PrerequisteFieldsUnset(ValidationPrototypeContextBuilder, Vec<&'static str>),
    #[error("prop not found by id: {0}")]
    PropNotFound(PropId),
}

pub type ValidationPrototypeResult<T> = Result<T, ValidationPrototypeError>;

const LIST_FOR_PROP: &str = include_str!("../queries/validation_prototype_list_for_prop.sql");
const LIST_FOR_SCHEMA_VARIANT: &str =
    include_str!("../queries/validation_prototype_list_for_schema_variant.sql");
const LIST_FOR_FUNC: &str = include_str!("../queries/validation_prototype_list_for_func.sql");
const FIND_FOR_CONTEXT: &str = include_str!("../queries/validation_prototype_find_for_context.sql");

pk!(ValidationPrototypePk);
pk!(ValidationPrototypeId);

// An ValidationPrototype joins a `Func` to the context in which
// the component that is created with it can use to generate a ValidationResolver.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct ValidationPrototype {
    pk: ValidationPrototypePk,
    id: ValidationPrototypeId,
    func_id: FuncId,
    args: serde_json::Value,
    link: Option<String>,
    #[serde(flatten)]
    context: ValidationPrototypeContext,
    #[serde(flatten)]
    tenancy: WriteTenancy,
    #[serde(flatten)]
    timestamp: Timestamp,
    #[serde(flatten)]
    visibility: Visibility,
}

impl_standard_model! {
    model: ValidationPrototype,
    pk: ValidationPrototypePk,
    id: ValidationPrototypeId,
    table_name: "validation_prototypes",
    history_event_label_base: "validation_prototype",
    history_event_message_name: "Validation Prototype"
}

impl ValidationPrototype {
    #[instrument(skip_all)]
    pub async fn new(
        ctx: &DalContext,
        func_id: FuncId,
        args: serde_json::Value,
        context: ValidationPrototypeContext,
    ) -> ValidationPrototypeResult<Self> {
        let row = ctx
            .txns()
            .pg()
            .query_one(
                "SELECT object FROM validation_prototype_create_v1($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    ctx.write_tenancy(),
                    ctx.visibility(),
                    &func_id,
                    &args,
                    &context.prop_id(),
                    &context.schema_id(),
                    &context.schema_variant_id(),
                    &context.system_id(),
                ],
            )
            .await?;
        let object = standard_model::finish_create_from_row(ctx, row).await?;
        Ok(object)
    }

    standard_model_accessor!(func_id, Pk(FuncId), ValidationPrototypeResult);
    standard_model_accessor!(args, Json<JsonValue>, ValidationPrototypeResult);
    standard_model_accessor!(link, Option<String>, ValidationPrototypeResult);

    pub fn context(&self) -> &ValidationPrototypeContext {
        &self.context
    }

    /// List all [`ValidationPrototypes`](Self) for a given [`Prop`](crate::Prop).
    #[instrument(skip_all)]
    pub async fn list_for_prop(
        ctx: &DalContext,
        prop_id: PropId,
        system_id: SystemId,
    ) -> ValidationPrototypeResult<Vec<Self>> {
        let rows = ctx
            .txns()
            .pg()
            .query(
                LIST_FOR_PROP,
                &[ctx.read_tenancy(), ctx.visibility(), &prop_id, &system_id],
            )
            .await?;
        let object = objects_from_rows(rows)?;
        Ok(object)
    }

    /// List all [`ValidationPrototypes`](Self) for all [`Props`](crate::Prop) in a
    /// [`SchemaVariant`](crate::SchemaVariant).
    ///
    /// _You can access the [`PropId`](crate::Prop) via the [`ValidationPrototypeContext`], if
    /// needed._
    #[instrument(skip_all)]
    pub async fn list_for_schema_variant(
        ctx: &DalContext,
        schema_variant_id: SchemaVariantId,
        system_id: SystemId,
    ) -> ValidationPrototypeResult<Vec<Self>> {
        let rows = ctx
            .txns()
            .pg()
            .query(
                LIST_FOR_SCHEMA_VARIANT,
                &[
                    ctx.read_tenancy(),
                    ctx.visibility(),
                    &schema_variant_id,
                    &system_id,
                ],
            )
            .await?;
        let object = objects_from_rows(rows)?;
        Ok(object)
    }

    /// List all [`ValidationPrototypes`](Self) for a [`Func`](crate::Func)
    #[instrument(skip_all)]
    pub async fn list_for_func(
        ctx: &DalContext,
        func_id: FuncId,
    ) -> ValidationPrototypeResult<Vec<Self>> {
        let rows = ctx
            .txns()
            .pg()
            .query(
                LIST_FOR_FUNC,
                &[ctx.read_tenancy(), ctx.visibility(), &func_id],
            )
            .await?;

        Ok(objects_from_rows(rows)?)
    }

    pub async fn find_for_context(
        ctx: &DalContext,
        context: ValidationPrototypeContext,
    ) -> ValidationPrototypeResult<Vec<Self>> {
        let rows = ctx
            .txns()
            .pg()
            .query(
                FIND_FOR_CONTEXT,
                &[
                    ctx.read_tenancy(),
                    ctx.visibility(),
                    &context.prop_id(),
                    &context.schema_variant_id(),
                    &context.schema_id(),
                ],
            )
            .await?;

        Ok(objects_from_rows(rows)?)
    }
}
