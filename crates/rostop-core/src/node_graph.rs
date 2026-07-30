//! Backend-neutral selected-topic topology derived from endpoint snapshots.

use std::collections::BTreeMap;

use crate::endpoint::{EndpointInfo, EndpointSets};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub namespace: String,
    pub name: String,
    pub endpoint_count: usize,
}

impl GraphNode {
    pub fn fully_qualified_name(&self) -> String {
        fully_qualified_name(&self.namespace, &self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphSide {
    Unavailable,
    Known(Vec<GraphNode>),
}

impl GraphSide {
    pub fn nodes(&self) -> Option<&[GraphNode]> {
        match self {
            Self::Unavailable => None,
            Self::Known(nodes) => Some(nodes),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedTopicGraph {
    pub publishers: GraphSide,
    pub subscribers: GraphSide,
}

impl SelectedTopicGraph {
    pub fn from_endpoints(endpoints: Option<&EndpointSets>) -> Self {
        let (publishers, subscribers) = endpoints
            .map(|(publishers, subscribers)| (publishers.as_deref(), subscribers.as_deref()))
            .unwrap_or((None, None));
        Self {
            publishers: graph_side(publishers),
            subscribers: graph_side(subscribers),
        }
    }
}

fn graph_side(endpoints: Option<&[EndpointInfo]>) -> GraphSide {
    let Some(endpoints) = endpoints else {
        return GraphSide::Unavailable;
    };
    let mut nodes: BTreeMap<(String, String), usize> = BTreeMap::new();
    for endpoint in endpoints {
        let namespace = normalize_namespace(&endpoint.node_namespace);
        let name = endpoint.node_name.trim_matches('/').to_string();
        *nodes.entry((namespace, name)).or_default() += 1;
    }
    GraphSide::Known(
        nodes
            .into_iter()
            .map(|((namespace, name), endpoint_count)| GraphNode {
                namespace,
                name,
                endpoint_count,
            })
            .collect(),
    )
}

fn normalize_namespace(namespace: &str) -> String {
    let trimmed = namespace.trim_matches('/');
    if trimmed.is_empty() {
        "/".into()
    } else {
        format!("/{trimmed}")
    }
}

fn fully_qualified_name(namespace: &str, name: &str) -> String {
    if namespace == "/" {
        format!("/{}", name.trim_matches('/'))
    } else {
        format!(
            "{}/{}",
            namespace.trim_end_matches('/'),
            name.trim_matches('/')
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::endpoint::{
        DurabilityKind, HistoryKind, LivelinessKind, QosSnapshot, ReliabilityKind,
    };

    fn endpoint(namespace: &str, name: &str, gid: u8) -> EndpointInfo {
        EndpointInfo {
            node_name: name.into(),
            node_namespace: namespace.into(),
            topic_type: "std_msgs/msg/String".into(),
            endpoint_gid: vec![gid],
            qos: QosSnapshot {
                reliability: ReliabilityKind::Reliable,
                durability: DurabilityKind::Volatile,
                history: HistoryKind::KeepLast,
                depth: 10,
                deadline: None,
                lifespan: None,
                liveliness: LivelinessKind::Automatic,
                liveliness_lease: Some(Duration::from_secs(1)),
            },
        }
    }

    #[test]
    fn deduplicates_endpoints_by_fully_qualified_node() {
        let sets = (
            Some(vec![
                endpoint("/robot/", "/camera", 1),
                endpoint("robot", "camera", 2),
                endpoint("/", "planner", 3),
            ]),
            Some(vec![]),
        );
        let graph = SelectedTopicGraph::from_endpoints(Some(&sets));
        let nodes = graph.publishers.nodes().unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].fully_qualified_name(), "/planner");
        assert_eq!(nodes[0].endpoint_count, 1);
        assert_eq!(nodes[1].fully_qualified_name(), "/robot/camera");
        assert_eq!(nodes[1].endpoint_count, 2);
    }

    #[test]
    fn preserves_known_empty_and_unavailable_sides() {
        let sets = (Some(vec![]), None);
        let graph = SelectedTopicGraph::from_endpoints(Some(&sets));
        assert_eq!(graph.publishers, GraphSide::Known(vec![]));
        assert_eq!(graph.subscribers, GraphSide::Unavailable);

        let missing = SelectedTopicGraph::from_endpoints(None);
        assert_eq!(missing.publishers, GraphSide::Unavailable);
        assert_eq!(missing.subscribers, GraphSide::Unavailable);
    }
}
