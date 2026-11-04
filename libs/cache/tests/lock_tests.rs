use std::sync::Arc;
use std::time::Duration;

use omni_cache::backend::MemoryBackend;
use omni_cache::error::{CacheError, LockAcquireReason, LockLostReason};
use omni_cache::lock::DistributedLock;
use omni_errors::AppError;
use tokio::time::sleep;

fn create_test_lock(
    backend: Arc<MemoryBackend>,
    namespace: &str,
    resource_id: &str,
    ttl: Duration,
) -> DistributedLock<MemoryBackend> {
    DistributedLock::new(
        "omni",
        "order-service",
        namespace,
        resource_id,
        backend,
        ttl,
        0.33,                      // renew at 1/3 of TTL
        Duration::from_millis(10), // retry min delay
        Duration::from_millis(50), // retry max delay
    )
    .expect("Failed to create DistributedLock")
}

#[tokio::test]
async fn test_lock_acquire_success_and_fencing_token_monotonically_increases() {
    let backend = Arc::new(MemoryBackend::new());
    let lock = create_test_lock(backend, "order-checkout", "ord_001", Duration::from_secs(5));

    // First acquisition
    let mut guard1 = lock.try_lock().await.expect("Failed first acquire");
    assert_eq!(guard1.fencing_token(), 1);
    assert_ne!(guard1.owner_token(), "");
    let parsed_token = uuid::Uuid::parse_str(guard1.owner_token()).expect("Valid UUID");
    assert_eq!(parsed_token.get_version(), Some(uuid::Version::SortRand));

    // Release explicitly
    let released = guard1.release().await.expect("Failed to release guard1");
    assert!(released);

    // Second acquisition on the same resource
    let guard2 = lock.try_lock().await.expect("Failed second acquire");
    assert_eq!(guard2.fencing_token(), 2);
    assert_ne!(guard1.owner_token(), guard2.owner_token());
}

#[tokio::test]
async fn test_try_lock_contention_and_error_mapping() {
    let backend = Arc::new(MemoryBackend::new());
    let lock1 = create_test_lock(
        Arc::clone(&backend),
        "order-checkout",
        "ord_002",
        Duration::from_secs(5),
    );
    let lock2 = create_test_lock(backend, "order-checkout", "ord_002", Duration::from_secs(5));

    let _guard1 = lock1.try_lock().await.expect("Failed to acquire lock1");

    // Second try_lock should fail immediately with Contended
    let res = lock2.try_lock().await;
    assert!(res.is_err());

    match res.unwrap_err() {
        CacheError::LockNotAcquired { resource, reason } => {
            assert_eq!(resource, "ord_002");
            assert_eq!(reason, LockAcquireReason::Contended);

            // Test mapping to AppError -> HTTP 409 Conflict
            let app_err = AppError::from(CacheError::LockNotAcquired { resource, reason });
            assert!(matches!(app_err, AppError::Conflict { .. }));
        }
        err => panic!("Unexpected error: {err:?}"),
    }
}

#[tokio::test]
async fn test_lock_with_timeout_retry_success() {
    let backend = Arc::new(MemoryBackend::new());
    let lock1 = create_test_lock(
        Arc::clone(&backend),
        "payment",
        "pay_001",
        Duration::from_secs(5),
    );
    let lock2 = create_test_lock(backend, "payment", "pay_001", Duration::from_secs(5));

    let mut guard1 = lock1.try_lock().await.unwrap();

    // Spawn task to release lock1 after 50ms
    tokio::spawn(async move {
        sleep(Duration::from_millis(50)).await;
        let _ = guard1.release().await;
    });

    // lock2 waits with retry and succeeds
    let guard2 = lock2
        .lock_with_timeout(Duration::from_millis(500))
        .await
        .expect("lock2 should succeed after lock1 releases");

    assert_eq!(guard2.fencing_token(), 2);
}

#[tokio::test]
async fn test_lock_with_timeout_exhausted() {
    let backend = Arc::new(MemoryBackend::new());
    let lock1 = create_test_lock(
        Arc::clone(&backend),
        "payment",
        "pay_002",
        Duration::from_secs(5),
    );
    let lock2 = create_test_lock(backend, "payment", "pay_002", Duration::from_secs(5));

    let _guard1 = lock1.try_lock().await.unwrap();

    // lock2 times out waiting
    let res = lock2.lock_with_timeout(Duration::from_millis(60)).await;
    assert!(res.is_err());

    match res.unwrap_err() {
        CacheError::LockNotAcquired { reason, .. } => {
            assert_eq!(reason, LockAcquireReason::Timeout);
        }
        err => panic!("Expected LockNotAcquired::Timeout, got {err:?}"),
    }
}

#[tokio::test]
async fn test_watchdog_heartbeat_renewal() {
    let backend = Arc::new(MemoryBackend::new());
    // TTL is 150ms, renew interval is 150ms * 0.33 = 50ms
    let lock = create_test_lock(
        Arc::clone(&backend),
        "long-task",
        "task_001",
        Duration::from_millis(150),
    );

    let guard = lock.try_lock().await.expect("Failed to acquire lock");

    // Sleep for 300ms (twice the TTL) - without watchdog, the lock would have expired at 150ms!
    sleep(Duration::from_millis(300)).await;

    // Another caller attempting to acquire must still be blocked because watchdog kept it alive!
    let lock_competing =
        create_test_lock(backend, "long-task", "task_001", Duration::from_millis(150));
    let res = lock_competing.try_lock().await;
    assert!(
        res.is_err(),
        "Lock should still be held thanks to watchdog renewal"
    );

    drop(guard);
}

#[tokio::test]
async fn test_raii_drop_releases_lock() {
    let backend = Arc::new(MemoryBackend::new());
    let lock = create_test_lock(backend, "raii", "res_001", Duration::from_secs(5));

    {
        let _guard = lock.try_lock().await.expect("Acquire failed");
        // guard dropped here
    }

    // Give detached release task a moment to finish
    sleep(Duration::from_millis(20)).await;

    // Must be able to re-acquire immediately
    let guard2 = lock.try_lock().await.expect("Failed to acquire after drop");
    assert_eq!(guard2.fencing_token(), 2);
}

#[tokio::test]
async fn test_lock_loss_signal_on_backend_failure() {
    let backend = Arc::new(MemoryBackend::new());
    // TTL is 100ms, renewal every 33ms
    let lock = create_test_lock(
        Arc::clone(&backend),
        "loss-detect",
        "res_loss",
        Duration::from_millis(100),
    );

    let guard = lock.try_lock().await.unwrap();
    let mut loss_rx = guard.loss_signal();

    // Trigger backend partition
    backend.set_unhealthy(true);

    // Watchdog should fail to renew and notify loss_signal
    let received_loss = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            if loss_rx.changed().await.is_ok()
                && let Some(ref reason) = *loss_rx.borrow()
            {
                return reason.clone();
            }
        }
    })
    .await;

    assert!(
        received_loss.is_ok(),
        "Watchdog did not report lock loss in time"
    );
    assert!(matches!(
        received_loss.unwrap(),
        LockLostReason::RenewalFailed(_)
    ));
}
