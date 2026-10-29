//! Internationalization (i18n) & Localization (l10n) Subsystem.
//!
//! Implements:
//! - BCP 47 & RFC 4647 language subtag canonicalization (`matcher`)
//! - 5-step Locale Resolution Waterfall Axum extractor (`extractor`)
//! - Lock-free RCU Mozilla Fluent bundle manager with hot-reload (`manager`)

pub mod extractor;
pub mod manager;
pub mod matcher;

pub use extractor::{AuthUserClaims, RequestLocale, X_LOCALE_HEADER};
pub use manager::{
    ConcurrentFluentBundle, I18nError, I18nManager, LocaleBundles, ReloadLocalesResult,
    create_reload_locales_router, spawn_sighup_reload_listener,
};
pub use matcher::{
    DEFAULT_LOCALE, SUPPORTED_LOCALES, match_accept_language, match_canonical_locale,
    parse_accept_language,
};

