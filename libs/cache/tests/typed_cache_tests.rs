use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use omni_cache::backend::MemoryBackend;
use omni_cache::health::CacheHealthState;
use omni_cache::instrument::CacheMetricsRecorder;
use omni_cache::invalidation::InvalidationReplayBuffer;
use omni_cache::key::KeySpace;
use omni_cache::typed::{CachePolicy, TypedCache};
use omni_common::config::redis::RedisCompression;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProductDto {
    id: String,
    title: String,
    price_cents: i64,
}

fn create_test_cache(
    backend: Arc<MemoryBackend>,
    mut policy: CachePolicy,
) -> (
    TypedCache<ProductDto, MemoryBackend>,
    Arc<CacheHealthState>,
    Arc<InvalidationReplayBuffer>,
) {
    let ks = KeySpace::new("omni", "product-service", "product", 1, None).unwrap();
    let health = Arc::new(CacheHealthState::new(3, 10, true));
    let replay_buffer = Arc::new(InvalidationReplayBuffer::new(1000));
    let metrics = CacheMetricsRecorder::new("product", "127.0.0.1", 6379);

    policy.prefer_compression = RedisCompression::None;

    let cache = TypedCache::new(
        ks,
        backend,
        policy,
        Arc::clone(&health),
        Arc::clone(&replay_buffer),
        metrics,
    );
    (cache, health, replay_buffer)
}

#[tokio::test]
async fn test_cache_aside_miss_then_hit() {
    let backend = Arc::new(MemoryBackend::new());
    let policy = CachePolicy::default();
    let (cache, _, _) = create_test_cache(backend, policy);

    let load_counter = Arc::new(AtomicUsize::new(0));

    // First call: cache miss, calls loader
    let counter1 = Arc::clone(&load_counter);
    let p1 = cache
        .get_or_load("prod_001", || async move {
            counter1.fetch_add(1, Ordering::SeqCst);
            Ok(Some(ProductDto {
                id: "prod_001".into(),
                title: "Mechanical Keyboard".into(),
                price_cents: 15000,
            }))
        })
        .await
        .expect("First get_or_load failed");

    assert!(p1.is_some());
    assert_eq!(p1.unwrap().title, "Mechanical Keyboard");
    assert_eq!(load_counter.load(Ordering::SeqCst), 1);

    // Second call: cache hit, loader NOT called
    let counter2 = Arc::clone(&load_counter);
    let p2 = cache
        .get_or_load("prod_001", || async move {
            counter2.fetch_add(1, Ordering::SeqCst);
            Ok(Some(ProductDto {
                id: "prod_001".into(),
                title: "Should Not Be Called".into(),
                price_cents: 0,
            }))
        })
        .await
        .expect("Second get_or_load failed");

    assert!(p2.is_some());
    assert_eq!(p2.unwrap().title, "Mechanical Keyboard");
    // Loader count remains 1!
    assert_eq!(load_counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_direct_get_set_invalidate() {
    let backend = Arc::new(MemoryBackend::new());
    let policy = CachePolicy::default();
    let (cache, _, _) = create_test_cache(backend, policy);

    let product = ProductDto {
        id: "prod_002".into(),
        title: "Wireless Mouse".into(),
        price_cents: 8000,
    };

    // Initially None
    let initial = cache.get("prod_002").await.unwrap();
    assert_eq!(initial, None);

    // Set value directly
    cache
        .set("prod_002", Some(&product))
        .await
        .expect("set failed");

    // Read back
    let fetched = cache.get("prod_002").await.unwrap();
    assert_eq!(fetched, Some(product));

    // Invalidate
    cache
        .invalidate("prod_002")
        .await
        .expect("invalidate failed");

    // Read after invalidate
    let after_del = cache.get("prod_002").await.unwrap();
    assert_eq!(after_del, None);
}

#[tokio::test]
async fn test_negative_caching_defense() {
    let backend = Arc::new(MemoryBackend::new());
    let policy = CachePolicy {
        negative_ttl: Duration::from_millis(300),
        ..Default::default()
    };
    let (cache, _, _) = create_test_cache(backend, policy);

    let load_counter = Arc::new(AtomicUsize::new(0));

    // First call: Entity does not exist in DB -> loader returns Ok(None)
    let c1 = Arc::clone(&load_counter);
    let res1 = cache
        .get_or_load("non_existent_001", || async move {
            c1.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        })
        .await
        .unwrap();

    assert_eq!(res1, None);
    assert_eq!(load_counter.load(Ordering::SeqCst), 1);

    // Second call: Cached negative sentinel hit -> loader NOT called!
    let c2 = Arc::clone(&load_counter);
    let res2 = cache
        .get_or_load("non_existent_001", || async move {
            c2.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        })
        .await
        .unwrap();

    assert_eq!(res2, None);
    assert_eq!(load_counter.load(Ordering::SeqCst), 1);

    // Wait for negative TTL to expire
    sleep(Duration::from_millis(350)).await;

    // Third call: Sentinel expired, loader is invoked again
    let c3 = Arc::clone(&load_counter);
    let _ = cache
        .get_or_load("non_existent_001", || async move {
            c3.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        })
        .await;

    assert_eq!(load_counter.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_fail_open_read_on_backend_failure() {
    let backend = Arc::new(MemoryBackend::new());
    let policy = CachePolicy::default();
    let (cache, _, _) = create_test_cache(Arc::clone(&backend), policy);

    let load_counter = Arc::new(AtomicUsize::new(0));

    // Simulate backend network outage
    backend.set_unhealthy(true);

    // Direct get fails open -> returns Ok(None)
    let direct_res = cache.get("prod_failopen").await;
    assert_eq!(direct_res.unwrap(), None);

    // get_or_load fails open -> degrades gracefully to invoking loader!
    let c = Arc::clone(&load_counter);
    let result = cache
        .get_or_load("prod_failopen", || async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(Some(ProductDto {
                id: "prod_failopen".into(),
                title: "Emergency Fallback Item".into(),
                price_cents: 9999,
            }))
        })
        .await;

    // The business request still succeeds!
    assert!(result.is_ok());
    let product = result.unwrap().unwrap();
    assert_eq!(product.title, "Emergency Fallback Item");
    assert_eq!(load_counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_sentinel_bypass_mode() {
    let backend = Arc::new(MemoryBackend::new());
    let policy = CachePolicy::default();
    let (cache, health, _) = create_test_cache(Arc::clone(&backend), policy);

    // Pre-populate cache while healthy
    let product = ProductDto {
        id: "prod_bypass".into(),
        title: "Cached Value".into(),
        price_cents: 100,
    };
    cache.set("prod_bypass", Some(&product)).await.unwrap();

    // Activate Sentinel Bypass
    health.set_bypassed(true);

    // Direct get returns None in bypass mode
    assert_eq!(cache.get("prod_bypass").await.unwrap(), None);

    // get_or_load directly invokes loader, bypassing cache
    let bypass_loader_called = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&bypass_loader_called);
    let res = cache
        .get_or_load("prod_bypass", || async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(Some(ProductDto {
                id: "prod_bypass".into(),
                title: "Fresh DB Value".into(),
                price_cents: 200,
            }))
        })
        .await
        .unwrap();

    assert_eq!(res.unwrap().title, "Fresh DB Value");
    assert_eq!(bypass_loader_called.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_delayed_double_delete() {
    let backend = Arc::new(MemoryBackend::new());
    let policy = CachePolicy::default();
    let (cache, _, _) = create_test_cache(Arc::clone(&backend), policy);

    let product = ProductDto {
        id: "prod_double_del".into(),
        title: "To Be Double Deleted".into(),
        price_cents: 500,
    };

    cache.set("prod_double_del", Some(&product)).await.unwrap();
    assert!(cache.get("prod_double_del").await.unwrap().is_some());

    // Invalidate with delayed double delete (50ms delay)
    cache
        .invalidate_with_double_delete("prod_double_del", Duration::from_millis(50))
        .await
        .unwrap();

    // Immediately deleted
    assert_eq!(cache.get("prod_double_del").await.unwrap(), None);

    // Re-insert right away (simulating a race condition / read-replica lag)
    cache.set("prod_double_del", Some(&product)).await.unwrap();
    assert!(cache.get("prod_double_del").await.unwrap().is_some());

    // Wait 100ms for delayed second deletion to fire
    sleep(Duration::from_millis(100)).await;

    // The delayed deletion successfully wiped out the stale write!
    assert_eq!(cache.get("prod_double_del").await.unwrap(), None);
}

#[tokio::test]
async fn test_invalidation_replay_buffer_on_backend_failure() {
    let backend = Arc::new(MemoryBackend::new());
    let policy = CachePolicy::default();
    let (cache, _, replay_buffer) = create_test_cache(Arc::clone(&backend), policy);

    // Backend network failure
    backend.set_unhealthy(true);

    // Invalidate during network partition
    let _ = cache.invalidate("prod_partitioned").await;

    // Key must be buffered in the replay buffer for later draining!
    assert_eq!(replay_buffer.len(), 1);
    let drained = replay_buffer.drain_all();
    assert_eq!(drained.len(), 1);
    assert!(drained[0].as_str().contains("prod_partitioned"));
}
