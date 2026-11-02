//! Optimal Probabilistic Early Expiration Algorithm (`XFetch`, Vattani et al. VLDB 2015).
//!
//! Recomputes cache values before expiration based on execution cost and approaching expiry:
//!
//! $$\text{now} - (\delta \times \beta \times \ln(\text{rand}())) \ge \text{expiry}$$

use rand::Rng;

/// Đánh giá xem key có thỏa mãn điều kiện kích hoạt nạp sớm `XFetch` hay không.
///
/// # Tham số
/// - `now_ms`: Thời điểm hiện tại (epoch ms).
/// - `expiry_ms`: Thời điểm key sẽ hết hạn trong Redis (epoch ms).
/// - `delta_ms`: Thời gian thực thi của loader trong lần nạp gần nhất (ms).
/// - `beta`: Hệ số nhạy cảm (`xfetch_beta`, mặc định `1.0`). Khi `beta <= 0.0`, `XFetch` bị tắt.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn should_recompute_xfetch(now_ms: u64, expiry_ms: u64, delta_ms: u64, beta: f64) -> bool {
    if beta <= 0.0 || delta_ms == 0 {
        return false;
    }

    if now_ms >= expiry_ms {
        // Đã hết hạn hoàn toàn
        return true;
    }

    let mut rng = rand::rng();
    // Sinh số thực ngẫu nhiên trong khoảng (0.0, 1.0] (tránh 0.0 để không gây ln(0) = -inf)
    let r: f64 = rng.random_range(0.000_000_1..=1.0);
    let ln_r = r.ln(); // Cho giá trị <= 0.0

    // -(delta * beta * ln(r)) >= 0
    let early_window = -((delta_ms as f64) * beta * ln_r);

    (now_ms as f64) + early_window >= (expiry_ms as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xfetch_disabled_when_beta_zero() {
        let now = 1_000_000;
        let expiry = 1_010_000;
        let delta = 200;
        assert!(!should_recompute_xfetch(now, expiry, delta, 0.0));
    }

    #[test]
    fn test_xfetch_always_triggers_when_expired() {
        let now = 1_010_000;
        let expiry = 1_005_000;
        let delta = 200;
        assert!(should_recompute_xfetch(now, expiry, delta, 1.0));
    }

    #[test]
    fn test_xfetch_probabilistic_behavior() {
        let now = 1_000_000;
        let delta = 500;
        let beta = 1.0;

        // Key vừa mới tạo, expiry còn rất xa (300s = 300,000ms)
        let far_expiry = now + 300_000;
        let mut triggered_far = 0;
        for _ in 0..1000 {
            if should_recompute_xfetch(now, far_expiry, delta, beta) {
                triggered_far += 1;
            }
        }
        assert_eq!(
            triggered_far, 0,
            "Should not trigger when expiry is far away"
        );

        // Key sắp hết hạn (còn 100ms, trong khi delta = 500ms)
        let near_expiry = now + 100;
        let mut triggered_near = 0;
        for _ in 0..1000 {
            if should_recompute_xfetch(now, near_expiry, delta, beta) {
                triggered_near += 1;
            }
        }
        assert!(
            triggered_near > 500,
            "Should frequently trigger when expiry is imminent: got {triggered_near}"
        );
    }
}
