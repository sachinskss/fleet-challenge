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
- Each edge is connected on both ends to a node (non-empty `source`/`sink`).
- Each node has at least two edges connected to it (counting both directions).
- Each node is reachable from any other node. Edges are directed (the sample map's arrows point
  one way), so this means the layout's graph must be **strongly connected**.
- As a bonus, node ids and edge ids are each checked for uniqueness — the spec doesn't call this
  out explicitly, but a duplicate id would make edge references ambiguous.

Self-loops are not rejected because they are not part of the specified validation contract.

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

## Additional context

A separate engineering record and design rationale live in [ENGINEERING_JOURNAL.md](ENGINEERING_JOURNAL.md). The README keeps the operational and behavioral documentation needed to run, test, and use the service.

- **Scope:** correctness on the required validation and routing behavior, with distance and concurrency features included because they fit the design.
- **State model:** only a graph built from a layout that passes validation replaces the stored graph.
- **Architecture:** the raw `Layout` (wire format) stays separate from the validated `Graph` (indexed and ready for routing).
- **Future improvements:** explicit edge costs, broader automated testing, larger-scale graph strategies, and richer structured validation errors.

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
| Validation edge rules | Missing source, missing sink, and unknown node references. |
| Validation graph rules | Minimum incident degree and failed strong connectivity. |
| Validation robustness | Multiple simultaneous violations. |
| Routing success | Direct route, multi-hop route, weighted shortest route, deterministic equal-cost route selection, and `start == goal`. |
| Routing failures | Unknown start, unknown goal, and unreachable destination. |
| API success | Valid layout submission and route planning through the real Axum router. |
| API state | Invalid layout does not replace the previous valid graph; routing before and after validation is covered. |
| API errors | Malformed layout JSON, malformed route JSON, and unknown-node responses. |
| API directionality | Route planning follows directed edges rather than treating the graph as undirected. |

The remaining higher-confidence follow-ups would be property-based graph testing, explicit
load or stress testing, and a domain-defined edge cost such as travel time rather than inferred
Euclidean distance.