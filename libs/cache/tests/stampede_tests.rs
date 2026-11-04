use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use omni_cache::stampede::{
    SingleFlight, calculate_jittered_ttl, calculate_jittered_ttl_ms, should_recompute_xfetch,
};
use tokio::time::sleep;

#[tokio::test]
async fn test_single_flight_deduplication() {
    let flight = Arc::new(SingleFlight::<String, String>::new());
    let invocation_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..20 {
        let f = Arc::clone(&flight);
        let count = Arc::clone(&invocation_count);

        handles.push(tokio::spawn(async move {
            f.execute_or_join("resource_key_1".to_string(), || async move {
                count.fetch_add(1, Ordering::SeqCst);
                // Simulate slow DB loader
                sleep(Duration::from_millis(60)).await;
                "computed_value_from_db".to_string()
            })
            .await
        }));
    }

    for h in handles {
        let res = h.await.unwrap();
        assert_eq!(res, "computed_value_from_db");
    }

    // Only 1 execution despite 20 concurrent requests
    assert_eq!(invocation_count.load(Ordering::SeqCst), 1);
    assert_eq!(flight.active_count(), 0);
}

#[tokio::test]
async fn test_single_flight_leader_cancellation_and_follower_takeover() {
    let flight = Arc::new(SingleFlight::<String, String>::new());
    let invocation_count = Arc::new(AtomicUsize::new(0));

    // Spawn leader task
    let f1 = Arc::clone(&flight);
    let c1 = Arc::clone(&invocation_count);
    let leader_handle = tokio::spawn(async move {
        f1.execute_or_join("cancellable_key".to_string(), || async move {
            c1.fetch_add(1, Ordering::SeqCst);
            sleep(Duration::from_millis(500)).await;
            "leader_result".to_string()
        })
        .await
    });

    // Wait slightly so leader definitely starts and enters execute_or_join
    sleep(Duration::from_millis(30)).await;
    assert_eq!(flight.active_count(), 1);

    // Spawn follower task
    let f2 = Arc::clone(&flight);
    let c2 = Arc::clone(&invocation_count);
    let follower_handle = tokio::spawn(async move {
        f2.execute_or_join("cancellable_key".to_string(), || async move {
            c2.fetch_add(1, Ordering::SeqCst);
            sleep(Duration::from_millis(50)).await;
            "follower_takeover_result".to_string()
        })
        .await
    });

    sleep(Duration::from_millis(20)).await;

    // Leader is cancelled (simulating client disconnecting prematurely)
    leader_handle.abort();

    // Follower must not deadlock, but must safely become the new leader and complete!
    let follower_res = follower_handle.await.unwrap();
    assert_eq!(follower_res, "follower_takeover_result");

    // Two executions occurred: first by cancelled leader, second by takeover follower
    assert_eq!(invocation_count.load(Ordering::SeqCst), 2);
    assert_eq!(flight.active_count(), 0);
}

#[test]
fn test_jitter_bounds_and_distribution() {
    let base_secs = 300;
    let jitter_pct = 10;

    let mut distinct = std::collections::HashSet::new();

    for _ in 0..100 {
        let jittered = calculate_jittered_ttl(base_secs, jitter_pct);
        assert!(
            jittered >= 270,
            "Jittered TTL {jittered} below minimum 270s"
        );
        assert!(
            jittered <= 330,
            "Jittered TTL {jittered} above maximum 330s"
        );
        distinct.insert(jittered);
    }

    // Must have good randomization spread
    assert!(
        distinct.len() > 10,
        "Jitter should produce varied TTLs, got {} distinct values",
        distinct.len()
    );

    // Zero jitter returns exact base
    assert_eq!(calculate_jittered_ttl(base_secs, 0), base_secs);
    assert_eq!(calculate_jittered_ttl_ms(1000, 0), 1000);
}

#[test]
fn test_xfetch_probabilistic_early_expiration() {
    let now = 1_000_000;

    // 1. Expired key (now >= expiry) must always recompute
    assert!(should_recompute_xfetch(now, now - 100, 100, 1.0));

    // 2. Beta <= 0 disables XFetch
    assert!(!should_recompute_xfetch(now, now + 10_000, 100, 0.0));

    // 3. Zero delta disables early recomputation
    assert!(!should_recompute_xfetch(now, now + 10_000, 0, 1.0));

    // 4. Large remaining TTL compared to tiny loader delta almost never recomputes
    let expiry_far = now + 3_600_000; // 1 hour away
    let delta_tiny = 50;
    assert!(!should_recompute_xfetch(now, expiry_far, delta_tiny, 1.0));

    // 5. Very small remaining TTL compared to high delta with beta = 1.0 will recompute
    let expiry_near = now + 1; // 1 ms away
    let delta_large = 5_000; // 5000 ms loader duration
    let mut triggered = 0;
    for _ in 0..100 {
        if should_recompute_xfetch(now, expiry_near, delta_large, 1.0) {
            triggered += 1;
        }
    }
    assert!(
        triggered >= 90,
        "Expected high trigger rate for near-expiry with heavy delta, got {triggered}"
    );
}
