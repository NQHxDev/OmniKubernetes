//! BCP 47 & RFC 4647 Language Subtag Canonicalization and `Accept-Language` Quality Matching.
//!
//! Provides zero-allocation canonical locale matching and RFC 9110 `Accept-Language` parsing.

/// List of canonical BCP 47 locales supported by the platform.
pub const SUPPORTED_LOCALES: &[&str] = &["vi-VN", "en-US", "ja-JP"];

/// System-wide fallback default locale when no client preference matches.
pub const DEFAULT_LOCALE: &str = "vi-VN";

/// Canonicalizes and matches a language tag against [`SUPPORTED_LOCALES`] using BCP 47 / RFC 4647 rules
/// with **zero dynamic heap allocations**.
///
/// ### Resolution Strategy:
/// 1. **Exact Match (Case-Insensitive):** Matches full tags (`"vi-VN"`, `"en-us"`, `"ja_JP"`).
/// 2. **Primary Subtag Prefix Match:** Extracts the primary language subtag (`"vi"`, `"en"` from `"en-GB"`)
///    and matches against the primary subtag of [`SUPPORTED_LOCALES`] (`"vi"` -> `"vi-VN"`, `"en-GB"` -> `"en-US"`).
pub fn match_canonical_locale(input: &str) -> Option<&'static str> {
    let raw = input.split(';').next().unwrap_or("").trim();
    if raw.is_empty() || raw == "*" {
        return None;
    }

    // Pass 1: Exact case-insensitive match (supporting both '-' and '_' separators without allocation)
    for &supported in SUPPORTED_LOCALES {
        if eq_locale_tag_ignore_case(supported, raw) {
            return Some(supported);
        }
    }

    // Pass 2: Primary language subtag prefix match (e.g., "vi" -> "vi-VN", "en" or "en-GB" -> "en-US")
    let input_primary = raw.split(['-', '_']).next().unwrap_or(raw);
    if input_primary.is_empty() {
        return None;
    }

    for &supported in SUPPORTED_LOCALES {
        if let Some((lang_prefix, _)) = supported.split_once('-')
            && lang_prefix.eq_ignore_ascii_case(input_primary)
        {
            return Some(supported);
        }
    }

    None
}

/// Compares two locale tags case-insensitively while treating `_` and `-` as equivalent separators,
/// performing zero heap allocations.
#[inline]
fn eq_locale_tag_ignore_case(canonical: &str, candidate: &str) -> bool {
    if canonical.len() != candidate.len() {
        return false;
    }
    canonical
        .bytes()
        .zip(candidate.bytes())
        .all(|(a, b)| match (a, b) {
            (b'-', b'-' | b'_') => true,
            _ => a.eq_ignore_ascii_case(&b),
        })
}

/// Parses an RFC 9110 `Accept-Language` header string (e.g. `"vi,en-US;q=0.9,en;q=0.8"`)
/// into language range slices ordered by descending quality weight (`q`), preserving original
/// order on ties and excluding `q=0`.
pub fn parse_accept_language(header_value: &str) -> Vec<&str> {
    let mut items: Vec<(&str, u16, usize)> = Vec::new();

    for (index, part) in header_value.split(',').enumerate() {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let mut segments = part.split(';');
        let tag = segments.next().unwrap_or("").trim();
        if tag.is_empty() {
            continue;
        }

        let mut q_millis: u16 = 1000;
        let mut valid = true;

        for param in segments {
            let param = param.trim();
            if let Some(q_str) = param
                .strip_prefix("q=")
                .or_else(|| param.strip_prefix("Q="))
            {
                if let Ok(q_val) = q_str.trim().parse::<f32>() {
                    if !(0.0..=1.0).contains(&q_val) {
                        valid = false;
                        break;
                    }
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        q_millis = (q_val * 1000.0).round() as u16;
                    }
                } else {
                    valid = false;
                    break;
                }
            }
        }

        if valid && q_millis > 0 {
            items.push((tag, q_millis, index));
        }
    }

    // Sort by descending quality weight, then ascending original position
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));

    items.into_iter().map(|(tag, _, _)| tag).collect()
}

/// Evaluates an `Accept-Language` header value and returns the highest-priority canonical locale match.
pub fn match_accept_language(header_value: &str) -> Option<&'static str> {
    for candidate in parse_accept_language(header_value) {
        if let Some(matched) = match_canonical_locale(candidate) {
            return Some(matched);
        }
    }
    None
}

