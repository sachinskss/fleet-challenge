use crate::models::{Edge, EdgeId, Layout, Node, NodeId};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Graph {
    pub nodes: HashMap<NodeId, Node>,
    pub edges: HashMap<EdgeId, Edge>,
    pub adjacency: HashMap<NodeId, Vec<EdgeId>>,
    pub edge_weights: HashMap<EdgeId, f64>,
}

impl Graph {
    pub fn from_layout(layout: &Layout) -> Self {
        let nodes: HashMap<NodeId, Node> = layout
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.clone()))
            .collect();
        let edges: HashMap<EdgeId, Edge> = layout
            .edges
            .iter()
            .map(|edge| (edge.id.clone(), edge.clone()))
            .collect();
        let mut adjacency: HashMap<NodeId, Vec<EdgeId>> = layout
            .nodes
            .iter()
            .map(|node| (node.id.clone(), Vec::new()))
            .collect();
        let edge_weights: HashMap<EdgeId, f64> = layout
            .edges
            .iter()
            .map(|edge| {
                let source = &nodes[&edge.source];
                let target = &nodes[&edge.sink];
                let weight = ((target.position.x - source.position.x).powi(2)
                    + (target.position.y - source.position.y).powi(2))
                .sqrt();
                (edge.id.clone(), weight)
            })
            .collect();

        for edge in &layout.edges {
            adjacency
                .get_mut(&edge.source)
                .expect("Graph::from_layout requires a validated layout")
                .push(edge.id.clone());
        }

        Self {
            nodes,
            edges,
            adjacency,
            edge_weights,
        }
    }
}
