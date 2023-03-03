use super::{pkg_open, PkgResult};
use crate::server::extract::{AccessBuilder, HandlerContext};
use axum::Json;
use dal::{pkg::import_pkg_from_pkg, Visibility, WsEvent};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallPkgRequest {
    pub name: String,
    #[serde(flatten)]
    pub visibility: Visibility,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallPkgResponse {
    pub success: bool,
}

pub async fn install_pkg(
    HandlerContext(builder): HandlerContext,
    AccessBuilder(request_ctx): AccessBuilder,
    Json(request): Json<InstallPkgRequest>,
) -> PkgResult<Json<InstallPkgResponse>> {
    let ctx = builder.build(request_ctx.build(request.visibility)).await?;

    let pkg = pkg_open(&builder, &request.name).await?;
    import_pkg_from_pkg(&ctx, &pkg, &request.name).await?;

    WsEvent::change_set_written(&ctx)
        .await?
        .publish_on_commit(&ctx)
        .await?;
    ctx.commit().await?;

    Ok(Json(InstallPkgResponse { success: true }))
}
