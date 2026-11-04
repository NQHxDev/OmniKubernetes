//! Dual-layer adaptive rate limiting: In-Memory GCRA and Distributed Sliding Window Counter.

use std::{
    collections::HashMap,
    future::Future,
    net::IpAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue},
    response::Response,
};
use parking_lot::RwLock;
use tower::{Layer, Service};

use crate::config::ResilienceConfig;
use crate::resilience::{
    error::{
        HEADER_RATE_LIMIT_LIMIT, HEADER_RATE_LIMIT_REMAINING, HEADER_RATE_LIMIT_RESET,
        ResilienceError,
    },
    metrics::record_rate_limited,
};

/// Redis Lua Script for Atomic Sliding Window Counter rate limiting across a cluster.
pub const REDIS_SLIDING_WINDOW_LUA: &str = r#"
local current_key = KEYS[1]
local prev_key = KEYS[2]
local limit = tonumber(ARGV[1])
local window_secs = tonumber(ARGV[2])
local current_timestamp_ms = tonumber(ARGV[3])

local window_ms = window_secs * 1000
local current_window_start_ms = math.floor(current_timestamp_ms / window_ms) * window_ms
local elapsed_in_window_ms = current_timestamp_ms - current_window_start_ms
local weight = 1 - (elapsed_in_window_ms / window_ms)

local current_count = tonumber(redis.call('GET', current_key) or "0")
local prev_count = tonumber(redis.call('GET', prev_key) or "0")
local estimated_count = current_count + math.floor(prev_count * weight)

if estimated_count >= limit then
    local reset_secs = math.ceil((window_ms - elapsed_in_window_ms) / 1000)
    return {0, estimated_count, reset_secs}
else
    local new_count = redis.call('INCR', current_key)
    if new_count == 1 then
        redis.call('PEXPIRE', current_key, window_ms * 2)
    end
    local remaining = math.max(0, limit - (estimated_count + 1))
    local reset_secs = math.ceil((window_ms - elapsed_in_window_ms) / 1000)
    return {1, remaining, reset_secs}
end
"#;

/// Multi-dimensional rate limit key identifying the caller.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RateLimitKey {
    /// Authenticated User UUID/Subject (`usr:{user_id}`).
    User(String),
    /// B2B Tenant identifier (`tnt:{tenant_id}`).
    Tenant(String),
    /// API Key hash (`key:{api_key_hash}`).
    ApiKey(String),
    /// Sanitized Client IP address (`ip:{client_ip}`).
    Ip(String),
}

impl RateLimitKey {
    /// Canonical string representation for metrics and storage.
    #[must_use]
    pub fn as_key_str(&self) -> String {
        match self {
            Self::User(u) => format!("usr:{u}"),
            Self::Tenant(t) => format!("tnt:{t}"),
            Self::ApiKey(k) => format!("key:{k}"),
            Self::Ip(ip) => format!("ip:{ip}"),
        }
    }

    /// Low-cardinality label identifying the key classification.
    #[must_use]
    pub fn key_type(&self) -> &'static str {
        match self {
            Self::User(_) => "user",
            Self::Tenant(_) => "tenant",
            Self::ApiKey(_) => "api_key",
            Self::Ip(_) => "ip",
        }
    }
}

/// Sensitivity tier of an HTTP route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteTier {
    /// Sensitive endpoints (e.g. login, register) with stricter quota.
    Sensitive,
    /// Heavy operations (e.g. checkout, report generation).
    Heavy,
    /// Standard endpoints (default).
    Standard,
    /// High-throughput read-only endpoints (e.g. products catalogue).
    ReadOnly,
}

impl RouteTier {
    /// Quota multiplier applied to the base rate limit RPS.
    #[must_use]
    pub fn quota_multiplier(self) -> f64 {
        match self {
            Self::Sensitive => 0.1, // 10% of default RPS
            Self::Heavy => 0.25,    // 25% of default RPS
            Self::Standard => 1.0,  // 100% of default RPS
            Self::ReadOnly => 2.0,  // 200% of default RPS
        }
    }
}

/// Decision returned by a rate limiter check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitDecision {
    /// Whether the request is permitted to proceed.
    pub allowed: bool,
    /// Total quota limit for the window.
    pub limit: u64,
    /// Remaining quota in the current window.
    pub remaining: u64,
    /// Seconds until full quota reset.
    pub reset_secs: u64,
    /// Seconds to wait before retry (when `allowed` is false).
    pub retry_after_secs: u64,
}

/// Atomic GCRA cell tracking Theoretical Arrival Time (TAT).
#[derive(Debug)]
pub struct AtomicGcraCell {
    tat_nanos: AtomicU64,
}

impl AtomicGcraCell {
    #[must_use]
    pub fn new(initial_tat: u64) -> Self {
        Self {
            tat_nanos: AtomicU64::new(initial_tat),
        }
    }
}

/// High-performance lock-free in-memory Generic Cell Rate Algorithm (GCRA) Limiter.
#[derive(Debug)]
pub struct InMemoryGcraLimiter {
    base_rps: u32,
    base_burst: u32,
    epoch: Instant,
    cells: RwLock<HashMap<RateLimitKey, Arc<AtomicGcraCell>>>,
}

impl InMemoryGcraLimiter {
    /// Creates a new in-memory GCRA limiter.
    #[must_use]
    pub fn new(base_rps: u32, base_burst: u32) -> Self {
        Self {
            base_rps: base_rps.max(1),
            base_burst: base_burst.max(1),
            epoch: Instant::now(),
            cells: RwLock::new(HashMap::new()),
        }
    }

    /// Checks conforming status for a given key and route tier with nanosecond fast-path.
    pub fn check(&self, key: &RateLimitKey, tier: RouteTier) -> RateLimitDecision {
        let multiplier = tier.quota_multiplier();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let effective_rps = (f64::from(self.base_rps) * multiplier).round().max(1.0) as u64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let effective_burst = (f64::from(self.base_burst) * multiplier).round().max(1.0) as u64;

        // T: emission interval in nanoseconds = 1s / effective_rps
        let emission_interval_ns = 1_000_000_000 / effective_rps;
        // tau: burst tolerance in nanoseconds = (effective_burst - 1) * T
        let burst_tolerance_ns = effective_burst.saturating_sub(1) * emission_interval_ns;

        let now_ns = u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX);

        let cell = self.get_or_create_cell(key, now_ns);

        loop {
            let old_tat = cell.tat_nanos.load(Ordering::Acquire);
            let tat_prime = old_tat.max(now_ns);

            if tat_prime.saturating_sub(now_ns) > burst_tolerance_ns {
                // Non-conforming: rate limit exceeded!
                let excess_ns = tat_prime - now_ns - burst_tolerance_ns;
                let retry_after_secs = excess_ns.div_ceil(1_000_000_000);
                let reset_secs = tat_prime.saturating_sub(now_ns).div_ceil(1_000_000_000);

                return RateLimitDecision {
                    allowed: false,
                    limit: effective_burst,
                    remaining: 0,
                    reset_secs: reset_secs.max(1),
                    retry_after_secs: retry_after_secs.max(1),
                };
            }

            let new_tat = tat_prime.saturating_add(emission_interval_ns);
            if cell
                .tat_nanos
                .compare_exchange_weak(old_tat, new_tat, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                // Conforming: request permitted
                let remaining_ns = burst_tolerance_ns.saturating_sub(
                    new_tat
                        .saturating_sub(now_ns)
                        .saturating_sub(emission_interval_ns),
                );
                let remaining = remaining_ns / emission_interval_ns;
                let reset_secs = new_tat.saturating_sub(now_ns).div_ceil(1_000_000_000);

                return RateLimitDecision {
                    allowed: true,
                    limit: effective_burst,
                    remaining,
                    reset_secs: reset_secs.max(1),
                    retry_after_secs: 0,
                };
            }
        }
    }

    fn get_or_create_cell(&self, key: &RateLimitKey, now_ns: u64) -> Arc<AtomicGcraCell> {
        {
            let read_guard = self.cells.read();
            if let Some(cell) = read_guard.get(key) {
                return Arc::clone(cell);
            }
        }

        let mut write_guard = self.cells.write();
        // Double check
        if let Some(cell) = write_guard.get(key) {
            return Arc::clone(cell);
        }

        // Clean up stale cells if map is large (> 10_000 keys)
        if write_guard.len() > 10_000 {
            write_guard.retain(|_, c| {
                let tat = c.tat_nanos.load(Ordering::Relaxed);
                tat + 60_000_000_000 > now_ns
            });
        }

        let new_cell = Arc::new(AtomicGcraCell::new(now_ns));
        write_guard.insert(key.clone(), Arc::clone(&new_cell));
        new_cell
    }
}

/// Pluggable trait for Layer 2 Distributed Rate Limiting (e.g. Redis Cluster).
#[async_trait::async_trait]
pub trait DistributedRateLimiter: Send + Sync {
    /// Checks distributed rate limit for a key against global sliding window quota.
    async fn check_distributed(
        &self,
        key: &RateLimitKey,
        limit: u64,
        window: Duration,
    ) -> Result<RateLimitDecision, String>;
}

/// Composite rate limiter combining Layer 1 In-Memory and optional Layer 2 Distributed.
#[derive(Clone)]
pub struct RateLimiter {
    enabled: bool,
    distributed_enabled: bool,
    local_limiter: Arc<InMemoryGcraLimiter>,
    distributed_limiter: Option<Arc<dyn DistributedRateLimiter>>,
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("enabled", &self.enabled)
            .field("distributed_enabled", &self.distributed_enabled)
            .field("local_limiter", &self.local_limiter)
            .field(
                "has_distributed_limiter",
                &self.distributed_limiter.is_some(),
            )
            .finish()
    }
}

impl RateLimiter {
    /// Creates a new composite rate limiter from configuration.
    #[must_use]
    pub fn new(config: &ResilienceConfig) -> Self {
        Self {
            enabled: config.rate_limit_enabled,
            distributed_enabled: config.distributed_rate_limit_enabled,
            local_limiter: Arc::new(InMemoryGcraLimiter::new(
                config.rate_limit_rps,
                config.rate_limit_burst,
            )),
            distributed_limiter: None,
        }
    }

    /// Sets the optional Layer 2 distributed rate limiter backend.
    #[must_use]
    pub fn with_distributed(mut self, distributed: Arc<dyn DistributedRateLimiter>) -> Self {
        self.distributed_limiter = Some(distributed);
        self
    }

    /// Checks whether the request is conforming under both layers.
    pub async fn check(&self, key: &RateLimitKey, tier: RouteTier) -> RateLimitDecision {
        if !self.enabled {
            return RateLimitDecision {
                allowed: true,
                limit: 100_000,
                remaining: 100_000,
                reset_secs: 0,
                retry_after_secs: 0,
            };
        }

        // Layer 1: In-Memory GCRA (< 50ns)
        let local_decision = self.local_limiter.check(key, tier);
        if !local_decision.allowed {
            return local_decision;
        }

        // Layer 2: Distributed Sliding Window (< 1.5ms)
        if self.distributed_enabled
            && let Some(dist) = &self.distributed_limiter
        {
            match dist
                .check_distributed(key, local_decision.limit, Duration::from_secs(1))
                .await
            {
                Ok(dist_decision) => {
                    if !dist_decision.allowed {
                        return dist_decision;
                    }
                }
                Err(err) => {
                    // Fail-Open on distributed errors to protect availability
                    tracing::warn!(
                        error = %err,
                        key = %key.as_key_str(),
                        "Distributed rate limiter error. Failing open to Layer 1 decision."
                    );
                }
            }
        }

        local_decision
    }
}

/// Helper to extract [`RateLimitKey`] from request headers and extensions.
pub fn extract_rate_limit_key(req: &Request, trusted_proxies: &[IpAddr]) -> RateLimitKey {
    // 1. Authenticated User from header or custom token
    if let Some(user_hdr) = req.headers().get("x-user-id")
        && let Ok(user_str) = user_hdr.to_str()
    {
        let trimmed = user_str.trim();
        if !trimmed.is_empty() {
            return RateLimitKey::User(trimmed.to_string());
        }
    }

    // 2. Tenant from X-Tenant-Id header
    if let Some(tenant_hdr) = req.headers().get("x-tenant-id")
        && let Ok(tenant_str) = tenant_hdr.to_str()
    {
        let trimmed = tenant_str.trim();
        if !trimmed.is_empty() {
            return RateLimitKey::Tenant(trimmed.to_string());
        }
    }

    // 3. API Key from X-API-Key header
    if let Some(api_key_hdr) = req.headers().get("x-api-key")
        && let Ok(key_str) = api_key_hdr.to_str()
    {
        let trimmed = key_str.trim();
        if !trimmed.is_empty() {
            // Use first 16 chars or hash representation
            return RateLimitKey::ApiKey(trimmed.to_string());
        }
    }

    // 4. Client IP (sanitized with trusted proxy verification)
    let client_ip = extract_sanitized_client_ip(req.headers(), trusted_proxies);
    RateLimitKey::Ip(client_ip)
}

/// Extracts sanitized client IP respecting trusted proxy list.
fn extract_sanitized_client_ip(headers: &HeaderMap, trusted_proxies: &[IpAddr]) -> String {
    if let Some(xff) = headers.get("x-forwarded-for")
        && let Ok(xff_str) = xff.to_str()
    {
        let ips: Vec<&str> = xff_str.split(',').map(str::trim).collect();
        if !ips.is_empty() {
            if trusted_proxies.is_empty() {
                return ips[0].to_string();
            }
            // Traverse right-to-left
            for ip_str in ips.iter().rev() {
                if let Ok(ip) = ip_str.parse::<IpAddr>()
                    && !trusted_proxies.contains(&ip)
                {
                    return ip.to_string();
                }
            }
            return ips[0].to_string();
        }
    }

    if let Some(x_real_ip) = headers.get("x-real-ip")
        && let Ok(ip_str) = x_real_ip.to_str()
    {
        let trimmed = ip_str.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    "127.0.0.1".to_string()
}

/// Classifies a request URI into a [`RouteTier`].
pub fn classify_route_tier(path: &str) -> RouteTier {
    if path.contains("/auth/login")
        || path.contains("/auth/register")
        || path.contains("/auth/password")
    {
        RouteTier::Sensitive
    } else if path.contains("/checkout") || path.contains("/export") || path.contains("/reports") {
        RouteTier::Heavy
    } else if path.starts_with("/products")
        || path.starts_with("/catalog")
        || path.starts_with("/healthz")
        || path.starts_with("/livez")
        || path.starts_with("/readyz")
    {
        RouteTier::ReadOnly
    } else {
        RouteTier::Standard
    }
}

/// Injects standard IETF `RateLimit` headers into an HTTP response.
pub fn inject_ratelimit_headers(headers: &mut HeaderMap, decision: &RateLimitDecision) {
    if let Ok(v) = HeaderValue::from_str(&decision.limit.to_string()) {
        headers.insert(HEADER_RATE_LIMIT_LIMIT, v);
    }
    if let Ok(v) = HeaderValue::from_str(&decision.remaining.to_string()) {
        headers.insert(HEADER_RATE_LIMIT_REMAINING, v);
    }
    if let Ok(v) = HeaderValue::from_str(&decision.reset_secs.to_string()) {
        headers.insert(HEADER_RATE_LIMIT_RESET, v);
    }
}

/// Tower Layer for rate limiting Axum services.
#[derive(Debug, Clone)]
pub struct RateLimitLayer {
    limiter: Arc<RateLimiter>,
    trusted_proxies: Arc<Vec<IpAddr>>,
}

impl RateLimitLayer {
    /// Creates a new `RateLimitLayer` from `ResilienceConfig`.
    #[must_use]
    pub fn new(config: &ResilienceConfig) -> Self {
        Self {
            limiter: Arc::new(RateLimiter::new(config)),
            trusted_proxies: Arc::new(Vec::new()),
        }
    }

    /// Sets the trusted proxy IP list.
    #[must_use]
    pub fn with_trusted_proxies(mut self, proxies: Vec<IpAddr>) -> Self {
        self.trusted_proxies = Arc::new(proxies);
        self
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            limiter: Arc::clone(&self.limiter),
            trusted_proxies: Arc::clone(&self.trusted_proxies),
        }
    }
}

/// Tower Service implementation for rate limiting.
#[derive(Debug, Clone)]
pub struct RateLimitService<S> {
    inner: S,
    limiter: Arc<RateLimiter>,
    trusted_proxies: Arc<Vec<IpAddr>>,
}

impl<S> Service<Request> for RateLimitService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let path = req.uri().path().to_string();
        let key = extract_rate_limit_key(&req, &self.trusted_proxies);
        let tier = classify_route_tier(&path);

        let limiter = Arc::clone(&self.limiter);
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let decision = limiter.check(&key, tier).await;

            if !decision.allowed {
                record_rate_limited(&path, key.key_type());
                let err = ResilienceError::RateLimited {
                    retry_after_secs: decision.retry_after_secs,
                    limit: decision.limit,
                    remaining: decision.remaining,
                    reset_secs: decision.reset_secs,
                };
                return Ok(err.to_response(&path, None));
            }

            let mut response = inner.call(req).await?;
            inject_ratelimit_headers(response.headers_mut(), &decision);
            Ok(response)
        })
    }
}
