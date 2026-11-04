//! Google sqlcommenter specification support.
//!
//! Augments SQL statements with structured comments containing W3C distributed tracing context
//! and application metadata, enabling database administrators to correlate slow queries in PostgreSQL
//! with APM traces in Grafana Tempo.

use std::fmt::Write as _;

/// Định dạng comment theo chuẩn Google sqlcommenter.
#[must_use]
pub fn format_sqlcomment(
    service_name: &str,
    trace_id: Option<&str>,
    span_id: Option<&str>,
) -> String {
    let mut comment = String::with_capacity(96);
    comment.push_str("/*");
    let _ = write!(comment, "application='{}'", url_encode_val(service_name));

    if let Some(trace) = trace_id {
        if let Some(span) = span_id {
            let _ = write!(
                comment,
                ",traceparent='00-{}-{}-01'",
                url_encode_val(trace),
                url_encode_val(span)
            );
        } else {
            let _ = write!(comment, ",trace_id='{}'", url_encode_val(trace));
        }
    }

    comment.push_str("*/");
    comment
}

/// Nối sqlcomment vào sau hoặc trước câu truy vấn SQL.
#[must_use]
pub fn append_sqlcomment(sql: &str, comment: &str) -> String {
    let trimmed = sql.trim_end();
    if let Some(stripped) = trimmed.strip_suffix(';') {
        format!("{stripped} {comment};")
    } else {
        format!("{trimmed} {comment}")
    }
}

fn url_encode_val(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}
