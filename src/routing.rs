use crate::graph::{EdgeIndex, Graph, NodeIndex};
use crate::models::{EdgeId, NodeId};

use serde::Serialize;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

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

impl RouteError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownStart(_) => "unknown_start_node",
            Self::UnknownGoal(_) => "unknown_goal_node",
            Self::NoRoute(_, _) => "no_route",
        }
    }
}

struct HeapItem {
    cost: f64,
    node: NodeIndex,
}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.node == other.node
    }
}
impl Eq for HeapItem {}
impl Ord for HeapItem {
    // Reversed so BinaryHeap (a max-heap) behaves as a min-heap on cost.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| self.node.cmp(&other.node))
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
    let Some(start_index) = graph.node_index(start) else {
        return Err(RouteError::UnknownStart(start.to_string()));
    };
    let Some(goal_index) = graph.node_index(goal) else {
        return Err(RouteError::UnknownGoal(goal.to_string()));
    };

    if start == goal {
        return Ok(RouteResult {
            nodes: vec![start.to_string()],
            edges: Vec::new(),
            distance: 0.0,
        });
    }

    let mut distances = vec![f64::INFINITY; graph.node_count()];
    let mut previous: Vec<Option<(NodeIndex, EdgeIndex)>> = vec![None; graph.node_count()];
    let mut heap = BinaryHeap::new();

    distances[start_index] = 0.0;
    heap.push(HeapItem {
        cost: 0.0,
        node: start_index,
    });

    while let Some(HeapItem { cost, node }) = heap.pop() {
        if node == goal_index {
            break;
        }
        if cost > distances[node] {
            continue; // stale heap entry
        }
        // Explore outgoing edges from the current node and store the best cost to reach each neighbor.
        for &edge_index in graph.outgoing_edges(node) {
            let edge = graph.edge(edge_index);
            let next_cost = cost + edge.weight;
            if next_cost < distances[edge.sink] {
                distances[edge.sink] = next_cost;
                previous[edge.sink] = Some((node, edge_index));
                heap.push(HeapItem {
                    cost: next_cost,
                    node: edge.sink,
                });
            }
        }
    }

    if distances[goal_index].is_infinite() {
        return Err(RouteError::NoRoute(start.to_string(), goal.to_string()));
    }

    let mut node_path = vec![graph.node_id(goal_index).clone()];
    let mut edge_path: Vec<EdgeId> = Vec::new();
    let mut current = goal_index;
    while current != start_index {
        let Some((parent, edge_index)) = previous[current] else {
            return Err(RouteError::NoRoute(start.to_string(), goal.to_string()));
        };
        edge_path.push(graph.edge(edge_index).id.clone());
        node_path.push(graph.node_id(parent).clone());
        current = parent;
    }
    node_path.reverse();
    edge_path.reverse();

    Ok(RouteResult {
        distance: distances[goal_index],
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
    fn chooses_an_equal_cost_route_deterministically() {
        let layout = Layout {
            id: "Equal_cost_map".into(),
            nodes: vec![
                node("Node_A", 0.0, 0.0),
                node("Node_B", 1.0, 0.0),
                node("Node_C", 0.0, 1.0),
                node("Node_D", 1.0, 1.0),
            ],
            edges: vec![
                edge("A_2_B", "Node_A", "Node_B"),
                edge("B_2_D", "Node_B", "Node_D"),
                edge("A_2_C", "Node_A", "Node_C"),
                edge("C_2_D", "Node_C", "Node_D"),
                edge("B_2_A", "Node_B", "Node_A"),
                edge("D_2_B", "Node_D", "Node_B"),
                edge("C_2_A", "Node_C", "Node_A"),
                edge("D_2_C", "Node_D", "Node_C"),
            ],
        };
        let graph = validate_layout(&layout).unwrap();
        let result = plan_route(&graph, "Node_A", "Node_D").unwrap();

        assert_eq!(result.nodes, vec!["Node_A", "Node_C", "Node_D"]);
        assert_eq!(result.edges, vec!["A_2_C", "C_2_D"]);
        assert_eq!(result.distance, 2.0);
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
        let mut layout = valid_layout();
        layout.nodes.push(node("Node_ISOLATED", 50.0, 50.0));
        layout
            .edges
            .push(edge("ISOLATED_LOOP", "Node_ISOLATED", "Node_ISOLATED"));
        let err = plan_route(
            &Graph::from_validated_layout(&layout),
            "Node_BL",
            "Node_ISOLATED",
        )
        .unwrap_err();
        assert!(matches!(err, RouteError::NoRoute(_, _)));
    }
}
