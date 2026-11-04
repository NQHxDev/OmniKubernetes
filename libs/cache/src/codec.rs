//! Wire Protocol Binary Envelope, Compression Engine & Decompression Bomb Defense (`codec.rs`).
//!
//! Encapsulates cache items in a fixed 12-byte header containing magic bytes (`OMC1`),
//! compression algorithm, serialization format, schema version, and uncompressed length.

use omni_common::config::redis::RedisCompression;
use serde::{Serialize, de::DeserializeOwned};

use crate::error::CacheError;

/// Chiều dài tiêu đề nhị phân cố định (12 bytes).
pub const HEADER_LEN: usize = 12;

/// Magic bytes nhận diện gói tin `omni-cache` version 1.
pub const MAGIC_BYTES: &[u8; 4] = b"OMC1";

/// Cờ Negative Cache Sentinel (bit 0 = 1: bản ghi không tồn tại trong DB).
pub const FLAG_NEGATIVE: u8 = 0x01;

/// Cờ Stale Content Allowed (bit 1 = 1: dữ liệu stale được chấp nhận khi DB lỗi).
pub const FLAG_STALE_ALLOWED: u8 = 0x02;

/// Định dạng tuần tự hóa JSON chuẩn.
pub const FORMAT_JSON: u8 = 0x01;

/// Mã định danh thuật toán nén trong header envelope.
pub const COMPRESSION_NONE: u8 = 0x00;
pub const COMPRESSION_LZ4: u8 = 0x01;
pub const COMPRESSION_ZSTD: u8 = 0x02;

/// Đóng gói dữ liệu thành Binary Envelope 12-byte header kèm nén tự động.
pub fn encode_envelope<T: Serialize>(
    value: Option<&T>,
    compression_threshold: usize,
    max_value_bytes: usize,
    schema_version: u8,
    prefer_compression: RedisCompression,
) -> Result<Vec<u8>, CacheError> {
    // 1. Trường hợp Negative Cache Sentinel (bản ghi rỗng)
    let Some(val) = value else {
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(MAGIC_BYTES);
        header.push(COMPRESSION_NONE);
        header.push(FORMAT_JSON);
        header.push(FLAG_NEGATIVE);
        header.push(schema_version);
        header.extend_from_slice(&0u32.to_le_bytes());
        return Ok(header);
    };

    // 2. Serialize JSON
    let raw_json = serde_json::to_vec(val)?;
    let uncompressed_len = raw_json.len();

    if uncompressed_len > max_value_bytes {
        return Err(CacheError::DecompressionBomb {
            claimed_bytes: uncompressed_len,
            max_allowed_bytes: max_value_bytes,
        });
    }

    // 3. Quyết định nén
    let (compression_flag, payload_bytes) = if uncompressed_len >= compression_threshold {
        match prefer_compression {
            RedisCompression::Lz4 => {
                #[cfg(feature = "lz4")]
                {
                    (COMPRESSION_LZ4, lz4_flex::compress(&raw_json))
                }
                #[cfg(not(feature = "lz4"))]
                {
                    (COMPRESSION_NONE, raw_json)
                }
            }
            RedisCompression::Zstd => {
                #[cfg(feature = "zstd")]
                {
                    let compressed = zstd::stream::encode_all(&raw_json[..], 3)
                        .map_err(|e| CacheError::Codec(format!("Zstd compression failed: {e}")))?;
                    (COMPRESSION_ZSTD, compressed)
                }
                #[cfg(not(feature = "zstd"))]
                {
                    (COMPRESSION_NONE, raw_json)
                }
            }
            RedisCompression::None => (COMPRESSION_NONE, raw_json),
        }
    } else {
        (COMPRESSION_NONE, raw_json)
    };

    let total_len = HEADER_LEN + payload_bytes.len();
    if total_len > max_value_bytes {
        return Err(CacheError::DecompressionBomb {
            claimed_bytes: total_len,
            max_allowed_bytes: max_value_bytes,
        });
    }

    let uncompressed_len_u32 = u32::try_from(uncompressed_len)
        .map_err(|_| CacheError::Codec("Payload uncompressed length exceeds u32 limit".into()))?;

    // 4. Ghép Envelope Header 12 bytes
    let mut envelope = Vec::with_capacity(total_len);
    envelope.extend_from_slice(MAGIC_BYTES);
    envelope.push(compression_flag);
    envelope.push(FORMAT_JSON);
    envelope.push(0x00); // flags: không phải negative
    envelope.push(schema_version);
    envelope.extend_from_slice(&uncompressed_len_u32.to_le_bytes());
    envelope.extend_from_slice(&payload_bytes);

    Ok(envelope)
}

/// Giải mã Binary Envelope, xác thực Magic bytes, schema version và chặn đứng Decompression Bomb.
pub fn decode_envelope<T: DeserializeOwned>(
    bytes: &[u8],
    max_value_bytes: usize,
    expected_schema_version: u8,
) -> Result<Option<T>, CacheError> {
    if bytes.len() < HEADER_LEN {
        return Err(CacheError::Codec(
            "Payload too short for envelope header".into(),
        ));
    }

    if &bytes[0..4] != MAGIC_BYTES {
        return Err(CacheError::Codec(
            "Invalid cache envelope magic bytes".into(),
        ));
    }

    let compression = bytes[4];
    let flags = bytes[6];
    let schema_ver = bytes[7];
    let uncompressed_len = u32::from_le_bytes(
        bytes[8..12]
            .try_into()
            .map_err(|_| CacheError::Codec("Corrupted length bytes".into()))?,
    ) as usize;

    // 1. Kiểm tra Negative Cache Sentinel
    if (flags & FLAG_NEGATIVE) != 0 {
        return Ok(None);
    }

    // 2. Chống lệch Schema Version
    if schema_ver != expected_schema_version {
        return Err(CacheError::SchemaVersionMismatch {
            expected: expected_schema_version,
            actual: schema_ver,
        });
    }

    // 3. CHỐNG DECOMPRESSION BOMB: Kiểm tra trần trước khi cấp phát bộ nhớ
    if uncompressed_len > max_value_bytes {
        return Err(CacheError::DecompressionBomb {
            claimed_bytes: uncompressed_len,
            max_allowed_bytes: max_value_bytes,
        });
    }

    let payload_slice = &bytes[HEADER_LEN..];

    // 4. Giải nén an toàn
    match compression {
        COMPRESSION_NONE => {
            let data: T = serde_json::from_slice(payload_slice)?;
            Ok(Some(data))
        }
        COMPRESSION_LZ4 => {
            #[cfg(feature = "lz4")]
            {
                let mut out = vec![0u8; uncompressed_len];
                let decompressed_size = lz4_flex::decompress_into(payload_slice, &mut out)
                    .map_err(|e| CacheError::Codec(format!("LZ4 decompression failed: {e}")))?;
                if decompressed_size != uncompressed_len {
                    return Err(CacheError::Codec(format!(
                        "LZ4 decompressed length mismatch: expected {uncompressed_len}, got {decompressed_size}"
                    )));
                }
                let data: T = serde_json::from_slice(&out)?;
                Ok(Some(data))
            }
            #[cfg(not(feature = "lz4"))]
            {
                Err(CacheError::Codec(
                    "LZ4 decompression requested but feature 'lz4' is disabled".into(),
                ))
            }
        }
        COMPRESSION_ZSTD => {
            #[cfg(feature = "zstd")]
            {
                let decoded = zstd::stream::decode_all(payload_slice)
                    .map_err(|e| CacheError::Codec(format!("Zstd decompression failed: {e}")))?;
                if decoded.len() > max_value_bytes {
                    return Err(CacheError::DecompressionBomb {
                        claimed_bytes: decoded.len(),
                        max_allowed_bytes: max_value_bytes,
                    });
                }
                let data: T = serde_json::from_slice(&decoded)?;
                Ok(Some(data))
            }
            #[cfg(not(feature = "zstd"))]
            {
                Err(CacheError::Codec(
                    "Zstd decompression requested but feature 'zstd' is disabled".into(),
                ))
            }
        }
        other => Err(CacheError::Codec(format!(
            "Unsupported compression algorithm code: {other}"
        ))),
    }
}
