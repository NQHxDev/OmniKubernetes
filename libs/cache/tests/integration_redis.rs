use std::time::Duration;

use omni_cache::client::CacheClient;
use omni_cache::typed::CachePolicy;
use omni_common::config::{AppConfig, AppEnvironment, ConfigLoader};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LiveItem {
    id: String,
    content: String,
}

#[tokio::test]
async fn test_live_redis_integration_suite() {
    let redis_url = match std::env::var("OMNI_TEST_REDIS_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => {
            eprintln!(
                "SKIPPING: OMNI_TEST_REDIS_URL not set; skipping live Redis integration tests"
            );
            return;
        }
    };

    let app_config: AppConfig = ConfigLoader::for_test("cache-test-service")
        .with_environment(AppEnvironment::Dev)
        .with_override("redis.enabled", "true")
        .with_override("redis.url", redis_url.as_str())
        .load()
        .expect("Failed to build test AppConfig");

    let client = CacheClient::connect(&app_config)
        .await
        .expect("Failed to connect to live Redis test instance");

    // 1. Test TypedCache on live Redis
    let cache = client
        .cache::<LiveItem>("live-item", CachePolicy::default())
        .expect("Failed to construct TypedCache");

    let item = LiveItem {
        id: "live_001".into(),
        content: "Integration Test Content".into(),
    };

    // Cache-aside miss then hit
    let item_clone = item.clone();
    let res1 = cache
        .get_or_load("live_001", move || {
            let item = item_clone.clone();
            async move { Ok(Some(item)) }
        })
        .await
        .expect("Live get_or_load miss failed");
    assert_eq!(res1, Some(item.clone()));

    let res2 = cache
        .get_or_load("live_001", || async {
            panic!("Loader should not be called on cache hit");
        })
        .await
        .expect("Live get_or_load hit failed");
    assert_eq!(res2, Some(item.clone()));

    // Invalidate
    cache
        .invalidate("live_001")
        .await
        .expect("Live invalidate failed");
    let after_del = cache.get("live_001").await.expect("Live get failed");
    assert_eq!(after_del, None);

    // 2. Test DistributedLock on live Redis
    let lock1 = client
        .lock("live-lock", "res_live_123")
        .expect("Failed to create lock");
    let lock2 = client
        .lock("live-lock", "res_live_123")
        .expect("Failed to create lock");

    let mut guard1 = lock1.try_lock().await.expect("Lock1 acquire failed");
    assert!(guard1.fencing_token() >= 1);

    // Contention check
    let res_contended = lock2.try_lock().await;
    assert!(res_contended.is_err());

    // Watchdog heartbeats for 500ms
    sleep(Duration::from_millis(500)).await;

    // Release
    let released = guard1.release().await.expect("Release failed");
    assert!(released);

    // Re-acquire by lock2
    let guard2 = lock2.try_lock().await.expect("Lock2 acquire failed");
    assert!(guard2.fencing_token() > guard1.fencing_token());
    drop(guard2);

    let _ = client.shutdown(Duration::from_secs(2)).await;
}
