//! Concurrency tests for the fleet-challenge API.
//!
//! These exercise the one real race in the design: multiple `/validate`
//! writers racing against each other and against concurrent `/route`
//! readers. `Router` is `Clone` and shares the same `AppState` (the
//! `RwLock` lives behind an `Arc`), so cloning it per spawned task still
//! hits shared state.

mod common;

use axum::http::StatusCode;
use common::{invalid_layout, post, valid_layout};
use fleet_challenge::app;

/// A second, node-disjoint valid layout, used to test two different valid
/// submissions racing for "last valid graph" status.
const ALTERNATE_VALID_LAYOUT: &str = r#"{
  "id": "Alternate_valid_map",
  "nodes": [
    { "id": "Node_X1", "position": { "x": 0.0, "y": 0.0 } },
    { "id": "Node_X2", "position": { "x": 5.0, "y": 0.0 } },
    { "id": "Node_X3", "position": { "x": 5.0, "y": 5.0 } }
  ],
  "edges": [
    { "id": "X1_2_X2", "source": "Node_X1", "sink": "Node_X2" },
    { "id": "X2_2_X3", "source": "Node_X2", "sink": "Node_X3" },
    { "id": "X3_2_X1", "source": "Node_X3", "sink": "Node_X1" }
  ]
}"#;

/// TEST 1 — many identical valid submissions concurrently.
///
/// Expectation: every response is 200 OK. Validating the *same* layout
/// repeatedly is idempotent, so there's nothing to actually race over —
/// this is mostly a smoke test that concurrent writers don't panic or
/// deadlock under load.
#[tokio::test]
async fn concurrent_identical_validations_all_succeed() {
    let router = app();
    let body = valid_layout();

    let mut handles = Vec::new();
    for _ in 0..20 {
        let router = router.clone();
        handles.push(tokio::spawn(async move {
            post(router, "/api/v1/layout/validate", body).await.status()
        }));
    }

    for h in handles {
        assert_eq!(h.await.unwrap(), StatusCode::OK);
    }
}

/// TEST 2 — two *different* valid layouts racing to become "last valid."
///
/// Expectation: both return 200 OK (both are independently valid). After
/// the race settles, exactly one of the two layouts is the stored graph —
/// not a corrupted mix of the two, and not neither. Routes valid only
/// against one layout's node ids must resolve consistently: exactly one
/// of the two probes below succeeds.
#[tokio::test]
async fn concurrent_distinct_valid_layouts_last_write_wins_cleanly() {
    let router = app();

    let (ra, rb) = tokio::join!(
        post(router.clone(), "/api/v1/layout/validate", valid_layout()),
        post(
            router.clone(),
            "/api/v1/layout/validate",
            ALTERNATE_VALID_LAYOUT
        ),
    );
    assert_eq!(ra.status(), StatusCode::OK);
    assert_eq!(rb.status(), StatusCode::OK);

    let probe_a = post(
        router.clone(),
        "/api/v1/route",
        r#"{"start":"Node_BL","goal":"Node_BC"}"#,
    )
    .await;
    let probe_b = post(
        router.clone(),
        "/api/v1/route",
        r#"{"start":"Node_X1","goal":"Node_X2"}"#,
    )
    .await;

    let successes = [probe_a.status(), probe_b.status()]
        .into_iter()
        .filter(|s| *s == StatusCode::OK)
        .count();
    assert_eq!(
        successes, 1,
        "expected exactly one layout to have won the race, got {} successes",
        successes
    );
}

/// TEST 3 — an invalid layout racing against a valid one.
///
/// Expectation: the invalid one always returns 422 and never becomes the
/// stored graph, regardless of arrival order relative to the valid one.
/// This is the state-model guarantee under concurrency, not just under
/// sequential calls.
#[tokio::test]
async fn concurrent_invalid_layout_never_wins_the_race() {
    let router = app();

    let (r_valid, r_invalid) = tokio::join!(
        post(router.clone(), "/api/v1/layout/validate", valid_layout()),
        post(router.clone(), "/api/v1/layout/validate", invalid_layout()),
    );
    assert_eq!(r_valid.status(), StatusCode::OK);
    assert_eq!(r_invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let route = post(
        router,
        "/api/v1/route",
        r#"{"start":"Node_BR","goal":"Node_BC"}"#,
    )
    .await;
    assert_eq!(route.status(), StatusCode::OK);
}

/// TEST 4 — many concurrent readers while a write is in flight.
///
/// Expectation: no reader ever observes a torn/partial state. Every read
/// either sees the pre-write graph or the post-write graph in full — never
/// an error caused by the swap itself.
#[tokio::test]
async fn concurrent_reads_during_a_write_never_see_a_torn_state() {
    let router = app();

    // Baseline so readers have something to hit before the concurrent
    // re-validation below.
    post(router.clone(), "/api/v1/layout/validate", valid_layout()).await;

    let mut handles = Vec::new();
    for _ in 0..50 {
        let router = router.clone();
        handles.push(tokio::spawn(async move {
            post(
                router,
                "/api/v1/route",
                r#"{"start":"Node_BR","goal":"Node_BC"}"#,
            )
            .await
            .status()
        }));
    }
    let writer = {
        let router = router.clone();
        tokio::spawn(async move {
            post(router, "/api/v1/layout/validate", valid_layout())
                .await
                .status()
        })
    };

    for h in handles {
        // The route is valid both before and after the concurrent
        // re-validation of the *same* layout, so every reader must see 200.
        assert_eq!(h.await.unwrap(), StatusCode::OK);
    }
    assert_eq!(writer.await.unwrap(), StatusCode::OK);
}

/// TEST 5 — a route request racing the very first validation ever received.
///
/// Expectation: the route request either sees "no layout yet"
/// (503 / `no_valid_layout`) if it lands before the write completes, or
/// succeeds (200) if it lands after — never a panic, never any other
/// status code. This is inherently timing-dependent, so the assertion only
/// constrains the *set* of acceptable outcomes rather than picking one.
#[tokio::test]
async fn route_racing_first_ever_validation_only_sees_two_valid_outcomes() {
    let router = app();

    let (route_res, validate_res) = tokio::join!(
        post(
            router.clone(),
            "/api/v1/route",
            r#"{"start":"Node_BR","goal":"Node_BC"}"#,
        ),
        post(router.clone(), "/api/v1/layout/validate", valid_layout()),
    );

    assert_eq!(validate_res.status(), StatusCode::OK);
    assert!(
        route_res.status() == StatusCode::SERVICE_UNAVAILABLE
            || route_res.status() == StatusCode::OK,
        "unexpected status: {}",
        route_res.status()
    );
}
