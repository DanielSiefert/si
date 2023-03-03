use std::io::{BufRead, Write};

use chrono::{DateTime, Utc};
use object_tree::{
    read_key_value_line, write_key_value_line, GraphError, NameStr, NodeChild, NodeKind,
    NodeWithChildren, ReadBytes, WriteBytes,
};

use crate::PkgSpec;

use super::{category::PackageCategory, PkgNode};

const KEY_CREATED_AT_STR: &str = "created_at";
const KEY_CREATED_BY_STR: &str = "created_by";
const KEY_DESCRIPTION_STR: &str = "description";
const KEY_NAME_STR: &str = "name";
const KEY_VERSION_STR: &str = "version";

#[derive(Clone, Debug)]
pub struct PackageNode {
    pub name: String,
    pub version: String,

    pub description: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

impl NameStr for PackageNode {
    fn name(&self) -> &str {
        &self.name
    }
}

impl WriteBytes for PackageNode {
    fn write_bytes<W: Write>(&self, writer: &mut W) -> Result<(), GraphError> {
        write_key_value_line(writer, KEY_NAME_STR, self.name())?;
        write_key_value_line(writer, KEY_VERSION_STR, &self.version)?;
        write_key_value_line(writer, KEY_DESCRIPTION_STR, &self.description)?;
        write_key_value_line(writer, KEY_CREATED_AT_STR, self.created_at.to_rfc3339())?;
        write_key_value_line(writer, KEY_CREATED_BY_STR, &self.created_by)?;
        Ok(())
    }
}

impl ReadBytes for PackageNode {
    fn read_bytes<R: BufRead>(reader: &mut R) -> Result<Self, GraphError>
    where
        Self: std::marker::Sized,
    {
        let name = read_key_value_line(reader, KEY_NAME_STR)?;
        let version = read_key_value_line(reader, KEY_VERSION_STR)?;
        let description = read_key_value_line(reader, KEY_DESCRIPTION_STR)?;
        let created_at_str = read_key_value_line(reader, KEY_CREATED_AT_STR)?;
        let created_at = created_at_str
            .parse::<DateTime<Utc>>()
            .map_err(GraphError::parse)?;
        let created_by = read_key_value_line(reader, KEY_CREATED_BY_STR)?;

        Ok(Self {
            name,
            version,
            description,
            created_at,
            created_by,
        })
    }
}

impl NodeChild for PkgSpec {
    type NodeType = PkgNode;

    fn as_node_with_children(&self) -> NodeWithChildren<Self::NodeType> {
        NodeWithChildren::new(
            NodeKind::Tree,
            Self::NodeType::Package(PackageNode {
                name: self.name.to_string(),
                version: self.version.to_string(),
                description: self.description.to_string(),
                created_at: self.created_at,
                created_by: self.created_by.clone(),
            }),
            vec![Box::new(PackageCategory::Schemas(self.schemas.clone()))
                as Box<dyn NodeChild<NodeType = Self::NodeType>>],
        )
    }
}
