//! This module contains "scenario" tests and the tools needed to write them. Scenario tests are
//! "sdf" tests intended to cover end-to-end scenarios.

// Scenario tests below...
mod model_and_fix_flow_aws_key_pair;
mod model_and_fix_flow_whiskers;
mod model_flow_fedora_coreos_ignition;

use axum::{http::Method, Router};
use dal::{
    component::confirmation::ConfirmationView, property_editor::values::PropertyEditorValue,
    socket::SocketEdgeKind, AttributeValue, AttributeValueId, ComponentId, ComponentView,
    ComponentViewProperties, DalContext, FixBatchId, NodeId, Prop, PropKind, Schema, SchemaId,
    Socket, StandardModel, Visibility,
};
use sdf_server::service::{
    change_set::{
        apply_change_set::{ApplyChangeSetRequest, ApplyChangeSetResponse},
        create_change_set::{CreateChangeSetRequest, CreateChangeSetResponse},
    },
    component::{
        get_property_editor_values::{
            GetPropertyEditorValuesRequest, GetPropertyEditorValuesResponse,
        },
        insert_property_editor_value::{
            InsertPropertyEditorValueRequest, InsertPropertyEditorValueResponse,
        },
        update_property_editor_value::{
            UpdatePropertyEditorValueRequest, UpdatePropertyEditorValueResponse,
        },
    },
    diagram::{
        create_connection::{CreateConnectionRequest, CreateConnectionResponse},
        create_node::{CreateNodeRequest, CreateNodeResponse},
    },
    fix::{
        confirmations::{ConfirmationsRequest, ConfirmationsResponse},
        list::{BatchHistoryView, ListFixesRequest, ListFixesResponse},
        run::{FixRunRequest, FixesRunRequest, FixesRunResponse},
    },
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};

use crate::service_tests::{api_request_auth_json_body, api_request_auth_query};

/// This _private_ struct is a wrapper around metadata related to a [`Component`](dal::Component)
/// for use in scenario tests.
struct ComponentBag {
    pub component_id: ComponentId,
    pub node_id: NodeId,
}

impl From<CreateNodeResponse> for ComponentBag {
    fn from(response: CreateNodeResponse) -> Self {
        Self {
            component_id: response.component_id,
            node_id: response.node_id,
        }
    }
}

impl ComponentBag {
    /// Generate a [`ComponentView`](dal::ComponentView) and return the
    /// [`properties`](dal_test::helpers::component_view::ComponentViewProperties).
    pub async fn view(&self, ctx: &DalContext) -> ComponentViewProperties {
        let component_view = ComponentView::new(ctx, self.component_id)
            .await
            .expect("could not create component view");
        ComponentViewProperties::try_from(component_view)
            .expect("cannot create component view properties from component view")
    }
}

/// A type alias for a collection of values in the [`PropertyEditor`](dal::property_editor).
type PropertyValues = GetPropertyEditorValuesResponse;

/// This _private_ harness provides helpers and caches for writing scenario tests.
struct ScenarioHarness {
    app: Router,
    auth_token: String,
    builtins: HashMap<&'static str, SchemaId>,
}

impl ScenarioHarness {
    /// Create a new [`harness`](Self) by caching relevant metadata, including a list of builtin
    /// [`Schemas`](dal::Schema) by name.
    pub async fn new(
        ctx: &DalContext,
        app: Router,
        auth_token: String,
        builtin_schema_names: &[&'static str],
    ) -> Self {
        let mut builtins: HashMap<&'static str, SchemaId> = HashMap::new();
        for builtin_schema_name in builtin_schema_names {
            let schema = Schema::find_by_attr(ctx, "name", &builtin_schema_name.to_string())
                .await
                .expect("could not find schema by name")
                .pop()
                .expect("schema not found");
            builtins.insert(builtin_schema_name, *schema.id());
        }
        Self {
            app,
            auth_token,
            builtins,
        }
    }

    /// Find the "value" in the property editor for a given [`ComponentId`](dal::Component) and
    /// path. The path corresponds to the child (in order of lineage) from the
    /// [`RootProp`](dal::RootProp).
    ///
    /// For example: if you want the "value" at "/root/domain/poop/canoe", you would pass in
    /// \["domain", "poop", "canoe"\] as the path. From that, we would find the target
    /// [`Prop`](dal::Prop), "canoe".
    ///
    /// This also works with elements within maps and arrays. To access a map element, provide
    /// the key as the path item (e.g. for map "/root/domain/map" and element
    /// "/root/domain/map/foo", provide \["domain", "map", "foo"\]). To access an array element,
    /// provide the index as the path item (e.g. for array "/root/domain/array" and element
    /// "/root/domain/array/bar", provide \["domain", "array", "bar"\]).
    async fn find_value(
        &self,
        ctx: &DalContext,
        component_id: ComponentId,
        path: &[&str],
    ) -> (AttributeValueId, PropertyEditorValue) {
        // Prepare the queue and pop the first item from it. This will be our identifier to track
        // the current value that we are looking for.
        let mut queue = path
            .iter()
            .map(|p| p.to_string())
            .collect::<VecDeque<String>>();
        let mut identifier = queue.pop_front().expect("provided empty path");

        // Collect the property editor values that we need to traverse.
        let property_values = self.get_values(ctx, component_id).await;

        // Start with the root's child values.
        let value_ids = property_values
            .child_values
            .get(&property_values.root_value_id)
            .expect("could not get child props for root")
            .clone();

        // Initialize our trackers. We use this to help find our target value at every "level".
        let mut parent_value_id = property_values.root_value_id;
        let mut target_value = None;
        let mut parent_kind = PropKind::Object;
        let mut array_element_attribute_value_id = AttributeValueId::NONE;

        // Alright, here's what's going down: we need to pretend like we are a user in the UI. Well,
        // we have to perform the equivalent to a user "descending" into fields in the property
        // editor. We do this by crawling the schema to find the name of the prop at each level
        // (via the provided queue) or the index/key of a map/array element.
        let mut work_queue = VecDeque::from(value_ids);
        'outer: while let Some(value_id) = work_queue.pop_front() {
            let value = property_values
                .values
                .get(&value_id)
                .expect("could not get value by id");

            // Find the value out of the current sibling group based on the parent's prop kind.
            // For arrays, we will use the index map. For maps, we will use the key. For everything
            // else, we will use the prop name.
            let found_value = match parent_kind {
                PropKind::Array => value.attribute_value_id() == array_element_attribute_value_id,
                PropKind::Map => {
                    let found_key = value.key.as_ref().expect("key not found for child of map");
                    found_key == &identifier
                }
                _ => {
                    let prop = Prop::get_by_id(ctx, &value.prop_id())
                        .await
                        .expect("could not perform get by id")
                        .expect("prop not found");
                    prop.name() == identifier
                }
            };

            if found_value {
                // If the queue is empty, we are done. If not, set self as the parent and continue.
                if queue.is_empty() {
                    target_value = Some(value.clone());
                    break 'outer;
                }

                // Pop the queue and descend to the next set of child values.
                identifier = queue.pop_front().expect("provided empty queue");

                // Get the prop for the value. If we were not an element of a map or array, we
                // probably did this once already, but we avoid during upon _every_ iteration
                // since all map and array elements will share the same prop id.
                let prop = Prop::get_by_id(ctx, &value.prop_id())
                    .await
                    .expect("could not perform get by id")
                    .expect("prop not found");

                // Before we do anything else, if we are an array, let's prepare the target id in
                // advance using the newly popped identifier.
                if *prop.kind() == PropKind::Array {
                    let array_attribute_value = AttributeValue::get_by_id(ctx, &value_id.into())
                        .await
                        .expect("could not perform get by id")
                        .expect("attribute value not found");
                    let index_map = array_attribute_value.index_map.expect("no index map found");
                    let index: usize = identifier
                        .parse()
                        .expect("could not convert identifier into index");
                    array_element_attribute_value_id = index_map.order()[index];
                }

                // Now, ensure we cache the parent information.
                parent_value_id = value_id;
                parent_kind = *prop.kind();

                // Wipe the current set of child values and extend with the new set.
                #[allow(clippy::eq_op)]
                work_queue.retain(|&v| v != v);
                work_queue.extend(
                    property_values
                        .child_values
                        .get(&value_id)
                        .expect("could not get child values"),
                );
            }
        }

        (
            parent_value_id.into(),
            target_value.expect("value not found"),
        )
    }

    // Update a "value" for a given path and [`Component`](dal::Component).
    pub async fn update_value(
        &self,
        ctx: &DalContext,
        component_id: ComponentId,
        path: &[&str],
        value: Option<Value>,
    ) {
        let (parent_attribute_value_id, property_value) =
            self.find_value(ctx, component_id, path).await;
        let request = UpdatePropertyEditorValueRequest {
            attribute_value_id: property_value.attribute_value_id(),
            parent_attribute_value_id: Some(parent_attribute_value_id),
            prop_id: property_value.prop_id(),
            component_id,
            value,
            key: property_value.key.clone(),
            visibility: *ctx.visibility(),
        };
        let response: UpdatePropertyEditorValueResponse = self
            .query_post("/api/component/update_property_editor_value", &request)
            .await;
        assert!(response.success)
    }

    /// Insert a "value" into a map or an array corresponding to a given path and
    /// [`Component`](dal::Component).
    pub async fn insert_value(
        &self,
        ctx: &DalContext,
        component_id: ComponentId,
        path: &[&str],
        value: Option<Value>,
    ) {
        let (_, property_value) = self.find_value(ctx, component_id, path).await;
        let request = InsertPropertyEditorValueRequest {
            parent_attribute_value_id: property_value.attribute_value_id(),
            prop_id: property_value.prop_id(),
            component_id,
            value,
            key: property_value.key.clone(),
            visibility: *ctx.visibility(),
        };
        let response: InsertPropertyEditorValueResponse = self
            .query_post("/api/component/insert_property_editor_value", &request)
            .await;
        assert!(response.success)
    }

    /// Get the latest [`PropertyValues`] for a given [`Component`](dal::Component).
    async fn get_values(&self, ctx: &DalContext, component_id: ComponentId) -> PropertyValues {
        let request = GetPropertyEditorValuesRequest {
            component_id,
            visibility: *ctx.visibility(),
        };
        let response: GetPropertyEditorValuesResponse = self
            .query_get("/api/component/get_property_editor_values", &request)
            .await;
        response
    }

    /// Create a "connection" between two [`Nodes`](dal::Node) via a matching
    /// [`Socket`](dal::Socket).
    pub async fn create_connection(
        &self,
        ctx: &DalContext,
        source_node_id: NodeId,
        destination_node_id: NodeId,
        shared_socket_name: &str,
    ) {
        let source_socket = Socket::find_by_name_for_edge_kind_and_node(
            ctx,
            shared_socket_name,
            SocketEdgeKind::ConfigurationOutput,
            source_node_id,
        )
        .await
        .expect("could not perform query")
        .expect("could not find socket");
        let destination_socket = Socket::find_by_name_for_edge_kind_and_node(
            ctx,
            shared_socket_name,
            SocketEdgeKind::ConfigurationInput,
            destination_node_id,
        )
        .await
        .expect("could not perform query")
        .expect("could not find socket");

        let request = CreateConnectionRequest {
            from_node_id: source_node_id,
            from_socket_id: *source_socket.id(),
            to_node_id: destination_node_id,
            to_socket_id: *destination_socket.id(),
            visibility: *ctx.visibility(),
        };
        let _response: CreateConnectionResponse = self
            .query_post("/api/diagram/create_connection", &request)
            .await;
    }

    /// Create a [`Component`](dal::Component) and [`Node`](dal::Node) for a given
    /// [`Schema`](dal::Schema). Optionally "place" the [`Node`](dal::Node) into a "frame".
    pub async fn create_node(
        &mut self,
        ctx: &DalContext,
        schema_name: &str,
        frame_node_id: Option<NodeId>,
    ) -> ComponentBag {
        let schema_id = *self
            .builtins
            .get(schema_name)
            .expect("could not find schema by name");
        let request = CreateNodeRequest {
            schema_id,
            parent_id: frame_node_id,
            x: "0".to_string(),
            y: "0".to_string(),
            visibility: *ctx.visibility(),
        };
        let create_node_response: CreateNodeResponse =
            self.query_post("/api/diagram/create_node", &request).await;
        create_node_response.into()
    }

    /// Create the [`ChangeSet`](dal::ChangeSet) based on the provided [`context`](dal::DalContext).
    pub async fn create_change_set_and_update_ctx(
        &self,
        ctx: &mut DalContext,
        change_set_name: impl Into<String>,
    ) {
        let request = CreateChangeSetRequest {
            change_set_name: change_set_name.into(),
        };
        let response: CreateChangeSetResponse = self
            .query_post("/api/change_set/create_change_set", &request)
            .await;
        ctx.update_visibility(Visibility::new(response.change_set.pk, None));
        assert!(!ctx.visibility().is_head());
    }

    /// Apply the [`ChangeSet`](dal::ChangeSet) based on the provided [`context`](dal::DalContext).
    pub async fn apply_change_set_and_update_ctx_visibility_to_head(&self, ctx: &mut DalContext) {
        assert!(!ctx.visibility().is_head());
        let request = ApplyChangeSetRequest {
            change_set_pk: ctx.visibility().change_set_pk,
        };
        let _response: ApplyChangeSetResponse = self
            .query_post("/api/change_set/apply_change_set", &request)
            .await;
        ctx.update_visibility(Visibility::new_head(false));
        assert!(ctx.visibility().is_head());
    }

    pub async fn list_confirmations(&self, ctx: &mut DalContext) -> Vec<ConfirmationView> {
        let request = ConfirmationsRequest {
            visibility: *ctx.visibility(),
        };
        let response: ConfirmationsResponse =
            self.query_get("/api/fix/confirmations", &request).await;
        response
    }

    pub async fn run_fixes(&self, ctx: &mut DalContext, fixes: Vec<FixRunRequest>) -> FixBatchId {
        let request = FixesRunRequest {
            list: fixes,
            visibility: *ctx.visibility(),
        };
        let response: FixesRunResponse = self.query_post("/api/fix/run", &request).await;
        response.id
    }

    pub async fn list_fixes(&self, ctx: &mut DalContext) -> Vec<BatchHistoryView> {
        let request = ListFixesRequest {
            visibility: *ctx.visibility(),
        };
        let response: ListFixesResponse = self.query_get("/api/fix/list", &request).await;
        response
    }

    /// Send a "GET" method query to the backend.
    async fn query_get<Req: Serialize, Res: DeserializeOwned>(
        &self,
        uri: impl AsRef<str>,
        request: &Req,
    ) -> Res {
        api_request_auth_query(self.app.clone(), uri, &self.auth_token, request).await
    }

    /// Send a "POST" method query to the backend.
    async fn query_post<Req: Serialize, Res: DeserializeOwned>(
        &self,
        uri: impl AsRef<str>,
        request: &Req,
    ) -> Res {
        api_request_auth_json_body(
            self.app.clone(),
            Method::POST,
            uri,
            &self.auth_token,
            request,
        )
        .await
    }
}
