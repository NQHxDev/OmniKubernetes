use omni_cache::codec::{HEADER_LEN, MAGIC_BYTES, decode_envelope, encode_envelope};
use omni_cache::error::CacheError;
use omni_common::config::redis::RedisCompression;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct UserProfile {
    id: String,
    username: String,
    roles: Vec<String>,
}

#[test]
fn test_encode_decode_roundtrip_uncompressed() {
    let user = UserProfile {
        id: "usr_123".into(),
        username: "alice".into(),
        roles: vec!["admin".into(), "user".into()],
    };

    let encoded = encode_envelope(
        Some(&user),
        10_000, // threshold high -> uncompressed
        1_048_576,
        1,
        RedisCompression::None,
    )
    .expect("Encoding failed");

    assert!(encoded.len() > HEADER_LEN);
    assert_eq!(&encoded[0..4], MAGIC_BYTES);
    assert_eq!(encoded[4], 0x00); // COMPRESSION_NONE
    assert_eq!(encoded[5], 0x01); // FORMAT_JSON
    assert_eq!(encoded[6], 0x00); // flags: not negative
    assert_eq!(encoded[7], 1); // schema_version = 1

    let decoded: Option<UserProfile> =
        decode_envelope(&encoded, 1_048_576, 1).expect("Decoding failed");

    assert_eq!(decoded, Some(user));
}

#[test]
fn test_encode_decode_roundtrip_lz4_compressed() {
    let large_text = "OmniKubernetes Distributed Caching Standard ".repeat(100);
    let user = UserProfile {
        id: "usr_large".into(),
        username: large_text,
        roles: vec!["editor".into()],
    };

    // threshold 128 bytes -> triggers LZ4 compression
    let encoded = encode_envelope(Some(&user), 128, 1_048_576, 2, RedisCompression::Lz4)
        .expect("Encoding failed");

    assert_eq!(&encoded[0..4], MAGIC_BYTES);
    assert_eq!(encoded[4], 0x01); // COMPRESSION_LZ4
    assert_eq!(encoded[7], 2); // schema_version = 2

    let decoded: Option<UserProfile> =
        decode_envelope(&encoded, 1_048_576, 2).expect("Decoding failed");

    assert_eq!(decoded, Some(user));
}

#[test]
fn test_negative_cache_sentinel() {
    // Encoding None creates a negative cache sentinel envelope
    let encoded = encode_envelope::<UserProfile>(None, 1024, 1_048_576, 1, RedisCompression::Lz4)
        .expect("Encoding None failed");

    assert_eq!(encoded.len(), HEADER_LEN);
    assert_eq!(&encoded[0..4], MAGIC_BYTES);
    assert_eq!(encoded[6] & 0x01, 0x01); // FLAG_NEGATIVE set

    let decoded: Option<UserProfile> =
        decode_envelope(&encoded, 1_048_576, 1).expect("Decoding sentinel failed");

    assert_eq!(decoded, None);
}

#[test]
fn test_invalid_magic_bytes_rejection() {
    let mut corrupted = vec![0u8; 32];
    corrupted[0..4].copy_from_slice(b"BAD!");

    let result: Result<Option<UserProfile>, CacheError> = decode_envelope(&corrupted, 1_048_576, 1);

    assert!(result.is_err());
    match result.unwrap_err() {
        CacheError::Codec(msg) => {
            assert!(msg.contains("magic bytes"));
        }
        err => panic!("Unexpected error type: {err:?}"),
    }
}

#[test]
fn test_header_too_short_rejection() {
    let truncated = vec![b'O', b'M', b'C'];

    let result: Result<Option<UserProfile>, CacheError> = decode_envelope(&truncated, 1_048_576, 1);

    assert!(result.is_err());
}

#[test]
fn test_schema_version_mismatch_rejection() {
    let user = UserProfile {
        id: "usr_1".into(),
        username: "bob".into(),
        roles: vec![],
    };

    let encoded = encode_envelope(
        Some(&user),
        1024,
        1_048_576,
        1, // encoded with schema version 1
        RedisCompression::None,
    )
    .unwrap();

    // Decode expecting schema version 2
    let result: Result<Option<UserProfile>, CacheError> = decode_envelope(&encoded, 1_048_576, 2);

    assert!(result.is_err());
    match result.unwrap_err() {
        CacheError::SchemaVersionMismatch { expected, actual } => {
            assert_eq!(expected, 2);
            assert_eq!(actual, 1);
        }
        err => panic!("Unexpected error type: {err:?}"),
    }
}

#[test]
fn test_decompression_bomb_defense_rejection() {
    // Malicious envelope claiming 50MB uncompressed length while max allowed is 1MB
    let mut malicious = Vec::with_capacity(HEADER_LEN + 10);
    malicious.extend_from_slice(MAGIC_BYTES);
    malicious.push(0x01); // LZ4
    malicious.push(0x01); // JSON
    malicious.push(0x00); // flags
    malicious.push(1); // schema version 1
    let huge_claimed_len: u32 = 50 * 1024 * 1024;
    malicious.extend_from_slice(&huge_claimed_len.to_le_bytes());
    malicious.extend_from_slice(&[0x42; 10]); // tiny compressed payload

    let max_allowed = 1_048_576; // 1 MB
    let result: Result<Option<UserProfile>, CacheError> =
        decode_envelope(&malicious, max_allowed, 1);

    assert!(result.is_err());
    match result.unwrap_err() {
        CacheError::DecompressionBomb {
            claimed_bytes,
            max_allowed_bytes,
        } => {
            assert_eq!(claimed_bytes, huge_claimed_len as usize);
            assert_eq!(max_allowed_bytes, max_allowed);
        }
        err => panic!("Unexpected error type: {err:?}"),
    }
}
