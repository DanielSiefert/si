use crate::extract::{HandlerContext, PosthogClient};
use crate::service::v2::AccessBuilder;

use axum::extract::{Host, OriginalUri, Path};
use axum::Json;
use dal::workspace_integrations::{WorkspaceIntegration, WorkspaceIntegrationId};
use dal::WorkspacePk;
use serde::{Deserialize, Serialize};

use super::{IntegrationsError, IntegrationsResult};

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIntegrationRequest {
    slack_webhook_url: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIntegrationResponse {
    pub integration: WorkspaceIntegration,
}

pub async fn update_integration(
    HandlerContext(builder): HandlerContext,
    AccessBuilder(access_builder): AccessBuilder,
    PosthogClient(_posthog_client): PosthogClient,
    OriginalUri(_original_uri): OriginalUri,
    Host(_host_name): Host,
    Path((_workspace_pk, workspace_integration_id)): Path<(WorkspacePk, WorkspaceIntegrationId)>,
    Json(request): Json<UpdateIntegrationRequest>,
) -> IntegrationsResult<Json<UpdateIntegrationResponse>> {
    let ctx = builder.build_head(access_builder).await?;

    let mut integration = WorkspaceIntegration::get_by_pk(&ctx, workspace_integration_id)
        .await?
        .ok_or(IntegrationsError::IntegrationNotFound(
            workspace_integration_id,
        ))?;

    if let Some(webhook_url) = request.slack_webhook_url {
        integration.update_webhook_url(&ctx, webhook_url).await?;
    }
    ctx.commit().await?;

    Ok(Json(UpdateIntegrationResponse { integration }))
}
