//! Lock-Free RCU Mozilla Fluent Bundle Manager (`I18nManager`).
//!
//! Manages localization bundles in memory using `Arc<ArcSwap<LocaleBundles>>` (Read-Copy-Update pattern):
//! - **Hot Path (Read):** Lock-free $O(1)$ atomic pointer load via `ArcSwap::load()`, zero mutex/rwlock contention.
//! - **Reload Path (Write):** Parses and validates new `.ftl` resources on an isolated heap allocation;
//!   only swaps the atomic pointer if 100% of resources pass syntax & completeness checks.
//! - **Reload Triggers:** Internal HTTP endpoint (`/internal/v1/reload-locales`) and Unix `SIGHUP` signal handler.

use std::{
    borrow::Cow,
    collections::HashMap,
    fmt, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use arc_swap::ArcSwap;
use axum::{
    Extension, Json, Router,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use fluent_bundle::{FluentArgs, FluentResource, FluentValue, concurrent::FluentBundle};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use unic_langid::LanguageIdentifier;
use utoipa::ToSchema;

use crate::{
    i18n::{
        extractor::RequestLocale,
        matcher::{DEFAULT_LOCALE, SUPPORTED_LOCALES, match_canonical_locale},
    },
    middleware::context::RequestContext,
    response::{ApiResponse, ResponseMeta},
};

/// Thread-safe concurrent Fluent bundle type (`Send + Sync`).
pub type ConcurrentFluentBundle = FluentBundle<FluentResource>;

/// Map from canonical BCP 47 locale tag (e.g. `"vi-VN"`) to its compiled [`ConcurrentFluentBundle`].
pub type LocaleBundles = HashMap<String, ConcurrentFluentBundle>;

macro_rules! include_locale {
    ($path:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/", $path))
    };
}

// Compile-time embedded default `.ftl` resources for zero-disk-I/O initialization & resilience.
const VI_VN_COMMON: &str = include_locale!("vi-VN/common.ftl");
const VI_VN_ERRORS: &str = include_locale!("vi-VN/errors.ftl");
const VI_VN_MESSAGES: &str = include_locale!("vi-VN/messages.ftl");

const EN_US_COMMON: &str = include_locale!("en-US/common.ftl");
const EN_US_ERRORS: &str = include_locale!("en-US/errors.ftl");
const EN_US_MESSAGES: &str = include_locale!("en-US/messages.ftl");

const JA_JP_COMMON: &str = include_locale!("ja-JP/common.ftl");
const JA_JP_ERRORS: &str = include_locale!("ja-JP/errors.ftl");
const JA_JP_MESSAGES: &str = include_locale!("ja-JP/messages.ftl");

/// Errors that can occur when loading or hot-reloading Fluent `.ftl` localization resources.
#[derive(Debug, Error)]
pub enum I18nError {
    /// Failed to read `.ftl` directory or file from disk.
    #[error("Failed to read locale resource from disk at '{path}': {source}")]
    Io {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Invalid BCP 47 language identifier.
    #[error("Invalid BCP 47 language identifier '{locale}': {details}")]
    InvalidLanguageId {
        /// Invalid locale string.
        locale: String,
        /// Parser error details.
        details: String,
    },

    /// Syntax error in a `.ftl` Fluent resource file.
    #[error("Fluent syntax validation failed for locale '{locale}' ({resource}): {details}")]
    SyntaxError {
        /// Target locale.
        locale: String,
        /// Resource name or file path.
        resource: String,
        /// Fluent parser errors.
        details: String,
    },

    /// Duplicate message keys or bundle registration failure.
    #[error("Failed to register Fluent resource '{resource}' into bundle '{locale}': {details}")]
    BundleRegistrationError {
        /// Target locale.
        locale: String,
        /// Resource name or file path.
        resource: String,
        /// Error details.
        details: String,
    },

    /// Required locale directory or bundle is missing during reload validation.
    #[error("Missing required locale bundle '{locale}' during validation")]
    MissingRequiredLocale {
        /// Missing canonical locale.
        locale: String,
    },
}

/// Lock-free RCU internationalization manager backed by `Arc<ArcSwap<LocaleBundles>>`.
#[derive(Clone)]
pub struct I18nManager {
    bundles: Arc<ArcSwap<LocaleBundles>>,
    locales_dir: Option<Arc<PathBuf>>,
}

impl fmt::Debug for I18nManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let guard = self.bundles.load();
        let mut locales: Vec<&str> = guard.keys().map(String::as_str).collect();
        locales.sort_unstable();
        f.debug_struct("I18nManager")
            .field("loaded_locales", &locales)
            .field("locales_dir", &self.locales_dir)
            .finish()
    }
}

impl Default for I18nManager {
    fn default() -> Self {
        Self::new_embedded().expect("Embedded Fluent locale files must be syntactically valid")
    }
}

impl I18nManager {
    /// Initializes an [`I18nManager`] from compile-time embedded `.ftl` resources (`vi-VN`, `en-US`, `ja-JP`).
    ///
    /// Guarantees zero disk I/O during pod startup and unit tests.
    pub fn new_embedded() -> Result<Self, I18nError> {
        let sources: Vec<(&str, Vec<(&str, &str)>)> = vec![
            (
                "vi-VN",
                vec![
                    ("common.ftl", VI_VN_COMMON),
                    ("errors.ftl", VI_VN_ERRORS),
                    ("messages.ftl", VI_VN_MESSAGES),
                ],
            ),
            (
                "en-US",
                vec![
                    ("common.ftl", EN_US_COMMON),
                    ("errors.ftl", EN_US_ERRORS),
                    ("messages.ftl", EN_US_MESSAGES),
                ],
            ),
            (
                "ja-JP",
                vec![
                    ("common.ftl", JA_JP_COMMON),
                    ("errors.ftl", JA_JP_ERRORS),
                    ("messages.ftl", JA_JP_MESSAGES),
                ],
            ),
        ];

        let bundles = Self::build_bundles_from_memory(&sources)?;
        Ok(Self {
            bundles: Arc::new(ArcSwap::from_pointee(bundles)),
            locales_dir: None,
        })
    }

    /// Initializes an [`I18nManager`] by reading and validating `.ftl` files from a filesystem directory
    /// (such as a Kubernetes `ConfigMap` volume mount).
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, I18nError> {
        let path = dir.as_ref().to_path_buf();
        let bundles = Self::load_bundles_from_dir(&path)?;
        Ok(Self {
            bundles: Arc::new(ArcSwap::from_pointee(bundles)),
            locales_dir: Some(Arc::new(path)),
        })
    }

    /// Returns a list of currently loaded canonical locale codes.
    pub fn loaded_locales(&self) -> Vec<String> {
        let guard = self.bundles.load();
        let mut keys: Vec<String> = guard.keys().cloned().collect();
        keys.sort();
        keys
    }

    /// Returns `true` if a message `key` exists in the specified `locale` bundle.
    pub fn has_message(&self, locale: &str, key: &str) -> bool {
        let canonical = match_canonical_locale(locale).unwrap_or(DEFAULT_LOCALE);
        let guard = self.bundles.load();
        guard
            .get(canonical)
            .is_some_and(|bundle| bundle.has_message(key))
    }

    /// Performs a zero-downtime RCU hot-reload from the configured `locales_dir` (or re-validates embedded bundles).
    ///
    /// If any `.ftl` file fails syntax validation or a required locale is missing, the reload is aborted,
    /// a warning is logged, and the active `ArcSwap` bundles remain untouched.
    pub fn reload(&self) -> Result<(), I18nError> {
        if let Some(dir) = &self.locales_dir {
            self.reload_from_dir(dir.as_ref())
        } else {
            let refreshed = Self::new_embedded()?;
            let next = refreshed.bundles.load_full();
            self.bundles.store(next);
            tracing::info!("Successfully reloaded embedded Fluent i18n bundles");
            Ok(())
        }
    }

    /// Performs a zero-downtime RCU hot-reload from the specified filesystem directory.
    pub fn reload_from_dir(&self, dir: impl AsRef<Path>) -> Result<(), I18nError> {
        let path = dir.as_ref();
        match Self::load_bundles_from_dir(path) {
            Ok(next_bundles) => {
                self.bundles.store(Arc::new(next_bundles));
                tracing::info!(
                    locales_dir = %path.display(),
                    "Successfully hot-reloaded Fluent i18n bundles via RCU ArcSwap"
                );
                Ok(())
            }
            Err(err) => {
                tracing::warn!(
                    locales_dir = %path.display(),
                    error = %err,
                    "Aborted Fluent i18n hot-reload due to validation error; retaining previous bundles"
                );
                Err(err)
            }
        }
    }

    /// Performs a zero-downtime RCU hot-reload from in-memory `(locale, Vec<(filename, ftl_content)>)` tuples.
    ///
    /// Validates all resources before swapping the atomic pointer.
    pub fn reload_from_memory(
        &self,
        sources: &[(&str, Vec<(&str, &str)>)],
    ) -> Result<(), I18nError> {
        match Self::build_bundles_from_memory(sources) {
            Ok(next_bundles) => {
                self.bundles.store(Arc::new(next_bundles));
                tracing::info!(
                    "Successfully hot-reloaded in-memory Fluent i18n bundles via RCU ArcSwap"
                );
                Ok(())
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "Aborted in-memory Fluent i18n hot-reload due to validation error; retaining previous bundles"
                );
                Err(err)
            }
        }
    }

    /// Translates a message key without interpolation arguments using lock-free atomic read.
    pub fn translate_simple(&self, locale: &str, key: &str) -> String {
        self.translate(locale, key, None)
    }

    /// Translates a message key with optional [`FluentArgs`] using lock-free atomic read (`bundles.load()`).
    ///
    /// Falls back to [`DEFAULT_LOCALE`] if the key is missing in the requested locale,
    /// and returns `key` if not found in any bundle.
    pub fn translate(&self, locale: &str, key: &str, args: Option<&FluentArgs<'_>>) -> String {
        let canonical = match_canonical_locale(locale).unwrap_or(DEFAULT_LOCALE);
        let guard = self.bundles.load();

        if let Some(rendered) = Self::format_from_bundle(&guard, canonical, key, args) {
            return rendered;
        }

        if canonical != DEFAULT_LOCALE
            && let Some(rendered) = Self::format_from_bundle(&guard, DEFAULT_LOCALE, key, args)
        {
            return rendered;
        }

        key.to_string()
    }

    /// Translates a message key using dynamic JSON parameters (`HashMap<String, serde_json::Value>`).
    ///
    /// Automatically converts JSON numbers, strings, and booleans into [`FluentValue`] arguments.
    pub fn translate_with_params(
        &self,
        locale: &str,
        key: &str,
        params: Option<&HashMap<String, serde_json::Value>>,
    ) -> String {
        let Some(param_map) = params else {
            return self.translate(locale, key, None);
        };

        if param_map.is_empty() {
            return self.translate(locale, key, None);
        }

        // Convert non-primitive values to owned strings first so they outlive FluentArgs
        let owned_strings: HashMap<&str, String> = param_map
            .iter()
            .filter_map(|(k, v)| match v {
                serde_json::Value::String(_) | serde_json::Value::Number(_) => None,
                serde_json::Value::Bool(b) => Some((k.as_str(), b.to_string())),
                other => Some((k.as_str(), other.to_string())),
            })
            .collect();

        let mut fluent_args = FluentArgs::with_capacity(param_map.len());
        for (k, v) in param_map {
            match v {
                serde_json::Value::String(s) => {
                    fluent_args.set(k.as_str(), FluentValue::from(s.as_str()));
                }
                serde_json::Value::Number(n) => {
                    if let Some(f) = n.as_f64() {
                        fluent_args.set(k.as_str(), FluentValue::from(f));
                    }
                }
                _ => {
                    if let Some(s) = owned_strings.get(k.as_str()) {
                        fluent_args.set(k.as_str(), FluentValue::from(s.as_str()));
                    }
                }
            }
        }

        self.translate(locale, key, Some(&fluent_args))
    }

    fn format_from_bundle(
        bundles: &LocaleBundles,
        locale: &str,
        key: &str,
        args: Option<&FluentArgs<'_>>,
    ) -> Option<String> {
        let bundle = bundles.get(locale)?;
        let msg = bundle.get_message(key)?;
        let pattern = msg.value()?;
        let mut errors = Vec::new();
        let value = bundle.format_pattern(pattern, args, &mut errors);
        Some(value.into_owned())
    }

    fn build_bundles_from_memory(
        sources: &[(&str, Vec<(&str, &str)>)],
    ) -> Result<LocaleBundles, I18nError> {
        let mut next_bundles = LocaleBundles::with_capacity(sources.len());

        for (locale, files) in sources {
            let lang_id: LanguageIdentifier =
                locale
                    .parse()
                    .map_err(|err: unic_langid::LanguageIdentifierError| {
                        I18nError::InvalidLanguageId {
                            locale: (*locale).to_string(),
                            details: err.to_string(),
                        }
                    })?;

            let mut bundle = ConcurrentFluentBundle::new_concurrent(vec![lang_id]);
            // Disable Unicode bidi isolation marks (\u{2068} / \u{2069}) for clean JSON API strings
            bundle.set_use_isolating(false);

            for (resource_name, content) in files {
                let resource =
                    FluentResource::try_new((*content).to_string()).map_err(|(_, errs)| {
                        I18nError::SyntaxError {
                            locale: (*locale).to_string(),
                            resource: (*resource_name).to_string(),
                            details: format!("{errs:?}"),
                        }
                    })?;

                bundle.add_resource(resource).map_err(|errs| {
                    I18nError::BundleRegistrationError {
                        locale: (*locale).to_string(),
                        resource: (*resource_name).to_string(),
                        details: format!("{errs:?}"),
                    }
                })?;
            }

            next_bundles.insert((*locale).to_string(), bundle);
        }

        // Validate that all SUPPORTED_LOCALES are present
        for &required in SUPPORTED_LOCALES {
            if !next_bundles.contains_key(required) {
                return Err(I18nError::MissingRequiredLocale {
                    locale: required.to_string(),
                });
            }
        }

        Ok(next_bundles)
    }

    fn load_bundles_from_dir(dir: &Path) -> Result<LocaleBundles, I18nError> {
        let mut raw_sources: Vec<(String, Vec<(String, String)>)> = Vec::new();

        for &locale in SUPPORTED_LOCALES {
            let locale_dir = dir.join(locale);
            if !locale_dir.exists() || !locale_dir.is_dir() {
                return Err(I18nError::MissingRequiredLocale {
                    locale: locale.to_string(),
                });
            }

            let mut files: Vec<(String, String)> = Vec::new();
            let entries = fs::read_dir(&locale_dir).map_err(|source| I18nError::Io {
                path: locale_dir.clone(),
                source,
            })?;

            for entry in entries {
                let entry = entry.map_err(|source| I18nError::Io {
                    path: locale_dir.clone(),
                    source,
                })?;
                let file_path = entry.path();
                if file_path.extension().and_then(|e| e.to_str()) == Some("ftl") {
                    let content =
                        fs::read_to_string(&file_path).map_err(|source| I18nError::Io {
                            path: file_path.clone(),
                            source,
                        })?;
                    let file_name = file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown.ftl")
                        .to_string();
                    files.push((file_name, content));
                }
            }

            if files.is_empty() {
                return Err(I18nError::MissingRequiredLocale {
                    locale: locale.to_string(),
                });
            }

            files.sort_by(|a, b| a.0.cmp(&b.0));
            raw_sources.push((locale.to_string(), files));
        }

        let borrowed_sources: Vec<(&str, Vec<(&str, &str)>)> = raw_sources
            .iter()
            .map(|(loc, files)| {
                let pairs = files
                    .iter()
                    .map(|(name, content)| (name.as_str(), content.as_str()))
                    .collect();
                (loc.as_str(), pairs)
            })
            .collect();

        Self::build_bundles_from_memory(&borrowed_sources)
    }
}

/// Payload returned by the `/internal/v1/reload-locales` administrative endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReloadLocalesResult {
    /// List of active BCP 47 locales after reload.
    pub loaded_locales: Vec<String>,
}

/// Creates an internal router exposing `POST /internal/v1/reload-locales` for triggering RCU locale hot-reloads.
pub fn create_reload_locales_router(manager: I18nManager) -> Router {
    Router::new()
        .route("/internal/v1/reload-locales", post(reload_locales_handler))
        .layer(Extension(manager))
}

async fn reload_locales_handler(
    Extension(manager): Extension<I18nManager>,
    locale: RequestLocale,
    ctx: Option<Extension<RequestContext>>,
) -> Result<
    (
        StatusCode,
        HeaderMap,
        Json<ApiResponse<ReloadLocalesResult>>,
    ),
    (StatusCode, String),
> {
    let meta = ctx.map_or_else(
        || ResponseMeta {
            locale: locale.clone().into_inner(),
            ..Default::default()
        },
        |Extension(c)| c.to_meta(),
    );

    match manager.reload() {
        Ok(()) => {
            let msg = manager.translate_simple(locale.as_str(), "msg-operation-success");
            let data = ReloadLocalesResult {
                loaded_locales: manager.loaded_locales(),
            };
            Ok(ApiResponse::ok(data, Cow::Owned(msg), meta))
        }
        Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    }
}

/// Spawns a background Tokio task listening for Unix `SIGHUP` signals (e.g. Kubernetes `ConfigMap` updates)
/// to trigger zero-downtime RCU hot-reload of Fluent `.ftl` bundles.
#[cfg(unix)]
pub fn spawn_sighup_reload_listener(manager: I18nManager) -> tokio::task::JoinHandle<()> {
    use tokio::signal::unix::{SignalKind, signal};

    tokio::spawn(async move {
        let mut stream = match signal(SignalKind::hangup()) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(error = %err, "Failed to register SIGHUP signal handler for i18n hot-reload");
                return;
            }
        };

        while stream.recv().await.is_some() {
            tracing::info!("Received SIGHUP signal; triggering Fluent i18n bundle hot-reload");
            let _ = manager.reload();
        }
    })
}
