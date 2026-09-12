use serde::{Deserialize, Serialize};

pub type NodeId = String;
pub type EdgeId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    /// Node id this edge starts at.
    pub source: NodeId,
    /// Node id this edge ends at.
    pub sink: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub id: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
