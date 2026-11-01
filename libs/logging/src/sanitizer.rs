//! Data Sanitization & PII Scrubbing Policy Implementation.
//!
//! Enforces zero-leak compliance (Nghị định 13/2023/NĐ-CP & GDPR) by masking sensitive credentials,
//! tokens, and Personally Identifiable Information (PII) before emitting log records.

use std::borrow::Cow;

/// Canonical redacted placeholder.
pub const REDACTED: &str = "[REDACTED]";

/// Determines whether a field name matches known sensitive PII / secret keys.
pub fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "authorization"
            | "proxy-authorization"
            | "token"
            | "access_token"
            | "refresh_token"
            | "password"
            | "secret"
            | "api_key"
            | "apikey"
            | "pin"
            | "cvv"
            | "credit_card"
            | "card_number"
            | "id_number"
            | "cccd"
            | "cmnd"
            | "ssn"
            | "email"
    )
}

/// Sanitizes a value based on field name rules.
pub fn sanitize_field<'a>(field_name: &str, value: &'a str) -> Cow<'a, str> {
    let lower = field_name.to_ascii_lowercase();
    match lower.as_str() {
        "authorization" | "proxy-authorization" => Cow::Owned(mask_authorization_header(value)),
        "token" | "access_token" | "refresh_token" | "api_key" | "apikey" => {
            Cow::Owned(mask_token(value))
        }
        "password" | "secret" | "pin" | "cvv" => Cow::Borrowed(REDACTED),
        "credit_card" | "card_number" => Cow::Owned(mask_credit_card(value)),
        "id_number" | "cccd" | "cmnd" | "ssn" => Cow::Owned(mask_id_number(value)),
        "email" => Cow::Owned(mask_email(value)),
        _ => Cow::Borrowed(value),
    }
}

/// Masks HTTP `Authorization` header (`Bearer eyJhbG...[REDACTED]`).
pub fn mask_authorization_header(val: &str) -> String {
    let trimmed = val.trim();
    if let Some(token) = trimmed.strip_prefix("Bearer ") {
        format!("Bearer {}", mask_token(token))
    } else if let Some(token) = trimmed.strip_prefix("bearer ") {
        format!("Bearer {}", mask_token(token))
    } else {
        mask_token(trimmed)
    }
}

/// Masks tokens by keeping the first 6 characters and replacing the remainder with `[REDACTED]`.
pub fn mask_token(val: &str) -> String {
    let trimmed = val.trim();
    if trimmed.chars().count() <= 6 {
        return REDACTED.to_string();
    }
    let prefix: String = trimmed.chars().take(6).collect();
    format!("{prefix}...{REDACTED}")
}

/// Masks credit card numbers, preserving only the last 4 digits.
///
/// Example: `1234-5678-9012-3456` -> `****-****-****-3456`
pub fn mask_credit_card(val: &str) -> String {
    let digits_only: String = val.chars().filter(char::is_ascii_digit).collect();
    if digits_only.len() < 4 {
        return REDACTED.to_string();
    }
    let last4: String = digits_only
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("****-****-****-{last4}")
}

/// Masks national identity numbers (CCCD/CMND), preserving first 3 and last 3 digits.
///
/// Example: `001202019982` -> `001***982`
pub fn mask_id_number(val: &str) -> String {
    let trimmed = val.trim();
    let char_count = trimmed.chars().count();
    if char_count < 6 {
        return REDACTED.to_string();
    }
    let prefix: String = trimmed.chars().take(3).collect();
    let suffix: String = trimmed
        .chars()
        .rev()
        .take(3)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{prefix}***{suffix}")
}

/// Masks email addresses by keeping the first and last letter of the local part.
///
/// Example: `nguyen@example.com` -> `n***n@example.com`
pub fn mask_email(val: &str) -> String {
    let trimmed = val.trim();
    let parts: Vec<&str> = trimmed.split('@').collect();
    if parts.len() != 2 {
        return REDACTED.to_string();
    }
    let local = parts[0];
    let domain = parts[1];

    let local_len = local.chars().count();
    let masked_local = if local_len <= 2 {
        format!("{}*", local.chars().next().unwrap_or('*'))
    } else {
        let first = local.chars().next().unwrap_or('*');
        let last = local.chars().last().unwrap_or('*');
        format!("{first}***{last}")
    };

    format!("{masked_local}@{domain}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_authorization_header() {
        let auth = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        assert_eq!(
            mask_authorization_header(auth),
            "Bearer eyJhbG...[REDACTED]"
        );

        let short_auth = "Bearer secret";
        assert_eq!(mask_authorization_header(short_auth), "Bearer [REDACTED]");
    }

    #[test]
    fn test_mask_secrets() {
        assert_eq!(sanitize_field("password", "super_secret_123"), REDACTED);
        assert_eq!(sanitize_field("pin", "1234"), REDACTED);
        assert_eq!(sanitize_field("cvv", "999"), REDACTED);
    }

    #[test]
    fn test_mask_credit_card() {
        assert_eq!(
            mask_credit_card("1234-5678-9876-4321"),
            "****-****-****-4321"
        );
        assert_eq!(mask_credit_card("12"), REDACTED);
    }

    #[test]
    fn test_mask_id_number() {
        assert_eq!(mask_id_number("001095012345"), "001***345");
        assert_eq!(mask_id_number("12345"), REDACTED);
    }

    #[test]
    fn test_mask_email() {
        assert_eq!(mask_email("nguyen@example.com"), "n***n@example.com");
        assert_eq!(mask_email("me@domain.com"), "m*@domain.com");
        assert_eq!(mask_email("invalid-email"), REDACTED);
    }
}
