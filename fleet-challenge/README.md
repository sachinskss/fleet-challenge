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

Run all tests from the project root (`fleet-challenge/`) — 11 unit tests plus 5 API integration
tests:

```bash
cargo test
```

Run only the API integration tests:

```bash
cargo test --test validation_api
cargo test --test routing_api
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
  "valid": false,
  "errors": [
    { "rule": "edge_references_existing_node", "message": "Edge 'TR_2_GHOST' references unknown sink node 'Node_GHOST'" }
  ]
}
```

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

Response `404 Not Found` if `start`/`goal` don't exist in the current layout, or no route
exists between them. Response `409 Conflict` if no layout has passed validation yet.

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

Dijkstra's algorithm over the directed graph, with edge weight = Euclidean distance between the
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

Because accepted layouts must be strongly connected, a valid API graph can't demonstrate an
unreachable reverse route. The directionality test instead verifies that Dijkstra chooses the
available directed edges in the expected order.

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

Keep these JSON files at  `fleet-challenge/test_data/` - the integration tests include them
Try them against a running server, e.g.:

```bash
curl -s -X POST http://localhost:8080/api/v1/layout/validate \
  -H 'Content-Type: application/json' \
  -d @test_data/invalid_not_strongly_connected.json
```

## Journal / decisions & trade-offs

- **Scope:** Prioritized correctness on the two required endpoints and included the distance + concurrency features since they came free with the design.
- **Distance:** Dijkstra already computes it as a side effect of pathfinding.
- **Concurrency:** axum + RwLock allows multiple route requests to read the current graph concurrently, while layout updates acquire exclusive access.
- **Strong connectivity:** Used BFS from one node on the graph + its reverse, instead of BFS from every node. O(V+E) vs O(V·(V+E)).
- **Error reporting:** Validation collects all rule violations, not just the first. Each error has a `rule` id + human message.
- **State model:** Only a graph built from a layout that *passes* validation replaces the stored graph. Posting an invalid layout afterwards does not clear or corrupt the previously stored valid graph (verified with both manual and automated tests).
- **Distance metric:** Euclidean distance between edge endpoints, summed along the route this is a natural fit given the `{x, y}` positions provided.
- **Directed graph:** The example map's edges are one-directional (`TC -> TL`), so used strong (not weak) connectivity. The "at least two edges" rule counts an edge either way (incoming or outgoing), matching the spec wording and preventing a node with only-incoming or only-outgoing edges from passing as "connected."
- **Architecture:** Split the raw `Layout` (wire format) from a validated `Graph` (indexed, ready for routing) so `validate_layout` returns `Result<Graph, Vec<ValidationError>>`: routing never has to re-check invariants that validation already guarantees.
- **Future improvements:**
  - Explicit edge costs/travel times rather than deriving distance solely from node coordinates.
  - More comprehensive API/integration and property-based testing.
  - Optimized graph/routing strategies for larger layouts, potentially including A*.
  - Structured validation errors containing affected node/edge IDs.
- **AI assistance:** I used an AI assistant (Claude) as a partner and coding accelerator. It helped generate the initial boilerplate, draft baseline implementations for the validation and Dijkstra routing logic. I reviewed, refined, and tested all generated code against the specifications to ensure correctness, idiomatic Rust structure, and edge-case handling.