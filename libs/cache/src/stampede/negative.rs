//! Negative Cache Helpers for Non-Existent Entity Defense (`stampede/negative.rs`).
//!
//! Prevents Cache Penetration attacks by caching 12-byte negative sentinels with short TTLs.

use crate::codec::{FLAG_NEGATIVE, HEADER_LEN, MAGIC_BYTES};

/// Kiểm tra một mảng byte raw có phải là Negative Cache Sentinel hợp lệ hay không.
#[must_use]
pub fn is_negative_sentinel(bytes: &[u8]) -> bool {
    if bytes.len() < HEADER_LEN {
        return false;
    }

    if &bytes[0..4] != MAGIC_BYTES {
        return false;
    }

    let flags = bytes[6];
    (flags & FLAG_NEGATIVE) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::encode_envelope;
    use omni_common::config::redis::RedisCompression;

    #[test]
    fn test_negative_sentinel_detection() {
        let env = encode_envelope::<String>(None, 8192, 1024, 1, RedisCompression::None).unwrap();
        assert_eq!(env.len(), HEADER_LEN);
        assert!(is_negative_sentinel(&env));

        let data = "hello".to_string();
        let positive_env =
            encode_envelope(Some(&data), 8192, 1024, 1, RedisCompression::None).unwrap();
        assert!(!is_negative_sentinel(&positive_env));
    }
}
