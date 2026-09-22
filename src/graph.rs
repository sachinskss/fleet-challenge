use crate::models::{EdgeId, Layout, Node, NodeId};
use std::collections::HashMap;

pub type NodeIndex = usize;
pub type EdgeIndex = usize;

#[derive(Debug, Clone)]
pub struct Graph {
    nodes: Vec<Node>,
    node_indices: HashMap<NodeId, NodeIndex>,
    edges: Vec<GraphEdge>,
    adjacency: Vec<Vec<EdgeIndex>>,
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub id: EdgeId,
    pub source: NodeIndex,
    pub sink: NodeIndex,
    pub weight: f64,
}

impl Graph {
    pub(crate) fn from_validated_layout(layout: &Layout) -> Self {
        let nodes = layout.nodes.clone();
        let node_indices = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), index))
            .collect::<HashMap<_, _>>();
        let mut adjacency = vec![Vec::new(); nodes.len()];
        let mut edges = Vec::with_capacity(layout.edges.len());

        for edge in &layout.edges {
            let source = node_indices[&edge.source];
            let sink = node_indices[&edge.sink];
            let source_position = &nodes[source].position;
            let sink_position = &nodes[sink].position;
            let weight = ((sink_position.x - source_position.x).powi(2)
                + (sink_position.y - source_position.y).powi(2))
            .sqrt();
            let edge_index = edges.len();
            edges.push(GraphEdge {
                id: edge.id.clone(),
                source,
                sink,
                weight,
            });
            adjacency[source].push(edge_index);
        }

        Self {
            nodes,
            node_indices,
            edges,
            adjacency,
        }
    }

    pub(crate) fn node_index(&self, id: &str) -> Option<NodeIndex> {
        self.node_indices.get(id).copied()
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn node_id(&self, index: NodeIndex) -> &NodeId {
        &self.nodes[index].id
    }

    pub(crate) fn outgoing_edges(&self, index: NodeIndex) -> &[EdgeIndex] {
        &self.adjacency[index]
    }

    pub(crate) fn edge(&self, index: EdgeIndex) -> &GraphEdge {
        &self.edges[index]
    }
}
