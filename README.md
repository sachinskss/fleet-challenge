# Fleet Challenge — Layout Validation & Route Planning Service

A small REST API, written in Rust with [axum](https://github.com/tokio-rs/axum), that

1. validates a fleet layout (nodes/edges) against the rules from the spec, and
2. plans the shortest route between two nodes of the last validated graph.

The code keeps the HTTP layer in `api.rs`, graph construction and storage-ready data in
`graph.rs`, and the domain operations in `validation.rs` and `routing.rs`. `main.rs` is limited
to launching the reusable library application defined in `lib.rs`.

## Running it

### Toolchain and dependencies

This project supports Rust `1.75` and newer. `Cargo.lock` uses lockfile format 3 and records the
exact versions and checksums of all dependencies.

Install or select a compatible toolchain with `rustup` and verify the environment:

```bash
rustup toolchain install 1.75.0
rustup run 1.75.0 rustc --version
rustup run 1.75.0 cargo --version
```

```bash
cargo build
cargo test
```

```bash
cargo run
# -> fleet-challenge listening on http://127.0.0.1:8080
```

The server listens on port `8080` by default; set `PORT` to override it. Request bodies are limited to 1 MiB.

Run all tests from the project root (`fleet-challenge/`) — domain unit tests plus API integration tests:

```bash
cargo test
```

Run only the API integration tests:

```bash
cargo test --test validation_api
cargo test --test routing_api
cargo test --test concurrency_api
```

The integration tests drive Axum's router in-process via `tower::ServiceExt::oneshot`, so they
don't require `cargo run` / a live server.

## API

### `POST /api/v1/layout/validate`

Body: a layout JSON object (see `test_data/valid_map.json` for the example from the spec).

Response `200 OK` if valid:

```json
{ "valid": true, "errors": [] }
```

On success, the validated layout is converted to and stored as the **last valid graph**, which
the routing endpoint operates on. Layouts that fail validation are **not** stored — the previous
last-valid graph (if any) stays in effect.

Response `422 Unprocessable Entity` if invalid:

```json
{
  "error": {
    "code": "invalid_layout",
    "message": "layout validation failed",
    "details": {
      "violations": [
        { "rule": "edge_references_existing_node", "message": "Edge 'TR_2_GHOST' references unknown sink node 'Node_GHOST'" }
      ]
    }
  }
}
```

Malformed JSON returns `400 Bad Request` with error code `malformed_json`.

### `POST /api/v1/route`

Body:
```json
{ "start": "Node_BR", "goal": "Node_BC" }
```

Response `200 OK`:

```json
{
  "nodes": ["Node_BR", "Node_TR", "Node_BC"],
  "edges": ["BR_2_TR", "TR_2_BC"],
  "distance": 22.206555615733702
}
```

Response `404 Not Found` with error code `unknown_start_node` or `unknown_goal_node` if an
endpoint does not exist. Response `422 Unprocessable Entity` with error code `no_route` if both
endpoints exist but no directed route connects them. Response `503 Service Unavailable` with
error code `no_valid_layout` if no layout has passed validation yet. Application errors use this
envelope: `{ "error": { "code": "unknown_start_node", "message": "..." } }`.

### `GET /health`

Trivial liveness check, returns `ok`.

### `GET /`

Basic service info (name, version, endpoint list) — useful for humans and load balancers probing
the root path.

## Validated rules for layouts

- Each edge connects only existing nodes.
- Each edge is connected on both ends to a node (non-empty `source`/`sink`, plus a `source != sink`
  sanity check — a driveway looping back to the same intersection isn't a useful edge).
- Each node has at least two edges connected to it (counting both directions).
- Each node is reachable from any other node. Edges are directed (the sample map's arrows point
  one way), so this means the layout's graph must be **strongly connected**.
- As a bonus, node ids and edge ids are each checked for uniqueness — the spec doesn't call this
  out explicitly, but a duplicate id would make edge references ambiguous.

Validation always returns the **complete** list of problems it finds (not just the first one), so
a Layouter UI can highlight everything at once.

## Route planning

Dijkstra's algorithm over the directed graph, selecting the minimum summed Euclidean distance between the
positions of the two endpoint nodes. The Euclidean distance between edge endpoints is used as 
the edge cost and summed along the selected route. `start == goal` returns a trivial zero-length 
route.

**Concurrency:** the last-valid-graph is stored behind a `tokio::sync::RwLock`. Route planning
only takes a read lock, so many `/api/v1/route` requests are served concurrently and only the rare
write (a new layout passing validation) needs exclusive access. Combined with axum/tokio's async
request handling, this satisfies "endpoint can serve many requests at once" without extra
machinery.

## Integration tests

The application is exposed through `lib.rs` (`app()` returns the configured `Router`) so Cargo
integration tests can exercise the real Axum router without starting a network server. The tests
cover:

- a valid layout submission returns `200 OK`
- an invalid layout submission returns `422 Unprocessable Entity`
- validation followed by route planning returns the expected node and edge sequence
- an invalid layout does not replace the previously stored valid graph
- routing follows the directed edge sequence
- five concurrency scenarios covering concurrent writers, readers, state replacement, and first-load races

Because accepted layouts must be strongly connected, a valid API graph can't demonstrate an
unreachable reverse route. The directionality test instead verifies that Dijkstra chooses the
available directed edges in the expected order.

### Concurrency verification

The dedicated `tests/concurrency_api.rs` suite verifies the shared `AppState` and `RwLock` under
concurrent access. It covers:

1. Twenty identical valid layout submissions completing successfully.
2. Two distinct valid layouts racing, with exactly one complete graph winning last-write-wins.
3. An invalid layout racing with a valid layout without replacing valid state.
4. Concurrent route readers during a validation write without observing partial state.
5. A route request racing the first validation and seeing only the valid `503` or `200` outcome.

Verified against the project: the build is clean, the concurrency suite passes all 5 tests, five
repeated runs remain stable, and the existing 25 tests continue to pass.

## Test data

`test_data/` contains the valid map from the spec plus one invalid map per validation rule:

| File | Violates |
|---|---|
| `valid_map.json` | (nothing — the example from the spec) |
| `invalid_unknown_node_reference.json` | edge references a non-existent node |
| `invalid_missing_endpoint.json` | edge has an empty `sink` |
| `invalid_isolated_node.json` | a node with only one connected edge |
| `invalid_not_strongly_connected.json` | a node only reachable one-way (dead end) |
| `invalid_duplicate_node_id.json` | two nodes sharing the same id |

Keep these JSON files at `test_data/`; the integration tests include them.
Try them against a running server, for example:

```bash
curl -s -X POST http://localhost:8080/api/v1/layout/validate \
  -H 'Content-Type: application/json' \
  -d @test_data/invalid_not_strongly_connected.json
```

## Decisions & trade-offs

- **Scope:** Prioritized correctness on the two required endpoints and included the distance + concurrency features since they came free with the design.
- **Distance:** Dijkstra selects the minimum summed Euclidean distance. This is an explicit choice
  because the wire format does not provide edge costs; production routing would normally use a
  declared travel-time or distance field instead.
- **Concurrency:** axum + RwLock allows multiple route requests to read the current graph concurrently, while layout updates acquire exclusive access.
- **Strong connectivity:** Used BFS from one node on the graph + its reverse, instead of BFS from every node. O(V+E) vs O(V·(V+E)).
- **Error reporting:** Validation collects all rule violations, not just the first. Each error has a `rule` id + human message.
- **State model:** Only a graph built from a layout that *passes* validation replaces the stored graph. Posting an invalid layout afterwards does not clear or corrupt the previously stored valid graph (verified with both manual and automated tests).
- **Directed graph:** The example map's edges are one-directional (`TC -> TL`), so used strong (not weak) connectivity. The "at least two edges" rule counts an edge either way (incoming or outgoing), matching the spec wording and preventing a node with only-incoming or only-outgoing edges from passing as "connected."
- **Architecture:** Split the raw `Layout` (wire format) from a validated `Graph` (indexed, ready
  for routing). The graph keeps a string-to-index lookup for API IDs, contiguous node and edge
  storage, adjacency lists of integer edge indexes, and each edge's precomputed weight. Routing
  never has to re-check invariants that validation already guarantees.
- **Future improvements:**
  - Explicit edge costs/travel times rather than deriving distance solely from node coordinates.
  - More comprehensive API/integration and property-based testing.
  - Optimized graph/routing strategies for larger layouts, potentially including A*.
  - Structured validation errors containing affected node/edge IDs.
- **AI assistance:** I used an AI assistant (Claude) as a partner and coding accelerator. It helped generate the initial boilerplate, draft baseline implementations for the validation and Dijkstra routing logic. I reviewed, refined, and tested all generated code against the specifications to ensure correctness, idiomatic Rust structure, and edge-case handling.

## Journal

This section records the important implementation steps and decisions made while reviewing the
Fleet Challenge against a higher engineering bar.

### Implementation Steps

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

### Algorithm Decisions

| Area | Decision | Rationale and trade-off |
|---|---|---|
| Route planning | Dijkstra over directed edges | Edge weights are non-negative Euclidean distances. BFS would only minimize hop count; A* was not necessary for the expected graph size. |
| Edge cost | Euclidean distance between endpoint coordinates | The input provides coordinates but no explicit edge cost. This is a documented assumption, not a claim that coordinates always represent real travel distance. |
| Connectivity | BFS from one node in the original graph and its transpose | Strong connectivity can be checked in `O(V + E)` without running a traversal from every node or using an `O(V^3)` Floyd-Warshall pass. |
| Route reconstruction | Predecessor node and edge indexes | Preserves both the node path and the exact edge IDs returned to the client. |
| Tie handling | Stable index-based heap ordering | Makes equal-cost route selection deterministic for repeatable tests and responses. |

### Data-Structure Decisions

| Structure | Purpose | Why it was chosen |
|---|---|---|
| `HashMap<NodeId, NodeIndex>` | Resolve external node IDs | Provides average `O(1)` lookup while allowing compact internal indexes. |
| `Vec<Node>` | Store node records | Contiguous storage reduces overhead and improves locality during routing. |
| `Vec<GraphEdge>` | Store edge ID, endpoints, and weight | Keeps edge identity and precomputed cost together. |
| `Vec<Vec<EdgeIndex>>` | Store outgoing adjacency | Avoids duplicated node/weight data and preserves the exact traversed edge. |
| `HashMap<NodeIndex, f64>` | Store tentative Dijkstra distances | Keeps the implementation sparse; only discovered nodes need distance entries. |
| `RwLock<Option<Graph>>` | Store the last valid graph | Multiple routes can read concurrently, while validation replaces the graph atomically. |

### API Decisions

| Situation | Status | Error code or response | Decision |
|---|---:|---|---|
| Valid layout | `200` | `{ "valid": true, "errors": [] }` | Store the resulting graph as the last valid layout. |
| Invalid layout | `422` | `invalid_layout` with validation details | JSON is valid, but domain rules are not satisfied. |
| Malformed JSON | `400` | `malformed_json` | Reject invalid JSON before domain processing. |
| Oversized request | `413` | Framework limit response | Bound memory and processing costs before deserialization. |
| Unknown start or goal | `404` | `unknown_start_node` / `unknown_goal_node` | The requested endpoint does not exist in the current graph. |
| No route between existing nodes | `422` | `no_route` | Both resources exist, but the requested operation cannot be satisfied. |
| No validated graph loaded | `503` | `no_valid_layout` | The service is not ready to route because no usable state exists. |
| API error body | Varies | `{ "error": { "code", "message", "details" } }` | Keep machine-readable codes separate from human-readable messages. |
| CORS | N/A | Permissive in development | Explicitly documented for local development; production should restrict origins. |

### Test Matrix

| Area | Cases covered |
|---|---|
| Validation success | Provided valid map is accepted and converted into a graph. |
| Validation identity rules | Duplicate node IDs and duplicate edge IDs. |
| Validation edge rules | Missing endpoints, unknown node references, and self-loops. |
| Validation graph rules | Minimum incident degree and failed strong connectivity. |
| Validation robustness | Non-finite coordinates and multiple simultaneous violations. |
| Routing success | Direct route, multi-hop route, weighted shortest route, and `start == goal`. |
| Routing failures | Unknown start, unknown goal, and unreachable destination. |
| API success | Valid layout submission and route planning through the real Axum router. |
| API state | Invalid layout does not replace the previous valid graph; routing before validation is rejected. |
| API errors | Malformed layout JSON, malformed route JSON, and unknown-node responses. |
| API directionality | Route planning follows directed edges rather than treating the graph as undirected. |

The remaining higher-confidence follow-ups would be property-based graph testing, explicit
concurrency tests, and a domain-defined edge cost such as travel time rather than inferred
Euclidean distance.