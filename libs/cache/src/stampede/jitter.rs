//! Uniform Random TTL Jitter Calculation (`stampede/jitter.rs`).
//!
//! Spreads the expiration times of cache keys uniformly around a base TTL
//! to prevent simultaneous mass-expiration waves.

use rand::Rng;

/// Tính toán TTL có áp dụng Jitter ngẫu nhiên phân bố đều (Uniform Distribution).
///
/// $$\text{TTL}_{\text{actual}} = \text{TTL}_{\text{base}} \times \left(1.0 + \text{rand}(-1.0, 1.0) \times \frac{\text{jitter\_pct}}{100.0}\right)$$
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn calculate_jittered_ttl(base_ttl_secs: u64, jitter_pct: u8) -> u64 {
    if jitter_pct == 0 || base_ttl_secs == 0 {
        return base_ttl_secs;
    }

    let capped_pct = f64::from(jitter_pct.min(50)) / 100.0;
    let mut rng = rand::rng();
    let factor = rng.random_range(-capped_pct..=capped_pct);

    let jittered = (base_ttl_secs as f64) * (1.0 + factor);
    (jittered.round() as u64).max(1)
}

/// Tính toán TTL theo mili-giây có áp dụng Jitter ngẫu nhiên.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn calculate_jittered_ttl_ms(base_ttl_ms: u64, jitter_pct: u8) -> u64 {
    if jitter_pct == 0 || base_ttl_ms == 0 {
        return base_ttl_ms;
    }

    let capped_pct = f64::from(jitter_pct.min(50)) / 100.0;
    let mut rng = rand::rng();
    let factor = rng.random_range(-capped_pct..=capped_pct);

    let jittered = (base_ttl_ms as f64) * (1.0 + factor);
    (jittered.round() as u64).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jitter_within_bounds() {
        let base_ttl = 300;
        let jitter_pct = 10; // bounds: [270, 330]

        for _ in 0..1000 {
            let actual = calculate_jittered_ttl(base_ttl, jitter_pct);
            assert!(
                (270..=330).contains(&actual),
                "actual {actual} outside [270, 330]"
            );
        }
    }

    #[test]
    fn test_zero_jitter_returns_exact() {
        assert_eq!(calculate_jittered_ttl(300, 0), 300);
        assert_eq!(calculate_jittered_ttl_ms(5000, 0), 5000);
    }
}
