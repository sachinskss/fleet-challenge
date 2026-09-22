use crate::graph::Graph;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared server state. Holds the last graph that passed validation, which
/// the route-planning endpoint operates on.
///
/// `tokio::sync::RwLock` allows many concurrent readers (i.e. many concurrent
/// `/route` requests can be served in parallel) while still guaranteeing
/// exclusive access on the rare write (a new layout passing `/validate`).
#[derive(Clone)]
pub struct AppState {
    pub last_valid_graph: Arc<RwLock<Option<Graph>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            last_valid_graph: Arc::new(RwLock::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
