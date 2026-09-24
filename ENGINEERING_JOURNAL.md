# Engineering Journal

This document records the key implementation steps, design decisions, and trade-offs that shaped the Fleet Challenge solution.

## Implementation Steps

| Step | Change | Reason |
|---|---|---|
| 1 | Defined the wire-format models for layouts, nodes, positions, and directed edges. | Keep JSON input separate from the runtime graph representation. |
| 2 | Added complete layout validation with accumulated rule violations. | Return all actionable problems in one response instead of failing at the first error. |
| 3 | Added strong-connectivity validation using the graph and its transpose. | Check reachability in both directions in `O(V + E)`. |
| 4 | Built a graph only after validation succeeds. | Prevent invalid layouts from entering routing state. |
| 5 | Added weighted directed route planning with Dijkstra's algorithm. | Select the minimum-cost route when distance is required. |
| 6 | Stored only the last valid graph behind an async read/write lock. | Permit concurrent route reads while serializing layout replacement. |
| 7 | Added structured API errors and explicit HTTP status semantics. | Give clients stable machine-readable codes and distinguish failure types. |
| 8 | Added request limits, configurable port handling, and documented CORS scope. | Address basic deployment and abuse-resistance concerns. |
| 9 | Expanded unit and integration tests around invalid input and state transitions. | Test behavior at both the domain and HTTP boundaries. |

## Algorithm Decisions

| Area | Decision | Rationale and trade-off |
|---|---|---|
| Route planning | Dijkstra over directed edges | Edge weights are non-negative Euclidean distances. BFS would only minimize hop count; A* was not necessary for the expected graph size. |
| Edge cost | Euclidean distance between endpoint coordinates | The input provides coordinates but no explicit edge cost. This is a documented assumption, not a claim that coordinates always represent real travel distance. |
| Connectivity | BFS from one node in the original graph and its transpose | Strong connectivity can be checked in `O(V + E)` without running a traversal from every node or using an `O(V^3)` Floyd-Warshall pass. |
| Route reconstruction | Predecessor node and edge indexes | Preserves both the node path and the exact edge IDs returned to the client. |
| Tie handling | Stable index-based heap ordering | Makes equal-cost route selection deterministic for repeatable tests and responses. |

## Data-Structure Decisions

| Structure | Purpose | Why it was chosen |
|---|---|---|
| `HashMap<NodeId, NodeIndex>` | Resolve external node IDs | Provides average `O(1)` lookup while allowing compact internal indexes. |
| `Vec<Node>` | Store node records | Contiguous storage reduces overhead and improves locality during routing. |
| `Vec<GraphEdge>` | Store edge ID, endpoints, and weight | Keeps edge identity and precomputed cost together. |
| `Vec<Vec<EdgeIndex>>` | Store outgoing adjacency | Avoids duplicated node/weight data and preserves the exact traversed edge. |
| `HashMap<NodeIndex, f64>` | Store tentative Dijkstra distances | Keeps the implementation sparse; only discovered nodes need distance entries. |
| `RwLock<Option<Graph>>` | Store the last valid graph | Multiple routes can read concurrently, while validation replaces the graph atomically. |

## Notes

- Scope: correctness on the required validation and route-planning behavior, with distance and concurrency features added by the design rather than by separate scope expansion.
- State model: only a graph built from a layout that passes validation replaces the stored graph. Posting an invalid layout afterward does not clear or corrupt the previously stored valid graph.
- Architecture: the raw `Layout` (wire format) stays separate from the validated `Graph` (indexed and ready for routing). The graph keeps a string-to-index lookup for API IDs, compact node and edge storage, adjacency lists, and precomputed edge weights.
- Future improvements: explicit edge costs or travel times, broader property-based testing, larger-scale graph strategies such as A*, and richer structured validation errors.
- AI assistance: the initial implementation was accelerated with an AI assistant, and the generated work was reviewed, refined, and verified against the specification.
