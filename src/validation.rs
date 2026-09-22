use crate::graph::Graph;
use crate::models::{EdgeId, Layout, NodeId};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Serialize, PartialEq)]
pub struct ValidationError {
    /// Machine readable rule identifier, useful for tests/UI.
    pub rule: String,
    /// Human readable description of what went wrong.
    pub message: String,
}

/// Validates a layout against all fleet-management rules.
///
/// Rules enforced:
/// 1. Node ids are unique.
/// 2. Edge ids are unique.
/// 3. Every edge has a non-empty source and sink.
/// 4. Every edge's source/sink references an existing node ("connects only existing nodes",
///    "connected on both ends to a node").
/// 5. Every node has at least two edges attached to it (counting both directions).
/// 6. Every node is reachable from every other node (the layout is strongly connected),
///    treating edges as directed driveways.
pub fn validate_layout(layout: &Layout) -> Result<Graph, Vec<ValidationError>> {
    validate_rules(layout)?;
    Ok(Graph::from_layout(layout))
}

pub fn validate_rules(layout: &Layout) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    // --- Rule: node positions must produce finite route weights ---------------
    for node in &layout.nodes {
        if !node.position.x.is_finite() || !node.position.y.is_finite() {
            errors.push(ValidationError {
                rule: "finite_node_position".into(),
                message: format!("Node '{}' has a non-finite position", node.id),
            });
        }
    }

    // --- Rule: unique node ids -------------------------------------------------
    let mut node_ids: HashSet<NodeId> = HashSet::new();
    for node in &layout.nodes {
        if !node_ids.insert(node.id.clone()) {
            errors.push(ValidationError {
                rule: "unique_node_id".into(),
                message: format!("Duplicate node id '{}'", node.id),
            });
        }
    }

    // --- Rule: unique edge ids ---------------------------------------------------
    let mut edge_ids: HashSet<EdgeId> = HashSet::new();
    for edge in &layout.edges {
        if !edge_ids.insert(edge.id.clone()) {
            errors.push(ValidationError {
                rule: "unique_edge_id".into(),
                message: format!("Duplicate edge id '{}'", edge.id),
            });
        }
    }

    // --- Rule: edges have both endpoints set and reference existing nodes --------
    for edge in &layout.edges {
        if edge.source.trim().is_empty() || edge.sink.trim().is_empty() {
            errors.push(ValidationError {
                rule: "edge_endpoints_present".into(),
                message: format!(
                    "Edge '{}' is not connected on both ends (source or sink missing)",
                    edge.id
                ),
            });
        }
        if !node_ids.contains(&edge.source) {
            errors.push(ValidationError {
                rule: "edge_references_existing_node".into(),
                message: format!(
                    "Edge '{}' references unknown source node '{}'",
                    edge.id, edge.source
                ),
            });
        }
        if !node_ids.contains(&edge.sink) {
            errors.push(ValidationError {
                rule: "edge_references_existing_node".into(),
                message: format!(
                    "Edge '{}' references unknown sink node '{}'",
                    edge.id, edge.sink
                ),
            });
        }
        if edge.source == edge.sink {
            errors.push(ValidationError {
                rule: "edge_no_self_loop".into(),
                message: format!(
                    "Edge '{}' connects node '{}' to itself, which is not a valid driveway",
                    edge.id, edge.source
                ),
            });
        }
    }

    // --- Rule: every node has at least two edges attached -----------------------
    let mut degree: HashMap<&NodeId, usize> =
        layout.nodes.iter().map(|n| (&n.id, 0usize)).collect();
    // Counts edges in either direction (incoming or outgoing)
    for edge in &layout.edges {
        if let Some(d) = degree.get_mut(&edge.source) {
            *d += 1;
        }
        if let Some(d) = degree.get_mut(&edge.sink) {
            *d += 1;
        }
    }
    for node in &layout.nodes {
        let d = degree.get(&node.id).copied().unwrap_or(0);
        if d < 2 {
            errors.push(ValidationError {
                rule: "min_two_edges".into(),
                message: format!(
                    "Node '{}' only has {} connected edge(s), at least 2 are required",
                    node.id, d
                ),
            });
        }
    }

    // --- Rule: every node reachable from every other node ------------------------
    // Only meaningful once every edge points at a node that actually exists,
    // otherwise we'd just be re-reporting the errors above in a confusing way.
    let has_dangling_refs = errors
        .iter()
        .any(|e| e.rule == "edge_references_existing_node" || e.rule == "edge_endpoints_present");

    if !has_dangling_refs && !layout.nodes.is_empty() {
        let node_id_list: Vec<&NodeId> = layout.nodes.iter().map(|n| &n.id).collect();

        let mut forward_adj: HashMap<&NodeId, Vec<&NodeId>> =
            node_id_list.iter().map(|&id| (id, Vec::new())).collect();
        let mut backward_adj: HashMap<&NodeId, Vec<&NodeId>> =
            node_id_list.iter().map(|&id| (id, Vec::new())).collect();

        for edge in &layout.edges {
            forward_adj
                .get_mut(&edge.source)
                .expect("node id was inserted into adjacency map above")
                .push(&edge.sink);
            backward_adj
                .get_mut(&edge.sink)
                .expect("node id was inserted into adjacency map above")
                .push(&edge.source);
        }

        // A directed graph is strongly connected iff a BFS from any single node
        // reaches every other node both in the graph and in its transpose. This
        // lets us check "every node reachable from every other node" in O(V+E)
        // instead of running a BFS from every single node (O(V*(V+E))).
        let start = node_id_list[0];
        let forward_reachable = bfs_reachable(start, &forward_adj);
        let backward_reachable = bfs_reachable(start, &backward_adj);

        let mut unreachable_from_start: Vec<&NodeId> = node_id_list
            .iter()
            .filter(|id| !forward_reachable.contains(*id))
            .copied()
            .collect();
        let mut cannot_reach_start: Vec<&NodeId> = node_id_list
            .iter()
            .filter(|id| !backward_reachable.contains(*id))
            .copied()
            .collect();

        if !unreachable_from_start.is_empty() {
            unreachable_from_start.sort_unstable();
            errors.push(ValidationError {
                rule: "strongly_connected".into(),
                message: format!(
                    "Node(s) [{}] are not reachable from node '{}'",
                    unreachable_from_start
                        .iter()
                        .map(|id| id.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    start
                ),
            });
        }
        if !cannot_reach_start.is_empty() {
            cannot_reach_start.sort_unstable();
            errors.push(ValidationError {
                rule: "strongly_connected".into(),
                message: format!(
                    "Node(s) [{}] cannot reach node '{}'",
                    cannot_reach_start
                        .iter()
                        .map(|id| id.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    start
                ),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// store nodes reachable from 'start'
fn bfs_reachable<'a>(
    start: &'a NodeId,
    adjacency: &HashMap<&'a NodeId, Vec<&'a NodeId>>,
) -> HashSet<&'a NodeId> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some(current) = queue.pop_front() {
        if let Some(neighbors) = adjacency.get(current) {
            for &n in neighbors {
                if visited.insert(n) {
                    queue.push_back(n);
                }
            }
        }
    }
    visited
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Edge, Node, Position};

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
    fn accepts_the_provided_valid_map() {
        let result = validate_layout(&valid_layout());
        assert!(result.is_ok(), "errors: {:?}", result.err());
    }

    #[test]
    fn rejects_edge_to_unknown_node() {
        let mut layout = valid_layout();
        layout
            .edges
            .push(edge("BC_2_GHOST", "Node_BC", "Node_GHOST"));
        let result = validate_layout(&layout);
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.rule == "edge_references_existing_node"));
    }

    #[test]
    fn rejects_edge_missing_endpoint() {
        let mut layout = valid_layout();
        layout.edges.push(edge("dangling", "Node_BC", ""));
        let result = validate_layout(&layout);
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.rule == "edge_endpoints_present"));
    }

    #[test]
    fn rejects_node_with_fewer_than_two_edges() {
        let mut layout = valid_layout();
        layout.nodes.push(node("Node_ISOLATED", 5.0, 5.0));
        layout
            .edges
            .push(edge("BC_2_ISOLATED", "Node_BC", "Node_ISOLATED"));
        let result = validate_layout(&layout);
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.rule == "min_two_edges"));
    }

    #[test]
    fn rejects_layout_that_is_not_strongly_connected() {
        // Add an extra node only reachable *from* the graph, nothing points back to it.
        let mut layout = valid_layout();
        layout.nodes.push(node("Node_DEADEND", 30.0, 30.0));
        layout
            .edges
            .push(edge("TR_2_DEADEND", "Node_TR", "Node_DEADEND"));
        layout
            .edges
            .push(edge("TC_2_DEADEND", "Node_TC", "Node_DEADEND"));
        let result = validate_layout(&layout);
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.rule == "strongly_connected"));
    }

    #[test]
    fn rejects_duplicate_node_ids() {
        let mut layout = valid_layout();
        layout.nodes.push(node("Node_BL", 1.0, 1.0));
        let result = validate_layout(&layout);
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.rule == "unique_node_id"));
    }

    #[test]
    fn rejects_duplicate_edge_ids() {
        let mut layout = valid_layout();
        layout.edges.push(edge("BL_2_BC", "Node_BC", "Node_BR"));
        let errors = validate_layout(&layout).unwrap_err();
        assert!(errors.iter().any(|e| e.rule == "unique_edge_id"));
    }

    #[test]
    fn rejects_self_loops() {
        let mut layout = valid_layout();
        layout.edges.push(edge("self_loop", "Node_BC", "Node_BC"));
        let errors = validate_layout(&layout).unwrap_err();
        assert!(errors.iter().any(|e| e.rule == "edge_no_self_loop"));
    }

    #[test]
    fn rejects_non_finite_node_positions() {
        let mut layout = valid_layout();
        layout.nodes[0].position.x = f64::NAN;
        let errors = validate_layout(&layout).unwrap_err();
        assert!(errors.iter().any(|e| e.rule == "finite_node_position"));
    }

    #[test]
    fn reports_multiple_independent_violations() {
        let mut layout = valid_layout();
        layout.nodes.push(node("Node_BL", 1.0, 1.0));
        layout.edges.push(edge("duplicate", "Node_BC", ""));
        layout
            .edges
            .push(edge("duplicate", "Node_BC", "Node_GHOST"));
        let errors = validate_layout(&layout).unwrap_err();
        assert!(errors.iter().any(|e| e.rule == "unique_node_id"));
        assert!(errors.iter().any(|e| e.rule == "unique_edge_id"));
        assert!(errors.iter().any(|e| e.rule == "edge_endpoints_present"));
        assert!(errors
            .iter()
            .any(|e| e.rule == "edge_references_existing_node"));
    }
}
