//! Canonical Locale Resolution Waterfall Extractor (`RequestLocale`).
//!
//! Implements the 5-step zero-I/O locale resolution priority waterfall:
//! 1. Query Parameter (`?lang=` or `?locale=`)
//! 2. Custom Client Header (`X-Locale`)
//! 3. Authenticated Identity Claims (`AuthUserClaims` extension from JWT token)
//! 4. Standard HTTP Header (`Accept-Language` with RFC 9110 quality weighting & BCP 47 subtag matching)
//! 5. System Fallback Default (`DEFAULT_LOCALE = "vi-VN"`)

use std::{borrow::Cow, convert::Infallible};

use axum::{
    extract::FromRequestParts,
    http::{Extensions, HeaderMap, Uri, header, request::Parts},
};
use serde::{Deserialize, Serialize};

use crate::i18n::matcher::{DEFAULT_LOCALE, match_accept_language, match_canonical_locale};

/// Custom HTTP header name allowing clients (Mobile / SPA) to override OS locale.
pub const X_LOCALE_HEADER: &str = "x-locale";

/// Authenticated user identity claims injected into request extensions by the JWT Auth middleware.
///
/// Enables Step 3 of the Locale Resolution Waterfall with zero database I/O.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthUserClaims {
    /// Subject identifier (e.g. user UUID).
    pub sub: String,
    /// Optional user-configured BCP 47 locale from their profile settings embedded in the JWT.
    pub locale: Option<String>,
}

impl AuthUserClaims {
    /// Creates a new [`AuthUserClaims`] instance.
    pub fn new(sub: impl Into<String>, locale: Option<impl Into<String>>) -> Self {
        Self {
            sub: sub.into(),
            locale: locale.map(Into::into),
        }
    }
}

/// Extracted and canonicalized BCP 47 locale for the current HTTP request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestLocale(pub Cow<'static, str>);

impl RequestLocale {
    /// Returns the resolved BCP 47 locale string slice (e.g. `"vi-VN"`, `"en-US"`, `"ja-JP"`).
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner [`Cow<'static, str>`].
    #[inline]
    pub fn into_inner(self) -> Cow<'static, str> {
        self.0
    }

    /// Resolves the canonical locale from URI, headers, and extensions following the 5-step waterfall.
    pub fn resolve(uri: &Uri, headers: &HeaderMap, extensions: &Extensions) -> Self {
        // Check if already resolved and cached in request extensions
        if let Some(cached) = extensions.get::<RequestLocale>() {
            return cached.clone();
        }

        // [1] Query Parameter (?lang= or ?locale=)
        if let Some(query) = uri.query()
            && let Ok(pairs) =
                serde_urlencoded::from_str::<Vec<(Cow<'_, str>, Cow<'_, str>)>>(query)
        {
            for (key, value) in &pairs {
                if (key == "lang" || key == "locale")
                    && let Some(matched) = match_canonical_locale(value)
                {
                    return Self(Cow::Borrowed(matched));
                }
            }
        }

        // [2] Custom Client Header (X-Locale)
        if let Some(val) = headers.get(X_LOCALE_HEADER).and_then(|v| v.to_str().ok())
            && let Some(matched) = match_canonical_locale(val)
        {
            return Self(Cow::Borrowed(matched));
        }

        // [3] Authenticated Identity Claims (JWT Claim: 'locale' in request extensions - 0 Database I/O)
        if let Some(user_claims) = extensions.get::<AuthUserClaims>()
            && let Some(locale) = &user_claims.locale
            && let Some(matched) = match_canonical_locale(locale)
        {
            return Self(Cow::Borrowed(matched));
        }

        // [4] Standard HTTP Header (Accept-Language with RFC 9110 q-weight & BCP 47 subtag matching)
        if let Some(accept_str) = headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
            && let Some(matched) = match_accept_language(accept_str)
        {
            return Self(Cow::Borrowed(matched));
        }

        // [5] System Fallback Default
        Self(Cow::Borrowed(DEFAULT_LOCALE))
    }

    /// Resolves the canonical locale directly from Axum request [`Parts`].
    pub fn from_parts(parts: &Parts) -> Self {
        Self::resolve(&parts.uri, &parts.headers, &parts.extensions)
    }
}

impl Default for RequestLocale {
    fn default() -> Self {
        Self(Cow::Borrowed(DEFAULT_LOCALE))
    }
}

impl AsRef<str> for RequestLocale {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<S> FromRequestParts<S> for RequestLocale
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let locale = Self::from_parts(parts);
        std::future::ready(Ok(locale))
    }
}
