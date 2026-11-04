//! Tests for Keyset cursor pagination and bridge to `CursorPaginationMeta`.

use omni_database::{KeysetCursor, paginate_keyset};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestItem {
    id: Uuid,
    timestamp_millis: i64,
}

#[test]
fn test_keyset_cursor_encode_decode_round_trip() {
    let id = Uuid::now_v7();
    let ts = 1_700_000_123_456i64;
    let cursor = KeysetCursor::new(ts, id);

    let encoded = cursor.encode();
    assert!(!encoded.contains('=')); // Base64 URL-safe không padding

    let decoded = KeysetCursor::decode(&encoded).expect("cursor must decode cleanly");
    assert_eq!(cursor, decoded);
}

#[test]
fn test_paginate_keyset_meta_generation() {
    let items = vec![
        TestItem {
            id: Uuid::now_v7(),
            timestamp_millis: 100,
        },
        TestItem {
            id: Uuid::now_v7(),
            timestamp_millis: 90,
        },
        TestItem {
            id: Uuid::now_v7(),
            timestamp_millis: 80,
        },
    ];

    // Case 1: Limit = 2, total items = 3 -> has_more = true, truncated to 2
    let (paged, meta) = paginate_keyset(items.clone(), 2, false, |item| {
        KeysetCursor::new(item.timestamp_millis, item.id)
    });

    assert_eq!(paged.len(), 2);
    assert!(meta.has_more);
    assert_eq!(meta.limit, 2);
    assert!(meta.next_cursor.is_some());
    assert!(meta.prev_cursor.is_none());

    // Case 2: Limit = 5, total items = 3 -> has_more = false, not truncated
    let (paged2, meta2) = paginate_keyset(items, 5, true, |item| {
        KeysetCursor::new(item.timestamp_millis, item.id)
    });

    assert_eq!(paged2.len(), 3);
    assert!(!meta2.has_more);
    assert_eq!(meta2.limit, 5);
    assert!(meta2.next_cursor.is_none());
    assert!(meta2.prev_cursor.is_some());
}
