use crate::{server::Error, Graph, Id};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default, Debug)]
pub struct ValueCreationQueue {
    processing: Option<String>,
    queue: VecDeque<String>,
}

impl ValueCreationQueue {
    pub fn push(&mut self, reply_channel: String) {
        self.queue.push_back(reply_channel);
    }

    pub fn is_busy(&self) -> bool {
        self.processing.is_some()
    }

    pub fn fetch_next(&mut self) -> Option<String> {
        if self.is_busy() {
            return None;
        }
        let next_channel = self.queue.pop_front();
        self.processing = next_channel.clone();

        next_channel
    }

    pub fn finished_processing(&mut self, reply_channel: &str) -> Result<(), Error> {
        if self.processing.as_deref() != Some(reply_channel) {
            return Err(Error::UnexpectedJobId);
        }

        self.processing = None;

        Ok(())
    }

    pub fn remove(&mut self, reply_channel: &str) {
        self.processing = self.processing.take().filter(|el| *el != reply_channel);
        self.queue.retain(|el| reply_channel != el);
    }
}

#[derive(Default, Debug)]
pub struct NodeMetadata {
    // This should really be an ordered set, to remove duplicates, but we'll deal with
    // that later.
    wanted_by_reply_channels: VecDeque<String>,
    processing_reply_channel: Option<String>,
    depends_on_node_ids: HashSet<Id>,
}

impl NodeMetadata {
    pub fn merge_metadata(&mut self, reply_channel: String, dependencies: &Vec<Id>) {
        if !self.wanted_by_reply_channels.contains(&reply_channel) {
            self.wanted_by_reply_channels.push_back(reply_channel);
        }
        self.depends_on_node_ids.extend(dependencies);
    }

    pub fn remove_dependency(&mut self, node_id: Id) {
        self.depends_on_node_ids.remove(&node_id);
    }

    pub fn next_to_process(&mut self) -> Option<String> {
        if self.depends_on_node_ids.is_empty() && self.processing_reply_channel.is_none() {
            self.processing_reply_channel = self.wanted_by_reply_channels.pop_front();
            return self.processing_reply_channel.clone();
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.wanted_by_reply_channels.is_empty() && self.processing_reply_channel.is_none()
    }

    pub fn remove_channel(&mut self, reply_channel: &str) {
        self.wanted_by_reply_channels
            .retain(|el| el != reply_channel);
        self.processing_reply_channel = self
            .processing_reply_channel
            .take()
            .filter(|el| el != reply_channel);
    }
}

#[derive(Default, Debug)]
pub struct ChangeSetGraph {
    dependency_data: HashMap<Id, HashMap<Id, NodeMetadata>>,
}

impl ChangeSetGraph {
    pub fn is_empty(&self) -> bool {
        self.dependency_data.is_empty()
    }

    pub fn fetch_all_available(&mut self) -> Vec<(String, Id)> {
        let mut result = Vec::new();
        for graph in self.dependency_data.values_mut() {
            for (id, metadata) in graph.iter_mut() {
                if let Some(reply_channel) = metadata.next_to_process() {
                    result.push((reply_channel, *id));
                }
            }
        }
        result
    }

    pub fn merge_dependency_graph(
        &mut self,
        reply_channel: String,
        new_dependency_data: Graph,
        change_set_id: Id,
    ) -> Result<(), Error> {
        let change_set_graph_data = self.dependency_data.entry(change_set_id).or_default();

        for (attribute_value_id, dependencies) in new_dependency_data {
            change_set_graph_data
                .entry(attribute_value_id)
                .and_modify(|node| {
                    node.merge_metadata(reply_channel.clone(), &dependencies);
                })
                .or_insert_with(|| {
                    let mut new_node = NodeMetadata::default();
                    new_node.merge_metadata(reply_channel.clone(), &dependencies);

                    new_node
                });

            for dependency in dependencies {
                change_set_graph_data
                    .entry(dependency)
                    .and_modify(|node| {
                        node.merge_metadata(reply_channel.clone(), &Vec::new());
                    })
                    .or_insert_with(|| {
                        let mut new_node = NodeMetadata::default();
                        new_node.merge_metadata(reply_channel.clone(), &Vec::new());

                        new_node
                    });
            }
        }

        Ok(())
    }

    pub fn mark_node_as_processed(
        &mut self,
        reply_channel: String,
        change_set_id: Id,
        node_id: Id,
    ) -> Result<VecDeque<String>, Error> {
        let change_set_graph_data = self.dependency_data.get_mut(&change_set_id).unwrap();

        let node_is_complete;
        if let Some(node_metadata) = change_set_graph_data.get_mut(&node_id) {
            if node_metadata.processing_reply_channel.as_ref() != Some(&reply_channel) {
                return Err(Error::ShouldNotBeProcessingByJob);
            }
            node_metadata.processing_reply_channel = None;

            node_metadata
                .wanted_by_reply_channels
                .retain(|x| x != &reply_channel);

            node_is_complete = node_metadata.depends_on_node_ids.is_empty();
        } else {
            return Err(Error::UnknownNodeId);
        }

        if node_is_complete {
            let node_metadata = change_set_graph_data.remove(&node_id).unwrap();

            for node_metadata in change_set_graph_data.values_mut() {
                node_metadata.remove_dependency(node_id);
            }

            if change_set_graph_data.is_empty() {
                self.dependency_data.remove(&change_set_id).unwrap();
            }

            return Ok(node_metadata.wanted_by_reply_channels);
        }

        Ok(VecDeque::new())
    }

    pub fn remove_channel(&mut self, change_set_id: Id, reply_channel: &str) {
        if let Some(graph) = self.dependency_data.get_mut(&change_set_id) {
            let mut to_remove = Vec::new();
            for (id, metadata) in graph.iter_mut() {
                metadata.remove_channel(reply_channel);
                if metadata.is_empty() {
                    to_remove.push(*id);
                }
            }

            for id in to_remove {
                graph.remove(&id).unwrap();
            }
        }
    }
}
