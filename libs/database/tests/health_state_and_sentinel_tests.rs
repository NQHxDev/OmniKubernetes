//! Tests for `PoolHealthState` and Kubernetes Readiness probe caching.

use omni_database::PoolHealthState;

#[test]
fn test_pool_health_state_hysteresis_and_evaluation() {
    let health = PoolHealthState::new(3, 10);

    // 1. Initial healthy state
    assert!(health.evaluate(false).is_ok());

    // 2. Closed pool must reject immediately
    let closed_res = health.evaluate(true);
    assert!(closed_res.is_err());
    assert!(closed_res.unwrap_err().contains("closed"));

    // 3. One or two transient errors must NOT trigger unhealthy (Hysteresis / Anti-Flapping)
    health.record_failure("transient connection reset".to_string());
    assert_eq!(health.consecutive_failures(), 1);
    assert!(health.evaluate(false).is_ok());

    health.record_failure("timeout acquire".to_string());
    assert_eq!(health.consecutive_failures(), 2);
    assert!(health.evaluate(false).is_ok());

    // 4. Third failure reaches threshold (3) -> Unhealthy
    health.record_failure("failover in progress".to_string());
    assert_eq!(health.consecutive_failures(), 3);
    let fail_res = health.evaluate(false);
    assert!(fail_res.is_err());
    let err_msg = fail_res.unwrap_err();
    assert!(err_msg.contains("3 consecutive times"));
    assert!(err_msg.contains("failover in progress"));

    // 5. Success resets failure counter and restores health immediately
    health.record_success();
    assert_eq!(health.consecutive_failures(), 0);
    assert!(health.evaluate(false).is_ok());
}
