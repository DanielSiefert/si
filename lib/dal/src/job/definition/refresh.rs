use std::{collections::HashMap, convert::TryFrom};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    job::{
        consumer::{
            JobConsumer, JobConsumerError, JobConsumerMetadata, JobConsumerResult, JobInfo,
        },
        producer::{JobMeta, JobProducer, JobProducerResult},
    },
    AccessBuilder, Component, ComponentId, DalContext, StandardModel, Visibility,
};

#[derive(Debug, Deserialize, Serialize)]
struct RefreshJobArgs {
    component_ids: Vec<ComponentId>,
}

impl From<RefreshJob> for RefreshJobArgs {
    fn from(value: RefreshJob) -> Self {
        Self {
            component_ids: value.component_ids,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RefreshJob {
    component_ids: Vec<ComponentId>,
    access_builder: AccessBuilder,
    visibility: Visibility,
    job: Option<JobInfo>,
}

impl RefreshJob {
    pub fn new(ctx: &DalContext, component_ids: Vec<ComponentId>) -> Box<Self> {
        let access_builder = AccessBuilder::from(ctx.clone());
        let visibility = *ctx.visibility();

        Box::new(Self {
            component_ids,
            access_builder,
            visibility,
            job: None,
        })
    }
}

impl JobProducer for RefreshJob {
    fn args(&self) -> JobProducerResult<serde_json::Value> {
        Ok(serde_json::to_value(RefreshJobArgs::from(self.clone()))?)
    }

    fn meta(&self) -> JobProducerResult<JobMeta> {
        let mut custom = HashMap::new();
        custom.insert(
            "access_builder".to_string(),
            serde_json::to_value(self.access_builder.clone())?,
        );
        custom.insert(
            "visibility".to_string(),
            serde_json::to_value(self.visibility)?,
        );

        Ok(JobMeta {
            retry: Some(0),
            custom,
            ..JobMeta::default()
        })
    }

    fn identity(&self) -> String {
        serde_json::to_string(self).expect("Cannot serialize RefreshJob")
    }
}

impl JobConsumerMetadata for RefreshJob {
    fn type_name(&self) -> String {
        "RefreshJob".to_string()
    }

    fn access_builder(&self) -> AccessBuilder {
        self.access_builder.clone()
    }

    fn visibility(&self) -> Visibility {
        self.visibility
    }
}

#[async_trait]
impl JobConsumer for RefreshJob {
    async fn run(&self, ctx: &DalContext) -> JobConsumerResult<()> {
        let deleted_ctx = &ctx.clone_with_delete_visibility();
        for component_id in &self.component_ids {
            let component = Component::get_by_id(deleted_ctx, component_id)
                .await?
                .ok_or(JobConsumerError::ComponentNotFound(*component_id))?;
            component.act(deleted_ctx, "refresh").await?;
        }

        Ok(())
    }
}

impl TryFrom<JobInfo> for RefreshJob {
    type Error = JobConsumerError;

    fn try_from(job: JobInfo) -> Result<Self, Self::Error> {
        if job.args().len() != 3 {
            return Err(JobConsumerError::InvalidArguments(
                r#"[{ component_ids: Vec<ComponentId> }, <AccessBuilder>, <Visibility>]"#
                    .to_string(),
                job.args().to_vec(),
            ));
        }
        let args: RefreshJobArgs = serde_json::from_value(job.args()[0].clone())?;
        let access_builder: AccessBuilder = serde_json::from_value(job.args()[1].clone())?;
        let visibility: Visibility = serde_json::from_value(job.args()[2].clone())?;

        Ok(Self {
            component_ids: args.component_ids,
            access_builder,
            visibility,
            job: Some(job),
        })
    }
}
