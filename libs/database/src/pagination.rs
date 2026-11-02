//! Keyset (Cursor-based) Pagination Bridge for PostgreSQL queries.
//!
//! Bridges database keyset queries `(WHERE (created_at, id) < ($1, $2) ORDER BY created_at DESC, id DESC LIMIT $3)`
//! with [`omni_common::response::CursorPaginationMeta`].

use std::borrow::Cow;

use omni_common::response::CursorPaginationMeta;
use uuid::Uuid;

/// Lỗi giải mã con trỏ phân trang (Cursor Error).
#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    /// Định dạng Base64 không hợp lệ.
    #[error("invalid base64 encoding in cursor: {0}")]
    InvalidBase64(String),
    /// Nội dung con trỏ không đúng cấu trúc `timestamp:uuid`.
    #[error("invalid cursor payload format")]
    InvalidPayload,
    /// Dấu thời gian không phải số nguyên hợp lệ.
    #[error("invalid timestamp in cursor")]
    InvalidTimestamp,
    /// UUID không hợp lệ.
    #[error("invalid uuid in cursor: {0}")]
    InvalidUuid(#[from] uuid::Error),
}

/// Con trỏ phân trang Keyset kết hợp timestamp và `UUIDv7`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysetCursor {
    /// Dấu thời gian mili-giây (Unix Epoch).
    pub timestamp_millis: i64,
    /// Định danh `UUIDv7`.
    pub id: Uuid,
}

impl KeysetCursor {
    /// Khởi tạo [`KeysetCursor`] mới.
    #[must_use]
    pub const fn new(timestamp_millis: i64, id: Uuid) -> Self {
        Self {
            timestamp_millis,
            id,
        }
    }

    /// Mã hóa con trỏ thành chuỗi Base64 URL-safe (không padding).
    #[must_use]
    pub fn encode(&self) -> String {
        let raw = format!("{}:{}", self.timestamp_millis, self.id);
        base64_url_encode(raw.as_bytes())
    }

    /// Giải mã chuỗi con trỏ từ Base64 URL-safe.
    pub fn decode(s: &str) -> Result<Self, CursorError> {
        let bytes = base64_url_decode(s).map_err(CursorError::InvalidBase64)?;
        let s = String::from_utf8(bytes).map_err(|_| CursorError::InvalidPayload)?;
        let mut parts = s.split(':');
        let ts_str = parts.next().ok_or(CursorError::InvalidPayload)?;
        let id_str = parts.next().ok_or(CursorError::InvalidPayload)?;
        if parts.next().is_some() {
            return Err(CursorError::InvalidPayload);
        }

        let timestamp_millis = ts_str
            .parse::<i64>()
            .map_err(|_| CursorError::InvalidTimestamp)?;
        let id = Uuid::parse_str(id_str)?;

        Ok(Self {
            timestamp_millis,
            id,
        })
    }
}

/// Tiện ích đóng gói kết quả truy vấn Keyset thành danh sách và metadata phân trang [`CursorPaginationMeta`].
///
/// Yêu cầu câu lệnh SQL truy vấn với `LIMIT limit + 1` để phát hiện trang kế tiếp không cần COUNT.
pub fn paginate_keyset<T>(
    mut items: Vec<T>,
    limit: usize,
    has_prev: bool,
    cursor_fn: impl Fn(&T) -> KeysetCursor,
) -> (Vec<T>, CursorPaginationMeta) {
    let has_next_page = items.len() > limit;
    if has_next_page {
        items.truncate(limit);
    }

    let prev_cursor = if has_prev {
        items
            .first()
            .map(&cursor_fn)
            .map(|c| Cow::Owned(c.encode()))
    } else {
        None
    };
    let next_cursor = if has_next_page {
        items.last().map(&cursor_fn).map(|c| Cow::Owned(c.encode()))
    } else {
        None
    };

    let meta = CursorPaginationMeta {
        next_cursor,
        prev_cursor,
        has_more: has_next_page,
        limit: u32::try_from(limit).unwrap_or(u32::MAX),
    };

    (items, meta)
}

fn base64_url_encode(input: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((input.len() * 4).div_ceil(3));

    let (chunks, remainder) = input.as_chunks::<3>();

    for chunk in chunks {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(CHARSET[((n >> 18) & 63) as usize] as char);
        out.push(CHARSET[((n >> 12) & 63) as usize] as char);
        out.push(CHARSET[((n >> 6) & 63) as usize] as char);
        out.push(CHARSET[(n & 63) as usize] as char);
    }

    match remainder.len() {
        1 => {
            let n = u32::from(remainder[0]) << 16;
            out.push(CHARSET[((n >> 18) & 63) as usize] as char);
            out.push(CHARSET[((n >> 12) & 63) as usize] as char);
        }
        2 => {
            let n = (u32::from(remainder[0]) << 16) | (u32::from(remainder[1]) << 8);
            out.push(CHARSET[((n >> 18) & 63) as usize] as char);
            out.push(CHARSET[((n >> 12) & 63) as usize] as char);
            out.push(CHARSET[((n >> 6) & 63) as usize] as char);
        }
        _ => {}
    }

    out
}

fn base64_url_decode(input: &str) -> Result<Vec<u8>, String> {
    fn decode_char(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' | b'+' => Some(62),
            b'_' | b'/' => Some(63),
            _ => None,
        }
    }

    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity((bytes.len() * 3) / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in bytes {
        if b == b'=' {
            break;
        }
        let val = decode_char(b).ok_or_else(|| format!("invalid base64 byte: {b}"))?;
        buf = (buf << 6) | u32::from(val);
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buf >> bits) & 0xFF).unwrap_or(0));
            buf &= (1 << bits) - 1;
        }
    }

    Ok(out)
}
