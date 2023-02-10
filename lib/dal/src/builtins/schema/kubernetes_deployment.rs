use crate::schema::variant::definition::SchemaVariantDefinitionMetadataJson;
use crate::schema::variant::leaves::LeafKind;
use crate::{
    builtins::schema::MigrationDriver, schema::variant::leaves::LeafInputLocation, Prop, PropId,
};
use crate::{component::ComponentKind, schema::variant::leaves::LeafInput};
use crate::{
    func::argument::FuncArgument, socket::SocketArity, AttributePrototypeArgument,
    AttributeReadContext, AttributeValue, BuiltinsError, BuiltinsResult, DalContext,
    InternalProvider, PropKind, SchemaVariant, StandardModel,
};

/// The default Kubernetes API version used when creating documentation URLs.
const DEFAULT_KUBERNETES_API_VERSION: &str = "1.22";

/// Provides the documentation URL prefix for a given Kubernetes documentation URL path.
fn doc_url(path: impl AsRef<str>) -> String {
    format!(
        "https://v{}.docs.kubernetes.io/docs/{}",
        DEFAULT_KUBERNETES_API_VERSION.replace('.', "-"),
        path.as_ref(),
    )
}

impl MigrationDriver {
    pub async fn migrate_kubernetes_deployment(
        &self,
        ctx: &DalContext,
        ui_menu_category: &str,
        node_color: &str,
    ) -> BuiltinsResult<()> {
        let (_schema, mut schema_variant, root_prop, _, _, _) = match self
            .create_schema_and_variant(
                ctx,
                SchemaVariantDefinitionMetadataJson::new(
                    "Kubernetes Deployment",
                    Some("Deployment"),
                    ui_menu_category,
                    node_color,
                    ComponentKind::Standard,
                    None,
                    None,
                ),
                None,
            )
            .await?
        {
            Some(tuple) => tuple,
            None => return Ok(()),
        };

        schema_variant
            .set_link(
                ctx,
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/deployment-v1/",
                )),
            )
            .await?;

        let api_version_prop = self
            .create_prop(
                ctx,
                "apiVersion",
                PropKind::String,
                None,
                Some(root_prop.domain_prop_id),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/deployment-v1/#Deployment",
                )),
            )
            .await?;
        let kind_prop = self
            .create_prop(
                ctx,
                "kind",
                PropKind::String,
                None,
                Some(root_prop.domain_prop_id),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/deployment-v1/#Deployment",
                )),
            )
            .await?;

        let metadata_prop = self
            .create_kubernetes_metadata_prop_for_deployment(ctx, root_prop.domain_prop_id)
            .await?;

        let spec_prop = self
            .create_kubernetes_deployment_spec_prop(ctx, root_prop.domain_prop_id)
            .await?;

        // Qualifications
        let (qualification_func_id, qualification_func_argument_id) = self
            .find_func_and_single_argument_by_names(ctx, "si:qualificationKubevalYaml", "code")
            .await?;
        SchemaVariant::add_leaf(
            ctx,
            qualification_func_id,
            *schema_variant.id(),
            None,
            LeafKind::Qualification,
            vec![LeafInput {
                location: LeafInputLocation::Code,
                func_argument_id: qualification_func_argument_id,
            }],
        )
        .await?;

        // Add code generation
        let code_generation_func_id = self.get_func_id("si:generateYAML").ok_or(
            BuiltinsError::FuncNotFoundInMigrationCache("si:generateYAML"),
        )?;
        let code_generation_func_argument =
            FuncArgument::find_by_name_for_func(ctx, "domain", code_generation_func_id)
                .await?
                .ok_or_else(|| {
                    BuiltinsError::BuiltinMissingFuncArgument(
                        "si:generateYAML".to_string(),
                        "domain".to_string(),
                    )
                })?;
        SchemaVariant::add_leaf(
            ctx,
            code_generation_func_id,
            *schema_variant.id(),
            None,
            LeafKind::CodeGeneration,
            vec![LeafInput {
                location: LeafInputLocation::Domain,
                func_argument_id: *code_generation_func_argument.id(),
            }],
        )
        .await?;

        let identity_func_item = self
            .get_func_item("si:identity")
            .ok_or(BuiltinsError::FuncNotFoundInMigrationCache("si:identity"))?;

        let (docker_image_explicit_internal_provider, _input_socket) =
            InternalProvider::new_explicit_with_socket(
                ctx,
                *schema_variant.id(),
                "Container Image",
                identity_func_item.func_id,
                identity_func_item.func_binding_id,
                identity_func_item.func_binding_return_value_id,
                SocketArity::Many,
                false,
            )
            .await?;

        let (kubernetes_namespace_explicit_internal_provider, _input_socket) =
            InternalProvider::new_explicit_with_socket(
                ctx,
                *schema_variant.id(),
                "Kubernetes Namespace",
                identity_func_item.func_id,
                identity_func_item.func_binding_id,
                identity_func_item.func_binding_return_value_id,
                SocketArity::Many,
                false,
            )
            .await?;

        schema_variant.finalize(ctx, None).await?;

        // Set default values after finalization.
        self.set_default_value_for_prop(ctx, *api_version_prop.id(), serde_json::json!["apps/v1"])
            .await?;
        self.set_default_value_for_prop(ctx, *kind_prop.id(), serde_json::json!["Deployment"])
            .await?;

        // Connect the "domain namespace" prop to the "kubernetes_namespace" explicit internal provider.
        let domain_namespace_prop = self
            .find_child_prop_by_name(ctx, *metadata_prop.id(), "namespace")
            .await?;
        let domain_namespace_attribute_value_read_context =
            AttributeReadContext::default_with_prop(*domain_namespace_prop.id());
        let domain_namespace_attribute_value =
            AttributeValue::find_for_context(ctx, domain_namespace_attribute_value_read_context)
                .await?
                .ok_or(BuiltinsError::AttributeValueNotFoundForContext(
                    domain_namespace_attribute_value_read_context,
                ))?;
        let mut domain_namespace_attribute_prototype = domain_namespace_attribute_value
            .attribute_prototype(ctx)
            .await?
            .ok_or(BuiltinsError::MissingAttributePrototypeForAttributeValue)?;
        domain_namespace_attribute_prototype
            .set_func_id(ctx, identity_func_item.func_id)
            .await?;
        AttributePrototypeArgument::new_for_intra_component(
            ctx,
            *domain_namespace_attribute_prototype.id(),
            identity_func_item.func_argument_id,
            *kubernetes_namespace_explicit_internal_provider.id(),
        )
        .await?;

        // Connect the "template namespace" prop to the "kubernetes_namespace" explicit internal provider.
        let template_prop = self
            .find_child_prop_by_name(ctx, *spec_prop.id(), "template")
            .await?;
        let template_metadata_prop = self
            .find_child_prop_by_name(ctx, *template_prop.id(), "metadata")
            .await?;
        let template_namespace_prop = self
            .find_child_prop_by_name(ctx, *template_metadata_prop.id(), "namespace")
            .await?;
        let template_namespace_attribute_value_read_context =
            AttributeReadContext::default_with_prop(*template_namespace_prop.id());
        let template_namespace_attribute_value =
            AttributeValue::find_for_context(ctx, template_namespace_attribute_value_read_context)
                .await?
                .ok_or(BuiltinsError::AttributeValueNotFoundForContext(
                    template_namespace_attribute_value_read_context,
                ))?;
        let mut template_namespace_attribute_prototype = template_namespace_attribute_value
            .attribute_prototype(ctx)
            .await?
            .ok_or(BuiltinsError::MissingAttributePrototypeForAttributeValue)?;
        template_namespace_attribute_prototype
            .set_func_id(ctx, identity_func_item.func_id)
            .await?;
        AttributePrototypeArgument::new_for_intra_component(
            ctx,
            *template_namespace_attribute_prototype.id(),
            identity_func_item.func_argument_id,
            *kubernetes_namespace_explicit_internal_provider.id(),
        )
        .await?;

        // Connect the "/root/domain/spec/template/spec/containers" field to the "Container Image" explicit
        // internal provider. We need to use the appropriate function with and name the argument "images".
        let template_spec_prop = self
            .find_child_prop_by_name(ctx, *template_prop.id(), "spec")
            .await?;
        let containers_prop = self
            .find_child_prop_by_name(ctx, *template_spec_prop.id(), "containers")
            .await?;
        let containers_attribute_value_read_context =
            AttributeReadContext::default_with_prop(*containers_prop.id());
        let containers_attribute_value =
            AttributeValue::find_for_context(ctx, containers_attribute_value_read_context)
                .await?
                .ok_or(BuiltinsError::AttributeValueNotFoundForContext(
                    containers_attribute_value_read_context,
                ))?;
        let mut containers_attribute_prototype = containers_attribute_value
            .attribute_prototype(ctx)
            .await?
            .ok_or(BuiltinsError::MissingAttributePrototypeForAttributeValue)?;
        let (transformation_func_id, transformation_func_argument_id) = self
            .find_func_and_single_argument_by_names(
                ctx,
                "si:dockerImagesToK8sDeploymentContainerSpec",
                "images",
            )
            .await?;
        containers_attribute_prototype
            .set_func_id(ctx, transformation_func_id)
            .await?;
        AttributePrototypeArgument::new_for_intra_component(
            ctx,
            *containers_attribute_prototype.id(),
            transformation_func_argument_id,
            *docker_image_explicit_internal_provider.id(),
        )
        .await?;

        Ok(())
    }

    async fn create_kubernetes_deployment_spec_prop(
        &self,
        ctx: &DalContext,
        parent_prop_id: PropId,
    ) -> BuiltinsResult<Prop> {
        let spec_prop = self
            .create_prop(
                ctx,
                "spec",
                PropKind::Object,
                None,
                Some(parent_prop_id),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/deployment-v1/#DeploymentSpec",
                )),
            )
            .await?;

        let _replicas_prop = self
            .create_prop(
                ctx,
                "replicas",
                PropKind::Integer,
                None,
                Some(*spec_prop.id()),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/deployment-v1/#DeploymentSpec",
                )),
            )
            .await?;

        let _selector_prop = self
            .create_kubernetes_selector_prop(ctx, *spec_prop.id())
            .await?;
        let _template_prop = self
            .create_kubernetes_pod_template_spec_prop(ctx, *spec_prop.id())
            .await?;

        Ok(spec_prop)
    }

    async fn create_kubernetes_pod_template_spec_prop(
        &self,
        ctx: &DalContext,
        parent_prop_id: PropId,
    ) -> BuiltinsResult<Prop> {
        let template_prop = self
            .create_prop(
                ctx,
                "template",
                PropKind::Object,
                None,
                Some(parent_prop_id),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-template-v1/#PodTemplateSpec",
                )),
            )
            .await?;

        let _metadata_prop = self
            .create_kubernetes_metadata_prop_for_deployment(ctx, *template_prop.id())
            .await?;

        let _spec_prop = self
            .create_kubernetes_pod_spec_prop(ctx, *template_prop.id())
            .await?;

        Ok(template_prop)
    }

    async fn create_kubernetes_selector_prop(
        &self,
        ctx: &DalContext,
        parent_prop_id: PropId,
    ) -> BuiltinsResult<Prop> {
        let selector_prop = self
            .create_prop(
                ctx,
                "selector",
                PropKind::Object,
                None,
                Some(parent_prop_id),
                Some(doc_url(
                    "reference/kubernetes-api/common-definitions/label-selector/#LabelSelector",
                )),
            )
            .await?;

        {
            let match_labels_prop = self
                .create_prop(
                    ctx,
                    "matchLabels",
                    PropKind::Map,
                    None,
                    Some(*selector_prop.id()),
                    Some(doc_url(
                        "reference/kubernetes-api/common-definitions/label-selector/#LabelSelector",
                    )),
                )
                .await?;
            let _match_labels_value_prop = self
                .create_prop(
                    ctx,
                    "labelValue",
                    PropKind::String,
                    None,
                    Some(*match_labels_prop.id()),
                    Some(doc_url(
                        "reference/kubernetes-api/common-definitions/label-selector/#LabelSelector",
                    )),
                )
                .await?;
        }

        Ok(selector_prop)
    }

    async fn create_kubernetes_pod_spec_prop(
        &self,
        ctx: &DalContext,
        parent_prop_id: PropId,
    ) -> BuiltinsResult<Prop> {
        let spec_prop = self
            .create_prop(
                ctx,
                "spec",
                PropKind::Object,
                None,
                Some(parent_prop_id),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-v1/#PodSpec",
                )),
            )
            .await?;

        let containers_prop = self
            .create_prop(
                ctx,
                "containers",
                PropKind::Array,
                None,
                Some(*spec_prop.id()),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-v1/#containers",
                )),
            )
            .await?;
        let _containers_element_prop = self
            .create_kubernetes_container_prop(ctx, *containers_prop.id())
            .await?;

        Ok(spec_prop)
    }

    async fn create_kubernetes_container_prop(
        &self,
        ctx: &DalContext,
        parent_prop_id: PropId,
    ) -> BuiltinsResult<Prop> {
        let container_prop = self
            .create_prop(
                ctx,
                "container",
                PropKind::Object,
                None,
                Some(parent_prop_id),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-v1/#Container",
                )),
            )
            .await?;

        let _name_prop = self
            .create_prop(
                ctx,
                "name",
                PropKind::String,
                None,
                Some(*container_prop.id()),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-v1/#Container",
                )),
            )
            .await?;

        let _image_prop = self
            .create_prop(
                ctx,
                "image",
                PropKind::String,
                None,
                Some(*container_prop.id()),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-v1/#image",
                )),
            )
            .await?;

        let ports_prop = self
            .create_prop(
                ctx,
                "ports",
                PropKind::Array,
                None,
                Some(*container_prop.id()),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-v1/#ports",
                )),
            )
            .await?;
        let _ports_element_prop = self
            .create_kubernetes_container_port_prop(ctx, *ports_prop.id())
            .await?;

        Ok(container_prop)
    }

    async fn create_kubernetes_container_port_prop(
        &self,
        ctx: &DalContext,
        parent_prop_id: PropId,
    ) -> BuiltinsResult<Prop> {
        let port_prop = self
            .create_prop(
                ctx,
                "port",
                PropKind::Object,
                None,
                Some(parent_prop_id),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-v1/#ports",
                )),
            )
            .await?;

        let container_port_prop = self
            .create_prop(
                ctx,
                "containerPort",
                PropKind::Integer,
                None,
                Some(*port_prop.id()),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-v1/#ports",
                )),
            )
            .await?;

        let _protocol_prop = self
            .create_prop(
                ctx,
                "protocol",
                PropKind::String,
                None,
                Some(*port_prop.id()),
                Some(doc_url(
                    "reference/kubernetes-api/workload-resources/pod-v1/#ports",
                )),
            )
            .await?;

        Ok(container_port_prop)
    }

    async fn create_kubernetes_metadata_prop_for_deployment(
        &self,
        ctx: &DalContext,
        parent_prop_id: PropId,
    ) -> BuiltinsResult<Prop> {
        let metadata_prop = self
            .create_prop(
                ctx,
                "metadata",
                PropKind::Object,
                None,
                Some(parent_prop_id),
                Some(doc_url(
                    "reference/kubernetes-api/common-definitions/object-meta/#ObjectMeta",
                )),
            )
            .await?;

        {
            // TODO: add validation
            //validation: [
            //  {
            //    kind: ValidatorKind.Regex,
            //    regex: "^[A-Za-z0-9](?:[A-Za-z0-9-]{0,251}[A-Za-z0-9])?$",
            //    message: "Kubernetes names must be valid DNS subdomains",
            //    link:
            //      "https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#dns-subdomain-names",
            //  },
            //],

            let _name_prop = self
                .create_prop(
                    ctx,
                    "name",
                    PropKind::String,
                    None,
                    Some(*metadata_prop.id()),
                    Some(doc_url(
                        "reference/kubernetes-api/common-definitions/object-meta/#ObjectMeta",
                    )),
                )
                .await?;
        }

        {
            let _generate_name_prop = self
                .create_prop(
                    ctx,
                    "generateName",
                    PropKind::String,
                    None,
                    Some(*metadata_prop.id()),
                    Some(doc_url(
                        "reference/kubernetes-api/common-definitions/object-meta/#ObjectMeta",
                    )),
                )
                .await?;
        }

        {
            let _namespace_prop = self
                .create_prop(
                    ctx,
                    "namespace",
                    PropKind::String,
                    None,
                    Some(*metadata_prop.id()),
                    Some(doc_url(
                        "concepts/overview/working-with-objects/namespaces/",
                    )),
                )
                .await?;
        }

        {
            let labels_prop = self
                .create_prop(
                    ctx,
                    "labels",
                    PropKind::Map,
                    None,
                    Some(*metadata_prop.id()),
                    Some(doc_url("concepts/overview/working-with-objects/labels/")),
                )
                .await?;
            let _labels_value_prop = self
                .create_prop(
                    ctx,
                    "labelValue",
                    PropKind::String,
                    None,
                    Some(*labels_prop.id()),
                    Some(doc_url("concepts/overview/working-with-objects/labels/")),
                )
                .await?;
        }

        {
            let annotations_prop = self
                .create_prop(
                    ctx,
                    "annotations",
                    PropKind::Map,
                    None, // How to specify it as a map of string values?
                    Some(*metadata_prop.id()),
                    Some(doc_url(
                        "concepts/overview/working-with-objects/annotations/",
                    )),
                )
                .await?;
            let _annotations_value_prop = self
                .create_prop(
                    ctx,
                    "annotationValue",
                    PropKind::String,
                    None,
                    Some(*annotations_prop.id()),
                    Some(doc_url(
                        "concepts/overview/working-with-objects/annotations/",
                    )),
                )
                .await?;
        }
        Ok(metadata_prop)
    }
}
