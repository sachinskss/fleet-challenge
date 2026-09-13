use crate::graph::Graph;
use crate::models::NodeId;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

type EdgeId = String;

#[derive(Debug, Serialize, PartialEq)]
pub struct RouteResult {
    /// Node ids visited, in travel order (includes start and goal).
    pub nodes: Vec<NodeId>,
    /// Edge ids traversed, in travel order. `edges.len() == nodes.len() - 1`.
    pub edges: Vec<EdgeId>,
    /// Total euclidean travel distance along the route.
    pub distance: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("start node '{0}' does not exist in the current layout")]
    UnknownStart(String),
    #[error("goal node '{0}' does not exist in the current layout")]
    UnknownGoal(String),
    #[error("no route exists from '{0}' to '{1}'")]
    NoRoute(String, String),
}

#[derive(Clone)]
struct OutEdge {
    edge_id: EdgeId,
    target: String,
    weight: f64,
}

struct HeapItem {
    cost: f64,
    node: NodeId,
}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost
    }
}
impl Eq for HeapItem {}
impl Ord for HeapItem {
    // Reversed so BinaryHeap (a max-heap) behaves as a min-heap on cost.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Plans the shortest route between two nodes of a validated graph using
/// Dijkstra's algorithm, respecting edge direction.
pub fn plan_route(graph: &Graph, start: &str, goal: &str) -> Result<RouteResult, RouteError> {
    if !graph.nodes.contains_key(start) {
        return Err(RouteError::UnknownStart(start.to_string()));
    }
    if !graph.nodes.contains_key(goal) {
        return Err(RouteError::UnknownGoal(goal.to_string()));
    }

    if start == goal {
        return Ok(RouteResult {
            nodes: vec![start.to_string()],
            edges: Vec::new(),
            distance: 0.0,
        });
    }

    let mut dist: HashMap<NodeId, f64> = HashMap::new();
    let mut prev: HashMap<NodeId, (NodeId, EdgeId)> = HashMap::new(); // node -> (predecessor, edge used)
    let mut heap = BinaryHeap::new();

    dist.insert(start.to_string(), 0.0);
    heap.push(HeapItem {
        cost: 0.0,
        node: start.to_string(),
    });

    while let Some(HeapItem { cost, node }) = heap.pop() {
        if node == goal {
            break;
        }
        if let Some(&best) = dist.get(&node) {
            if cost > best {
                continue; // stale heap entry
            }
        }
        // Explore outgoing edges from the current node and store the best cost to reach each neighbor.
        if let Some(out_edges) = graph.adjacency.get(&node) {
            for edge_id in out_edges {
                let edge = &graph.edges[edge_id];
                let e = OutEdge {
                    edge_id: edge.id.clone(),
                    target: edge.sink.clone(),
                    weight: graph.edge_weights[edge_id],
                };
                let next_cost = cost + e.weight;
                let is_better = match dist.get(&e.target) {
                    Some(&d) => next_cost < d,
                    None => true,
                };
                if is_better {
                    dist.insert(e.target.clone(), next_cost);
                    prev.insert(e.target.clone(), (node.clone(), e.edge_id.clone()));
                    heap.push(HeapItem {
                        cost: next_cost,
                        node: e.target.clone(),
                    });
                }
            }
        }
    }

    if !dist.contains_key(goal) {
        return Err(RouteError::NoRoute(start.to_string(), goal.to_string()));
    }

    let mut node_path = vec![goal.to_string()];
    let mut edge_path: Vec<EdgeId> = Vec::new();
    let mut current = goal.to_string();
    while current != start {
        let (p, edge_id) = prev.get(&current).cloned().expect("path must exist");
        edge_path.push(edge_id);
        node_path.push(p.clone());
        current = p;
    }
    node_path.reverse();
    edge_path.reverse();

    Ok(RouteResult {
        distance: dist[goal],
        nodes: node_path,
        edges: edge_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Edge, Layout, Node, Position};
    use crate::validation::validate_layout;

    fn node(id: &str, x: f64, y: f64) -> Node {
        Node {
            id: id.into(),
            position: Position { x, y },
        }
    }

    fn edge(id: &str, source: &str, sink: &str) -> Edge {
        Edge {
            id: id.into(),
            source: source.into(),
            sink: sink.into(),
        }
    }

    fn valid_graph() -> Graph {
        validate_layout(&valid_layout()).expect("test layout should be valid")
    }

    fn valid_layout() -> Layout {
        Layout {
            id: "Valid_map".into(),
            nodes: vec![
                node("Node_BL", 0.0, 0.0),
                node("Node_BC", 18.0, 0.0),
                node("Node_BR", 25.0, 0.0),
                node("Node_TC", 10.0, 10.0),
                node("Node_TR", 25.0, 10.0),
                node("Node_TL", 0.0, 10.0),
            ],
            edges: vec![
                edge("BL_2_BC", "Node_BL", "Node_BC"),
                edge("BC_2_BR", "Node_BC", "Node_BR"),
                edge("BR_2_TR", "Node_BR", "Node_TR"),
                edge("TR_2_TC", "Node_TR", "Node_TC"),
                edge("BC_2_TR", "Node_BC", "Node_TR"),
                edge("BC_2_TC", "Node_BC", "Node_TC"),
                edge("TC_2_TL", "Node_TC", "Node_TL"),
                edge("TL_2_BL", "Node_TL", "Node_BL"),
                edge("TR_2_BC", "Node_TR", "Node_BC"),
            ],
        }
    }

    #[test]
    fn finds_direct_edge() {
        let result = plan_route(&valid_graph(), "Node_BL", "Node_BC").unwrap();
        assert_eq!(result.nodes, vec!["Node_BL", "Node_BC"]);
        assert_eq!(result.edges, vec!["BL_2_BC"]);
        assert!((result.distance - 18.0).abs() < 1e-9);
    }

    #[test]
    fn finds_shortest_multi_hop_route() {
        // Node_BR -> Node_TR -> Node_BC is shorter than going all the way around.
        let result = plan_route(&valid_graph(), "Node_BR", "Node_BC").unwrap();
        assert_eq!(result.nodes, vec!["Node_BR", "Node_TR", "Node_BC"]);
        assert_eq!(result.edges, vec!["BR_2_TR", "TR_2_BC"]);
        assert!((result.distance - 22.206555615733702).abs() < 1e-9);
    }

    #[test]
    fn same_start_and_goal_is_trivial_route() {
        let result = plan_route(&valid_graph(), "Node_BC", "Node_BC").unwrap();
        assert_eq!(result.nodes, vec!["Node_BC"]);
        assert!(result.edges.is_empty());
        assert_eq!(result.distance, 0.0);
    }

    #[test]
    fn unknown_start_is_rejected() {
        let err = plan_route(&valid_graph(), "Node_GHOST", "Node_BC").unwrap_err();
        assert!(matches!(err, RouteError::UnknownStart(_)));
    }

    #[test]
    fn unknown_goal_is_rejected() {
        let err = plan_route(&valid_graph(), "Node_BC", "Node_GHOST").unwrap_err();
        assert!(matches!(err, RouteError::UnknownGoal(_)));
    }

    #[test]
    fn no_route_when_unreachable() {
        // Node_BR has only outgoing edge to Node_TR in this layout; there's no
        // edge back "into" a would-be isolated node, so build one directly.
        let mut graph = valid_graph();
        graph
            .nodes
            .insert("Node_ISOLATED".into(), node("Node_ISOLATED", 50.0, 50.0));
        graph.adjacency.insert("Node_ISOLATED".into(), Vec::new());
        graph.adjacency.get_mut("Node_BL").unwrap().clear();
        let err = plan_route(&graph, "Node_BL", "Node_ISOLATED").unwrap_err();
        assert!(matches!(err, RouteError::NoRoute(_, _)));
    }
}
