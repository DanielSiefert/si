//! Create a [`SchemaVariant`](crate::SchemaVariant) with a [`Prop`](crate::Prop) tree via a
//! [`SchemaVariantDefinition`], stored in the database.

use async_recursion::async_recursion;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use telemetry::prelude::*;
use thiserror::Error;

use crate::schema::variant::{SchemaVariantError, SchemaVariantResult};
use crate::SchemaError;
use crate::{
    component::ComponentKind, edit_field::widget::WidgetKind, impl_standard_model, pk,
    standard_model, standard_model_accessor, DalContext, ExternalProvider, Func, HistoryEventError,
    InternalProvider, NatsError, PgError, Prop, PropId, PropKind, RootProp, Schema, SchemaVariant,
    SocketArity, StandardModel, StandardModelError, Tenancy, Timestamp, Visibility,
};

#[derive(Error, Debug)]
pub enum SchemaVariantDefinitionError {
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
    #[error("{0} is not a valid hex color string")]
    InvalidHexColor(String),
}

pub type SchemaVariantDefinitionResult<T> = Result<T, SchemaVariantDefinitionError>;

/// A cache of [`PropIds`](crate::Prop) where the _key_ is a tuple corresponding to the
/// [`Prop`](crate::Prop) name and the _parent_ [`PropId`](crate::Prop) who's child is the
/// [`PropId`](crate::Prop) in the _value_ of the entry.
///
/// It is recommended to start with the [`RootProp`](crate::RootProp) in order to descend into the
/// cache.
#[derive(Debug, Clone)]
pub struct PropCache(HashMap<(String, PropId), PropId>);

impl PropCache {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Attempts to retrieve the [`PropId`](crate::Prop) value for a given [`Prop`](crate::Prop)
    /// name and parent [`PropId`](crate::Prop) key tuple. An error is returned if nothing is found.
    pub fn get(
        &self,
        prop_name: impl AsRef<str>,
        parent_prop_id: PropId,
    ) -> SchemaVariantResult<PropId> {
        // NOTE(nick): the string handling could probably be better here.
        let prop_name = prop_name.as_ref().to_string();
        let prop_id = *self.0.get(&(prop_name.clone(), parent_prop_id)).ok_or(
            SchemaVariantError::PropNotFoundInCache(prop_name, parent_prop_id),
        )?;
        Ok(prop_id)
    }

    /// Insert the [`PropId`](crate::Prop) into [`self`](Self). The returned `option` from the
    /// underlying method is ignored.
    pub fn insert(&mut self, key: (String, PropId), value: PropId) {
        self.0.insert(key, value);
    }
}

impl Default for PropCache {
    fn default() -> Self {
        Self::new()
    }
}

pub fn hex_color_to_i64(color: &str) -> SchemaVariantDefinitionResult<i64> {
    let bytes: Vec<u8> = match hex::decode(color) {
        Ok(bytes) => bytes,
        Err(_) => Err(SchemaVariantDefinitionError::InvalidHexColor(
            color.to_string(),
        ))?,
    };

    if bytes.len() != 3 {
        return Err(SchemaVariantDefinitionError::InvalidHexColor(
            color.to_string(),
        ));
    }

    Ok(((bytes[0] as i64) << 16) + ((bytes[1] as i64) << 8) + bytes[2] as i64)
}

pk!(SchemaVariantDefinitionPk);
pk!(SchemaVariantDefinitionId);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct SchemaVariantDefinition {
    pk: SchemaVariantDefinitionPk,
    id: SchemaVariantDefinitionId,
    #[serde(flatten)]
    tenancy: Tenancy,
    #[serde(flatten)]
    timestamp: Timestamp,
    #[serde(flatten)]
    visibility: Visibility,

    /// Name for this variant. Actually, this is the name for this [`Schema`](crate::Schema), we're
    /// punting on the issue of multiple variants for the moment.
    name: String,
    /// Override for the UI name for this schema
    menu_name: Option<String>,
    /// The category this schema variant belongs to
    category: String,
    /// The color for the component on the component diagram as a hex string
    color: String,
    component_kind: ComponentKind,
    link: Option<String>,
    definition: String,
}

impl_standard_model! {
    model: SchemaVariantDefinition,
    pk: SchemaVariantDefinitionPk,
    id: SchemaVariantDefinitionId,
    table_name: "schema_variant_definitions",
    history_event_label_base: "schema_variant_definition",
    history_event_message_name: "Schema Variant Definition",
}

impl SchemaVariantDefinition {
    #[instrument(skip_all)]
    pub async fn new_from_structs(
        ctx: &DalContext,
        metadata: SchemaVariantDefinitionMetadataJson,
        definition: SchemaVariantDefinitionJson,
    ) -> SchemaVariantDefinitionResult<SchemaVariantDefinition> {
        SchemaVariantDefinition::new(
            ctx,
            metadata.name,
            metadata.menu_name,
            metadata.category,
            metadata.link,
            metadata.color,
            metadata.component_kind,
            serde_json::to_string(&definition)?,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        ctx: &DalContext,
        name: String,
        menu_name: Option<String>,
        category: String,
        link: Option<String>,
        color: String,
        component_kind: ComponentKind,
        definition: String,
    ) -> SchemaVariantDefinitionResult<SchemaVariantDefinition> {
        let row = ctx.txns()
            .pg()
            .query_one(
                "SELECT object FROM schema_variant_definition_create_v1($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                ctx.tenancy(),
                ctx.visibility(),
                &name,
                &menu_name,
                &category,
                &link,
                &color,
                &component_kind.as_ref(),
                &definition,
                ]
            ).await?;

        Ok(standard_model::finish_create_from_row(ctx, row).await?)
    }

    standard_model_accessor!(name, String, SchemaVariantDefinitionResult);
    standard_model_accessor!(menu_name, Option<String>, SchemaVariantDefinitionResult);
    standard_model_accessor!(category, String, SchemaVariantDefinitionResult);
    standard_model_accessor!(color, String, SchemaVariantDefinitionResult);
    standard_model_accessor!(
        component_kind,
        Enum(ComponentKind),
        SchemaVariantDefinitionResult
    );
    standard_model_accessor!(link, Option<String>, SchemaVariantDefinitionResult);
    standard_model_accessor!(definition, String, SchemaVariantDefinitionResult);
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaVariantDefinitionMetadataJson {
    /// Name for this variant. Actually, this is the name for this [`Schema`](crate::Schema), we're
    /// punting on the issue of multiple variants for the moment.
    pub name: String,
    /// Override for the UI name for this schema
    #[serde(alias = "menu_name")]
    pub menu_name: Option<String>,
    /// The category this schema variant belongs to
    pub category: String,
    /// The color for the component on the component diagram as a hex string
    pub color: String,
    #[serde(alias = "component_kind")]
    pub component_kind: ComponentKind,
    pub link: Option<String>,
}

impl From<SchemaVariantDefinition> for SchemaVariantDefinitionMetadataJson {
    fn from(value: SchemaVariantDefinition) -> Self {
        SchemaVariantDefinitionMetadataJson {
            name: value.name,
            menu_name: value.menu_name,
            category: value.category,
            color: value.color,
            component_kind: value.component_kind,
            link: value.link,
        }
    }
}

impl From<&SchemaVariantDefinition> for SchemaVariantDefinitionMetadataJson {
    fn from(value: &SchemaVariantDefinition) -> Self {
        SchemaVariantDefinitionMetadataJson {
            name: value.name.clone(),
            menu_name: value.menu_name.clone(),
            category: value.category.clone(),
            color: value.color.clone(),
            component_kind: value.component_kind,
            link: value.link.clone(),
        }
    }
}

impl SchemaVariantDefinitionMetadataJson {
    #[instrument(skip_all)]
    pub fn new(
        name: impl AsRef<str>,
        menu_name: Option<&str>,
        category: impl AsRef<str>,
        color: impl AsRef<str>,
        component_kind: ComponentKind,
        link: Option<&str>,
    ) -> SchemaVariantDefinitionMetadataJson {
        SchemaVariantDefinitionMetadataJson {
            name: name.as_ref().to_string(),
            menu_name: menu_name.map(|s| s.to_string()),
            category: category.as_ref().to_string(),
            color: color.as_ref().to_string(),
            component_kind,
            link: link.map(|l| l.to_string()),
        }
    }

    pub fn color_as_i64(&self) -> SchemaVariantDefinitionResult<i64> {
        hex_color_to_i64(&self.color)
    }
}

/// The definition for a [`SchemaVariant`](crate::SchemaVariant)'s [`Prop`](crate::Prop) tree (and
/// more in the future).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaVariantDefinitionJson {
    /// The immediate child [`Props`](crate::Prop) underneath "/root/domain".
    #[serde(default)]
    props: Vec<PropDefinition>,
    /// The input [`Sockets`](crate::Socket) and corresponding
    /// explicit [`InternalProviders`](crate::InternalProvider) created for the
    /// [`variant`](crate::SchemaVariant).
    #[serde(default)]
    input_sockets: Vec<SocketDefinition>,
    /// The output [`Sockets`](crate::Socket) and corresponding
    /// [`ExternalProviders`](crate::ExternalProvider) created for the
    /// [`variant`](crate::SchemaVariant).
    #[serde(default)]
    output_sockets: Vec<SocketDefinition>,
    /// A map of documentation links to reference. To reference links (values) specify the key via
    /// the "doc_link_ref" field for a [`PropDefinition`].
    doc_links: Option<HashMap<String, String>>,
}

impl TryFrom<SchemaVariantDefinition> for SchemaVariantDefinitionJson {
    type Error = SchemaVariantDefinitionError;

    fn try_from(value: SchemaVariantDefinition) -> Result<Self, Self::Error> {
        Ok(serde_json::from_str(&value.definition)?)
    }
}

impl TryFrom<&SchemaVariantDefinition> for SchemaVariantDefinitionJson {
    type Error = SchemaVariantDefinitionError;

    fn try_from(value: &SchemaVariantDefinition) -> Result<Self, Self::Error> {
        Ok(serde_json::from_str(&value.definition)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropWidgetDefinition {
    /// The [`kind`](crate::edit_field::widget::WidgetKind) of the [`Prop`](crate::Prop) to be created.
    kind: WidgetKind,
    /// The `Option<Value>` of the [`kind`](crate::edit_field::widget::WidgetKind) to be created.
    #[serde(default)]
    options: Option<Value>,
}

/// The definition for a [`Prop`](crate::Prop) in a [`SchemaVariant`](crate::SchemaVariant).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropDefinition {
    /// The name of the [`Prop`](crate::Prop) to be created.
    name: String,
    /// The [`kind`](crate::PropKind) of the [`Prop`](crate::Prop) to be created.
    kind: PropKind,
    /// An optional reference to a documentation link in the "doc_links" field for the
    /// [`SchemaVariantDefinitionJson`] for the [`Prop`](crate::Prop) to be created.
    doc_link_ref: Option<String>,
    /// An optional documentation link for the [`Prop`](crate::Prop) to be created.
    doc_link: Option<String>,
    /// If our [`kind`](crate::PropKind) is [`Object`](crate::PropKind::Object), specify the
    /// child definition(s).
    #[serde(default)]
    children: Vec<PropDefinition>,
    /// If our [`kind`](crate::PropKind) is [`Array`](crate::PropKind::Array), specify the entry
    /// definition.
    entry: Option<Box<PropDefinition>>,
    /// The [`WidgetDefinition`](crate::schema::variant::definition::PropWidgetDefinition) of the
    /// [`Prop`](crate::Prop) to be created.
    #[serde(default)]
    widget: Option<PropWidgetDefinition>,
}

/// The definition for a [`Socket`](crate::Socket) in a [`SchemaVariant`](crate::SchemaVariant).
/// A corresponding [`provider`](crate::provider) will be created as well.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketDefinition {
    /// The name of the [`Socket`](crate::Socket) to be created.
    name: String,
    /// The [`arity`](https://en.wikipedia.org/wiki/Arity) of the [`Socket`](crate::Socket).
    /// Defaults to [`SocketArity::Many`](crate::SocketArity::Many) if nothing is provided.
    arity: Option<SocketArity>,
}

// Not sure if this fits here still
impl SchemaVariant {
    /// Create a [`SchemaVariant`] like [`usual`](Self::new()), but use the
    /// [`SchemaVariantDefinition`] to create a [`Prop`](crate::Prop) tree as well with a
    /// [`cache`](PropCache).
    pub async fn new_with_definition(
        ctx: &DalContext,
        schema_variant_definition_metadata: SchemaVariantDefinitionMetadataJson,
        schema_variant_definition: SchemaVariantDefinitionJson,
    ) -> SchemaVariantResult<(
        Self,
        RootProp,
        PropCache,
        Vec<InternalProvider>,
        Vec<ExternalProvider>,
    )> {
        let variant_name = "v0".to_string();

        let schema_name = schema_variant_definition_metadata.name.clone();

        let schema_id = match Schema::schema_for_name(ctx, &schema_name).await {
            Ok(schema) => *schema.id(),
            Err(SchemaError::NotFoundByName(_)) => {
                let schema = Schema::new(ctx, &schema_name, &ComponentKind::Standard)
                    .await
                    .map_err(Box::new)?;
                *schema.id()
            }
            Err(e) => Err(Box::new(e))?,
        };

        let (mut schema_variant, root_prop) = Self::new(ctx, schema_id, variant_name).await?;
        let schema_variant_id = *schema_variant.id();

        // NOTE(nick): allow users to use a definition without props... just in case, I guess.
        let mut prop_cache = PropCache::new();
        let doc_links = schema_variant_definition
            .doc_links
            .clone()
            .unwrap_or_default();
        for prop_definition in schema_variant_definition.props {
            Self::walk_definition(
                ctx,
                &mut prop_cache,
                prop_definition,
                root_prop.domain_prop_id,
                &doc_links,
            )
            .await?;
        }

        // Only find the identity func if we have sockets to create.
        // FIXME(nick,wendy): allow other funcs to be specified in the definition manifest(s).
        let mut explicit_internal_providers = Vec::new();
        let mut external_providers = Vec::new();

        if !schema_variant_definition.input_sockets.is_empty()
            || !schema_variant_definition.output_sockets.is_empty()
        {
            let (identity_func, identity_func_binding, identity_func_binding_return_value) =
                Func::identity_with_binding_and_return_value(ctx).await?;
            let identity_func_id = *identity_func.id();
            let identity_func_binding_id = *identity_func_binding.id();
            let identity_func_binding_return_value_id = *identity_func_binding_return_value.id();

            for input_socket_definition in schema_variant_definition.input_sockets {
                let arity = match input_socket_definition.arity {
                    Some(found_arity) => found_arity,
                    None => SocketArity::Many,
                };
                let (explicit_internal_provider, _) = InternalProvider::new_explicit_with_socket(
                    ctx,
                    schema_variant_id,
                    input_socket_definition.name,
                    identity_func_id,
                    identity_func_binding_id,
                    identity_func_binding_return_value_id,
                    arity,
                    false,
                )
                .await?;
                explicit_internal_providers.push(explicit_internal_provider);
            }

            for output_socket_definition in schema_variant_definition.output_sockets {
                let arity = match output_socket_definition.arity {
                    Some(found_arity) => found_arity,
                    None => SocketArity::Many,
                };
                let (external_provider, _) = ExternalProvider::new_with_socket(
                    ctx,
                    schema_id,
                    schema_variant_id,
                    output_socket_definition.name,
                    None,
                    identity_func_id,
                    identity_func_binding_id,
                    identity_func_binding_return_value_id,
                    arity,
                    false,
                )
                .await?;
                external_providers.push(external_provider);
            }
        }

        schema_variant
            .set_color(
                ctx,
                Some(schema_variant_definition_metadata.color_as_i64()?),
            )
            .await?;

        Ok((
            schema_variant,
            root_prop,
            prop_cache,
            explicit_internal_providers,
            external_providers,
        ))
    }

    /// A recursive walk of [`PropDefinition`] that populates the [`cache`](PropCache) as each
    /// [`Prop`](crate::Prop) is created.
    #[async_recursion]
    async fn walk_definition(
        ctx: &DalContext,
        prop_cache: &mut PropCache,
        definition: PropDefinition,
        parent_prop_id: PropId,
        doc_links: &HashMap<String, String>,
    ) -> SchemaVariantResult<()> {
        // Start by creating the prop and setting the parent. We cache the id for later.
        let widget = match definition.widget {
            Some(widget) => Some((widget.kind, widget.options)),
            None => None,
        };
        let mut prop = Prop::new(ctx, definition.name.clone(), definition.kind, widget).await?;
        prop.set_parent_prop(ctx, parent_prop_id).await?;
        let prop_id = *prop.id();

        // Always cache the prop that was created.
        prop_cache.insert((prop.name().to_string(), parent_prop_id), prop_id);

        // Either use the doc link or the doc link ref. Do not use both.
        match (definition.doc_link.is_some(), definition.doc_link_ref) {
            (true, Some(_)) => {
                return Err(SchemaVariantError::MultipleDocLinksProvided(
                    definition.name.clone(),
                ));
            }
            (true, None) => prop.set_doc_link(ctx, definition.doc_link).await?,
            (false, Some(doc_link_ref)) => match doc_links.get(&doc_link_ref) {
                Some(link) => prop.set_doc_link(ctx, Some(link)).await?,
                None => return Err(SchemaVariantError::LinkNotFoundForDocLinkRef(doc_link_ref)),
            },
            (false, None) => {}
        }

        // Determine if we need to descend and check the "entry" and "children" fields accordingly.
        match definition.kind {
            PropKind::Object => {
                if definition.entry.is_some() {
                    return Err(SchemaVariantError::FoundEntryForObject(
                        definition.name.clone(),
                    ));
                }
                if definition.children.is_empty() {
                    return Err(SchemaVariantError::MissingChildrenForObject(
                        definition.name.clone(),
                    ));
                }
                for child in definition.children {
                    Self::walk_definition(ctx, prop_cache, child, prop_id, doc_links).await?;
                }
            }
            PropKind::Array => match definition.entry {
                Some(entry) => {
                    if !definition.children.is_empty() {
                        return Err(SchemaVariantError::FoundChildrenForArray(
                            definition.name.clone(),
                        ));
                    }
                    Self::walk_definition(ctx, prop_cache, *entry, prop_id, doc_links).await?;
                }
                None => {
                    return Err(SchemaVariantError::MissingEntryForArray(
                        definition.name.clone(),
                    ));
                }
            },
            PropKind::Map => todo!("maps not yet implemented simply because nick didn't need them yet and didn't want an untested solution"),
            _ => match (definition.entry.is_none(), definition.children.is_empty()) {
                (false, false) => {
                    return Err(SchemaVariantError::FoundChildrenAndEntryForPrimitive(
                        definition.name.clone(),
                    ));
                }
                (false, true) => {
                    return Err(SchemaVariantError::FoundEntryForPrimitive(
                        definition.name.clone(),
                    ));
                }
                (true, false) => {
                    return Err(SchemaVariantError::FoundChildrenForPrimitive(
                        definition.name.clone(),
                    ));
                }
                (true, true) => {}
            },
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hex_colors() {
        let colors: [&str; 5] = ["ababab", "ffffff", "caffed", "00ff00", "badf00"];

        for hex_color in colors {
            assert_eq!(
                *hex_color.to_string(),
                format!(
                    "{:06x}",
                    hex_color_to_i64(hex_color).expect("able to convert hex")
                )
            );
        }
    }
}
