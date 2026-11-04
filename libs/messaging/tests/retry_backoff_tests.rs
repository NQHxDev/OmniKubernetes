use std::time::Duration;

use omni_messaging::BackoffPolicy;

#[test]
fn test_backoff_attempt_zero_is_zero() {
    let policy = BackoffPolicy {
        initial_backoff: Duration::from_millis(100),
        max_backoff: Duration::from_secs(10),
        multiplier: 2.0,
        jitter: false,
    };

    assert_eq!(policy.calculate_delay(0), Duration::ZERO);
}

#[test]
fn test_deterministic_exponential_progression_without_jitter() {
    let policy = BackoffPolicy {
        initial_backoff: Duration::from_millis(100),
        max_backoff: Duration::from_secs(1),
        multiplier: 2.0,
        jitter: false,
    };

    // attempt 1: 100 * 2^0 = 100ms
    assert_eq!(policy.calculate_delay(1), Duration::from_millis(100));
    // attempt 2: 100 * 2^1 = 200ms
    assert_eq!(policy.calculate_delay(2), Duration::from_millis(200));
    // attempt 3: 100 * 2^2 = 400ms
    assert_eq!(policy.calculate_delay(3), Duration::from_millis(400));
    // attempt 4: 100 * 2^3 = 800ms
    assert_eq!(policy.calculate_delay(4), Duration::from_millis(800));
    // attempt 5: 100 * 2^4 = 1600ms -> capped at 1000ms
    assert_eq!(policy.calculate_delay(5), Duration::from_millis(1000));
}

#[test]
fn test_full_jitter_distribution_stays_within_bounds() {
    let policy = BackoffPolicy {
        initial_backoff: Duration::from_millis(100),
        max_backoff: Duration::from_millis(1000),
        multiplier: 2.0,
        jitter: true,
    };

    for _ in 0..100 {
        let delay_1 = policy.calculate_delay(1);
        assert!(delay_1 <= Duration::from_millis(100));

        let delay_3 = policy.calculate_delay(3);
        assert!(delay_3 <= Duration::from_millis(400));

        let delay_10 = policy.calculate_delay(10);
        assert!(delay_10 <= Duration::from_millis(1000));
    }
}
