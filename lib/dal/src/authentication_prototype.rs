use serde::{Deserialize, Serialize};

use crate::{FuncId, SchemaVariantId};

pub use si_id::AuthenticationPrototypeId;

// TODO(nick): remove this once import can just create the edge.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct AuthenticationPrototype {
    pub id: AuthenticationPrototypeId,
    pub func_id: FuncId,
    pub schema_variant_id: SchemaVariantId,
}
