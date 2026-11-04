//! Cache Stampede & Thundering Herd Defense in Depth (`stampede/*.rs`).
//!
//! Provides multi-layered defense mechanisms against cache stampedes:
//! 1. In-process sharded Single-Flight future sharing.
//! 2. Uniform random TTL Jitter.
//! 3. Probabilistic early expiration (Vattani VLDB 2015 `XFetch`).
//! 4. Stale-While-Revalidate & Stale-If-Error lifecycle windows.
//! 5. Distributed Lease coordination (Facebook Memcache model).
//! 6. Negative caching for non-existent entities.

pub mod jitter;
pub mod lease;
pub mod negative;
pub mod single_flight;
pub mod swr;
pub mod xfetch;

pub use jitter::{calculate_jittered_ttl, calculate_jittered_ttl_ms};
pub use lease::{DistributedLease, LeaseOutcome};
pub use negative::is_negative_sentinel;
pub use single_flight::SingleFlight;
pub use swr::CacheFreshness;
pub use xfetch::should_recompute_xfetch;
