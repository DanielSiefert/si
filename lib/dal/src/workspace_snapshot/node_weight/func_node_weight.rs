use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use si_events::{merkle_tree_hash::MerkleTreeHash, ulid::Ulid, ContentHash};

use crate::func::FuncKind;
use crate::workspace_snapshot::content_address::ContentAddressDiscriminants;
use crate::workspace_snapshot::vector_clock::VectorClockId;
use crate::EdgeWeightKindDiscriminants;
use crate::{
    change_set::ChangeSet,
    workspace_snapshot::{
        content_address::ContentAddress,
        graph::LineageId,
        node_weight::{NodeWeightError, NodeWeightResult},
        vector_clock::VectorClock,
    },
};

#[derive(Clone, Serialize, Deserialize)]
pub struct FuncNodeWeight {
    id: Ulid,
    lineage_id: LineageId,
    content_address: ContentAddress,
    merkle_tree_hash: MerkleTreeHash,
    vector_clock_first_seen: VectorClock,
    vector_clock_recently_seen: VectorClock,
    vector_clock_write: VectorClock,
    name: String,
    func_kind: FuncKind,
}

impl FuncNodeWeight {
    pub fn new(
        change_set: &ChangeSet,
        id: Ulid,
        content_address: ContentAddress,
        name: String,
        func_kind: FuncKind,
    ) -> NodeWeightResult<Self> {
        Ok(Self {
            id,
            lineage_id: change_set.generate_ulid()?,
            content_address,
            merkle_tree_hash: MerkleTreeHash::default(),
            name,
            func_kind,
            vector_clock_first_seen: VectorClock::new(change_set.vector_clock_id())?,
            vector_clock_recently_seen: VectorClock::new(change_set.vector_clock_id())?,
            vector_clock_write: VectorClock::new(change_set.vector_clock_id())?,
        })
    }

    pub fn content_address(&self) -> ContentAddress {
        self.content_address
    }

    pub fn content_hash(&self) -> ContentHash {
        self.content_address.content_hash()
    }

    pub fn content_store_hashes(&self) -> Vec<ContentHash> {
        vec![self.content_address.content_hash()]
    }

    pub fn id(&self) -> Ulid {
        self.id
    }

    pub fn increment_vector_clock(&mut self, change_set: &ChangeSet) -> NodeWeightResult<()> {
        self.vector_clock_write.inc(change_set.vector_clock_id())?;
        self.vector_clock_recently_seen
            .inc(change_set.vector_clock_id())?;

        Ok(())
    }

    pub fn lineage_id(&self) -> Ulid {
        self.lineage_id
    }

    pub fn mark_seen_at(&mut self, vector_clock_id: VectorClockId, seen_at: DateTime<Utc>) {
        self.vector_clock_recently_seen
            .inc_to(vector_clock_id, seen_at);
        if self
            .vector_clock_first_seen
            .entry_for(vector_clock_id)
            .is_none()
        {
            self.vector_clock_first_seen
                .inc_to(vector_clock_id, seen_at);
        }
    }

    pub fn merge_clocks(&mut self, change_set: &ChangeSet, other: &Self) -> NodeWeightResult<()> {
        self.vector_clock_write
            .merge(change_set.vector_clock_id(), &other.vector_clock_write)?;
        self.vector_clock_first_seen
            .merge(change_set.vector_clock_id(), &other.vector_clock_first_seen)?;
        self.vector_clock_recently_seen.merge(
            change_set.vector_clock_id(),
            &other.vector_clock_recently_seen,
        )?;

        Ok(())
    }

    pub fn merkle_tree_hash(&self) -> MerkleTreeHash {
        self.merkle_tree_hash
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = name.into();
        self
    }

    pub fn func_kind(&self) -> FuncKind {
        self.func_kind
    }

    pub fn set_func_kind(&mut self, func_kind: FuncKind) -> &mut Self {
        self.func_kind = func_kind;
        self
    }

    pub fn new_content_hash(&mut self, content_hash: ContentHash) -> NodeWeightResult<()> {
        let new_address = match &self.content_address {
            ContentAddress::Func(_) => ContentAddress::Func(content_hash),
            other => {
                return Err(NodeWeightError::InvalidContentAddressForWeightKind(
                    Into::<ContentAddressDiscriminants>::into(other).to_string(),
                    ContentAddressDiscriminants::Func.to_string(),
                ));
            }
        };

        self.content_address = new_address;

        Ok(())
    }

    pub fn new_with_incremented_vector_clock(
        &self,
        change_set: &ChangeSet,
    ) -> NodeWeightResult<Self> {
        let mut new_node_weight = self.clone();
        new_node_weight.increment_vector_clock(change_set)?;

        Ok(new_node_weight)
    }

    pub fn node_hash(&self) -> ContentHash {
        ContentHash::from(&serde_json::json![{
            "content_address": self.content_address,
            "name": self.name,
            "func_kind": self.func_kind,
        }])
    }

    pub fn set_merkle_tree_hash(&mut self, new_hash: MerkleTreeHash) {
        self.merkle_tree_hash = new_hash;
    }

    pub fn set_vector_clock_recently_seen_to(
        &mut self,
        change_set: &ChangeSet,
        new_val: DateTime<Utc>,
    ) {
        self.vector_clock_recently_seen
            .inc_to(change_set.vector_clock_id(), new_val);
    }

    pub fn vector_clock_first_seen(&self) -> &VectorClock {
        &self.vector_clock_first_seen
    }

    pub fn vector_clock_recently_seen(&self) -> &VectorClock {
        &self.vector_clock_recently_seen
    }

    pub fn vector_clock_write(&self) -> &VectorClock {
        &self.vector_clock_write
    }

    pub fn vector_clock_first_seen_mut(&mut self) -> &mut VectorClock {
        &mut self.vector_clock_first_seen
    }

    pub fn vector_clock_recently_seen_mut(&mut self) -> &mut VectorClock {
        &mut self.vector_clock_recently_seen
    }

    pub fn vector_clock_write_mut(&mut self) -> &mut VectorClock {
        &mut self.vector_clock_write
    }

    pub const fn exclusive_outgoing_edges(&self) -> &[EdgeWeightKindDiscriminants] {
        &[]
    }
}

impl std::fmt::Debug for FuncNodeWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("FuncNodeWeight")
            .field("id", &self.id().to_string())
            .field("lineage_id", &self.lineage_id.to_string())
            .field("name", &self.name)
            .field("func_kind", &self.func_kind)
            .field("content_hash", &self.content_hash())
            .field("merkle_tree_hash", &self.merkle_tree_hash)
            .field("vector_clock_first_seen", &self.vector_clock_first_seen)
            .field(
                "vector_clock_recently_seen",
                &self.vector_clock_recently_seen,
            )
            .field("vector_clock_write", &self.vector_clock_write)
            .finish()
    }
}
