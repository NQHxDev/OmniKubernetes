//! # `OmniKubernetes` Distributed Caching & Concurrency Control Standard (`omni-cache`).
//!
//! Provides a thin, explicit, observable distributed caching and concurrency control layer
//! over Redis and Valkey, adhering strictly to enterprise resilience, security, and low-cardinality standards.
//!
//! ## Core Architecture Capabilities
//!
//! 1. **Fail-Open Read Caching:** Reads never fail business requests; Redis errors degrade gracefully to PostgreSQL loader queries.
//! 2. **Fail-Closed Distributed Locking:** Strict concurrency coordination with monotonically increasing Fencing Tokens and Watchdog heartbeats.
//! 3. **6-Layer Stampede Protection:** In-process sharded Single-Flight, TTL Jitter, Vattani VLDB 2015 `XFetch`, SWR/SIE, Distributed Lease, and Negative Caching.
//! 4. **Cluster-Ready Keyspace & Atomic Lua:** Clean key grammar with hash tags `{id}` and `{lock-ns}` for multi-key script safety on Redis Cluster.
//! 5. **Zero-Leak PII & Decompression Bomb Protection:** HMAC-SHA256 masking for sensitive key identifiers and strict uncompressed size pre-allocation checks.

pub mod backend;
pub mod client;
pub mod codec;
pub mod config;
pub mod error;
pub mod health;
pub mod instrument;
pub mod invalidation;
pub mod key;
pub mod limiter;
pub mod lock;
pub mod scripts;
pub mod sentinel;
pub mod stampede;
pub mod typed;

// Re-exports for primary public API
pub use backend::{CacheBackend, MemoryBackend, RedisBackend};
pub use client::CacheClient;
pub use codec::{HEADER_LEN, MAGIC_BYTES, decode_envelope, encode_envelope};
pub use config::{CacheSettings, RedisEndpoint};
pub use error::{CacheError, LockAcquireReason, LockLostReason};
pub use health::CacheHealthState;
#[cfg(feature = "probes")]
pub use health::RedisHealthIndicator;
pub use instrument::{CacheMetricsRecorder, SLOW_COMMAND_THRESHOLD_MS};
pub use invalidation::InvalidationReplayBuffer;
pub use key::{CacheKey, KeyHasher, KeySpace, MAX_KEY_LENGTH, make_lock_keys};
pub use limiter::RedisRateLimiter;
pub use lock::{DistributedLock, LockGuard};
pub use scripts::{
    LuaScripts, SCRIPT_ACQUIRE_WITH_FENCE, SCRIPT_EXTEND, SCRIPT_RELEASE, SCRIPT_WRITE_WITH_LEASE,
};
pub use stampede::{
    CacheFreshness, DistributedLease, LeaseOutcome, SingleFlight, calculate_jittered_ttl,
    calculate_jittered_ttl_ms, is_negative_sentinel, should_recompute_xfetch,
};
pub use typed::{CachePolicy, TypedCache};
