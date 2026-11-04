use omni_cache::key::{KeyHasher, KeySpace, make_lock_keys};
use secrecy::SecretString;

#[test]
fn test_canonical_key_generation_and_hash_tag() {
    let ks = KeySpace::new("omni", "product-service", "product", 1, None)
        .expect("Failed to create KeySpace");

    let key = ks.key("018f4a2b-8a71-7000-8000-000000000001").unwrap();
    assert_eq!(
        key.as_str(),
        "omni:product-service:product:v1:{018f4a2b-8a71-7000-8000-000000000001}"
    );
    assert_eq!(key.resource(), "product");

    // Cluster hash tag check: everything inside {...} is the hash tag
    assert!(
        key.as_str()
            .contains("{018f4a2b-8a71-7000-8000-000000000001}")
    );

    // Lease key generation preserves the hash tag
    let lease_key = key.lease_key();
    assert_eq!(
        lease_key.as_str(),
        "omni:product-service:product:v1:{018f4a2b-8a71-7000-8000-000000000001}:lease"
    );
}

#[test]
fn test_key_with_sub_segments() {
    let ks = KeySpace::new("omni", "product-service", "category-tree", 2, None).unwrap();
    let key = ks
        .key_with(&["tenant-1", "vi-VN"], "root")
        .expect("Failed to build key with segments");

    assert_eq!(
        key.as_str(),
        "omni:product-service:category-tree:v2:tenant-1:vi-VN:{root}"
    );
}

#[test]
fn test_sensitive_pii_key_hmac_masking() {
    let secret = SecretString::from("a_very_secret_hmac_key_at_least_32_bytes_long!");
    let hasher = KeyHasher::new(secret).expect("Hasher creation failed");

    let ks = KeySpace::new(
        "omni",
        "auth-service",
        "user-by-email",
        1,
        Some(hasher.clone()),
    )
    .unwrap();

    let key1 = ks.key_sensitive("alice@example.com").unwrap();
    let key2 = ks.key_sensitive("alice@example.com").unwrap();
    let key3 = ks.key_sensitive("bob@example.com").unwrap();

    // Deterministic hashing
    assert_eq!(key1, key2);
    assert_ne!(key1, key3);

    // Verify key format: starts with omni:auth-service:user-by-email:v1:{h:...}
    assert!(
        key1.as_str()
            .starts_with("omni:auth-service:user-by-email:v1:{h:")
    );
    assert!(key1.as_str().ends_with('}'));
    // Ensure raw email is NEVER present in key
    assert!(!key1.as_str().contains("alice@example.com"));

    // Hasher with secret < 32 bytes must fail
    let short_secret = SecretString::from("too_short_key");
    assert!(KeyHasher::new(short_secret).is_err());
}

#[test]
fn test_sensitive_key_without_hasher_fails() {
    let ks = KeySpace::new("omni", "auth-service", "user-by-email", 1, None).unwrap();
    let res = ks.key_sensitive("alice@example.com");
    assert!(res.is_err());
}

#[test]
fn test_make_lock_keys_cluster_co_location() {
    fn extract_tag(s: &str) -> &str {
        let start = s.find('{').unwrap();
        let end = s.find('}').unwrap();
        &s[start..=end]
    }

    let (lock_key, fence_key) =
        make_lock_keys("omni", "order-service", "order-uuid-9999", "payment-lock").unwrap();

    assert_eq!(
        lock_key,
        "omni:order-service:lock:{order-uuid-9999}:r:payment-lock"
    );
    assert_eq!(fence_key, "omni:order-service:lock:{order-uuid-9999}:fence");

    // Both keys MUST share the exact same hash tag {order-uuid-9999}
    assert_eq!(extract_tag(&lock_key), extract_tag(&fence_key));
    assert_eq!(extract_tag(&lock_key), "{order-uuid-9999}");
}

#[test]
fn test_key_validation_rules() {
    // Schema version 0 must fail
    assert!(KeySpace::new("omni", "user", "profile", 0, None).is_err());

    // Empty segment must fail
    assert!(KeySpace::new("", "user", "profile", 1, None).is_err());

    // Prohibited characters in segment
    assert!(KeySpace::new("omni:bad", "user", "profile", 1, None).is_err());
    assert!(KeySpace::new("omni{bad}", "user", "profile", 1, None).is_err());
    assert!(KeySpace::new("omni bad", "user", "profile", 1, None).is_err());

    // Prohibited characters in id
    let ks = KeySpace::new("omni", "user", "profile", 1, None).unwrap();
    assert!(ks.key("bad{id}").is_err());
    assert!(ks.key("bad}id").is_err());
    assert!(ks.key("bad id").is_err());
    assert!(ks.key("").is_err());
}
