//! `OmniKubernetes` Enterprise Common Library (`omni-common`).
//!
//! Implements the core architecture standards for `OmniKubernetes` REST Microservices:
//! - **Base Response Envelopes (`response`):** [`ApiResponse<T>`], [`PaginatedResponse<T>`], [`ResponseMeta`],
//!   dual pagination (`Offset` & `Cursor`), and RFC 9110 `201 Created` / `204 No Content` helpers.
//! - **Internationalization & Localization (`i18n`):** 5-step Locale Resolution Waterfall (`RequestLocale`),
//!   zero-allocation BCP 47 subtag canonicalization, and lock-free RCU Mozilla Fluent [`I18nManager`]
//!   powered by `ArcSwap`.
//! - **Observability & Timing Middleware (`middleware`):** W3C `traceparent` & `X-Request-Id` correlation
//!   (`RequestContext`) and W3C `Server-Timing` / `X-Response-Time` latency headers.
//! - **Configuration & Environment Binding (`config`):** Enterprise 5-layer hierarchical configuration,
//!   fail-fast validation, zero-leak secret protection with memory zeroization, and Kubernetes Downward API.

pub mod config;
pub mod i18n;
pub mod middleware;
pub mod resilience;
pub mod response;

pub use config::{
    AppConfig, AppEnvironment, ConfigError, ConfigLoader, ConfigValidationIssue, EmptyCustomConfig,
    ExposeSecret, SecretString,
};

pub use resilience::{
    CircuitBreaker, CircuitBreakerLayer, GracefulShutdownCoordinator, InMemoryGcraLimiter,
    LoadShedController, LoadShedLayer, RateLimitDecision, RateLimitKey, RateLimitLayer,
    RateLimiter, ResilienceError, RetryBudget, RetryPolicy, RouteTier, ShutdownDrainGate,
    TimeoutLayer, circuit_breaker_layer, execute_with_retry, load_shed_layer, rate_limit_layer,
    timeout_layer, with_cancellation_token,
};

pub use i18n::{
    AuthUserClaims, DEFAULT_LOCALE, I18nError, I18nManager, LocaleBundles, ReloadLocalesResult,
    RequestLocale, SUPPORTED_LOCALES, create_reload_locales_router, match_accept_language,
    match_canonical_locale, spawn_sighup_reload_listener,
};
pub use middleware::{
    RequestContext, TraceContext, inject_egress_headers, observability_middleware,
    parse_traceparent, request_context_middleware, timing_middleware,
};
pub use response::{
    ApiResponse, ApiResponseBuilder, CursorPaginationMeta, OffsetPaginationMeta,
    PaginatedCursorResponse, PaginatedOffsetResponse, PaginatedResponse, PaginatedResponseMeta,
    PaginationMeta, ResponseMeta, build_standard_headers, no_content, no_content_from_meta,
};
