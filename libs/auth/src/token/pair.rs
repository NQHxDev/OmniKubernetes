//! `TokenPair` data contract for client responses.

use serde::{Deserialize, Serialize};

/// Cặp Token trả về cho Client sau khi đăng nhập hoặc làm mới phiên thành công.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenPair {
    /// Access token dạng JWT chuỗi nén (Header.Payload.Signature).
    pub access_token: String,
    /// Refresh token dạng JWT.
    pub refresh_token: String,
    /// Loại mã thông báo (mặc định luôn là "Bearer").
    pub token_type: String,
    /// Thời gian sống của access token tính bằng giây.
    pub expires_in: u64,
}

impl TokenPair {
    /// Khởi tạo [`TokenPair`] chuẩn Bearer.
    #[must_use]
    pub fn new(access_token: String, refresh_token: String, expires_in: u64) -> Self {
        Self {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_pair_new() {
        let pair = TokenPair::new("acc_token".into(), "ref_token".into(), 3600);
        assert_eq!(pair.access_token, "acc_token");
        assert_eq!(pair.refresh_token, "ref_token");
        assert_eq!(pair.token_type, "Bearer");
        assert_eq!(pair.expires_in, 3600);
    }

    #[test]
    fn test_token_pair_serde() {
        let pair = TokenPair::new("jwt_access".into(), "jwt_refresh".into(), 900);
        let json = serde_json::to_string(&pair).unwrap();
        let deserialized: TokenPair = serde_json::from_str(&json).unwrap();
        assert_eq!(pair, deserialized);
    }
}
