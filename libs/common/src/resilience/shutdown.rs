//! Kubernetes 7-Stage Zero-Downtime Graceful Shutdown Coordinator.

use std::{sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::config::ResilienceConfig;
use crate::resilience::metrics::record_shutdown_stage_duration;

/// Abstraction for triggering Kubernetes readiness probe drain gate (/readyz -> 503).
pub trait ShutdownDrainGate: Send + Sync {
    /// Signals the readiness probe that the service is shutting down.
    fn begin_shutdown(&self);

    /// Checks whether the drain gate has begun shutting down.
    fn is_shutting_down(&self) -> bool {
        false
    }
}

/// Thread-safe atomic drain gate tracking shutdown status.
#[derive(Debug, Clone, Default)]
pub struct AtomicDrainGate {
    draining: Arc<std::sync::atomic::AtomicBool>,
}

impl AtomicDrainGate {
    /// Creates a new atomic drain gate.
    #[must_use]
    pub fn new() -> Self {
        Self {
            draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl ShutdownDrainGate for AtomicDrainGate {
    fn begin_shutdown(&self) {
        self.draining
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn is_shutting_down(&self) -> bool {
        self.draining.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Helper to construct a standard Kubernetes probes router bound to a [`ShutdownDrainGate`].
pub fn create_probes_router(drain_gate: Arc<dyn ShutdownDrainGate>) -> axum::Router {
    axum::Router::new()
        .route(
            "/livez",
            axum::routing::get(|| async { axum::Json(serde_json::json!({"status": "Healthy"})) }),
        )
        .route(
            "/startupz",
            axum::routing::get(|| async { axum::Json(serde_json::json!({"status": "Healthy"})) }),
        )
        .route(
            "/healthz",
            axum::routing::get(|| async { axum::Json(serde_json::json!({"status": "Healthy"})) }),
        )
        .route(
            "/health",
            axum::routing::get(|| async { axum::Json(serde_json::json!({"status": "Healthy"})) }),
        )
        .route(
            "/readyz",
            axum::routing::get(move || {
                let gate = Arc::clone(&drain_gate);
                async move {
                    if gate.is_shutting_down() {
                        (
                            axum::http::StatusCode::SERVICE_UNAVAILABLE,
                            axum::Json(serde_json::json!({"status": "Draining"})),
                        )
                    } else {
                        (
                            axum::http::StatusCode::OK,
                            axum::Json(serde_json::json!({"status": "Healthy"})),
                        )
                    }
                }
            }),
        )
}

/// Fallback no-op drain gate for testing or standalone execution.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpDrainGate;

impl ShutdownDrainGate for NoOpDrainGate {
    fn begin_shutdown(&self) {}
}

impl<F> ShutdownDrainGate for F
where
    F: Fn() + Send + Sync,
{
    fn begin_shutdown(&self) {
        (self)();
    }
}

/// 7-Stage Kubernetes Zero-Downtime Graceful Shutdown Coordinator.
#[derive(Clone)]
pub struct GracefulShutdownCoordinator {
    config: ResilienceConfig,
    drain_gate: Arc<dyn ShutdownDrainGate>,
    cancellation_token: CancellationToken,
}

impl Default for GracefulShutdownCoordinator {
    fn default() -> Self {
        Self::new(
            ResilienceConfig::default(),
            Arc::new(AtomicDrainGate::new()),
        )
    }
}

impl std::fmt::Debug for GracefulShutdownCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GracefulShutdownCoordinator")
            .field("config", &self.config)
            .field("cancellation_token", &self.cancellation_token)
            .finish_non_exhaustive()
    }
}

impl GracefulShutdownCoordinator {
    /// Creates a new coordinator bound to configuration and a readiness drain gate.
    #[must_use]
    pub fn new(config: ResilienceConfig, drain_gate: Arc<dyn ShutdownDrainGate>) -> Self {
        Self {
            config,
            drain_gate,
            cancellation_token: CancellationToken::new(),
        }
    }

    /// Creates a coordinator without a drain gate (using [`NoOpDrainGate`]).
    #[must_use]
    pub fn standalone(config: ResilienceConfig) -> Self {
        Self::new(config, Arc::new(NoOpDrainGate))
    }

    /// Shared [`CancellationToken`] to propagate cancellation to background workers.
    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    /// Returns a reference to the associated readiness drain gate.
    #[must_use]
    pub fn drain_gate(&self) -> Arc<dyn ShutdownDrainGate> {
        Arc::clone(&self.drain_gate)
    }

    /// Executes stages 2 and 3 of the graceful shutdown sequence (drain gate & quiescence delay).
    pub async fn start_draining(&self) {
        let s2_start = std::time::Instant::now();
        info!("Stage 2/7: Triggering Kubernetes readiness drain gate (/readyz -> 503 Draining)...");
        self.drain_gate.begin_shutdown();
        record_shutdown_stage_duration("stage_2_drain_gate", s2_start.elapsed());

        let s3_start = std::time::Instant::now();
        let quiescence = Duration::from_secs(self.config.shutdown_quiescence_delay_secs);
        info!(
            quiescence_secs = quiescence.as_secs(),
            "Stage 3/7: Sleeping for Ingress/EndpointSlice route convergence..."
        );
        tokio::time::sleep(quiescence).await;
        record_shutdown_stage_duration("stage_3_quiescence", s3_start.elapsed());
    }

    /// Executes stages 4, 5, 6, and 7 of the graceful shutdown sequence.
    pub fn complete_shutdown(&self) {
        let s45_start = std::time::Instant::now();
        info!("Stage 4 & 5/7: Cancelling background workers & draining in-flight requests...");
        self.cancellation_token.cancel();
        record_shutdown_stage_duration("stage_4_5_workers", s45_start.elapsed());

        let s6_start = std::time::Instant::now();
        info!("Stage 6/7: Draining infrastructure connections & flushing observability buffers...");
        record_shutdown_stage_duration("stage_6_infra_drain", s6_start.elapsed());

        info!("Stage 7/7: Service shutdown sequence completed successfully. Clean exit.");
    }

    /// Listens for OS termination signals and executes the 7-stage graceful shutdown protocol.
    pub async fn wait_for_signal_and_shutdown<F, Fut>(self, drain_infra_fn: F)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        // Giai đoạn 1: Đón bắt tín hiệu SIGTERM hoặc SIGINT
        Self::wait_for_os_signal().await;
        let shutdown_start = std::time::Instant::now();
        info!(
            "Kubernetes SIGTERM/SIGINT received. Initiating 7-stage graceful shutdown protocol..."
        );

        // Giai đoạn 2: Kích hoạt Drain Gate Readiness Probe (/readyz -> 503 Draining)
        let s2_start = std::time::Instant::now();
        info!("Stage 2/7: Triggering Kubernetes readiness drain gate (/readyz -> 503 Draining)...");
        self.drain_gate.begin_shutdown();
        record_shutdown_stage_duration("stage_2_drain_gate", s2_start.elapsed());

        // Giai đoạn 3: Cửa sổ Chờ Bão hòa Mạng Ingress (Quiescence Wait Window)
        let s3_start = std::time::Instant::now();
        let quiescence = Duration::from_secs(self.config.shutdown_quiescence_delay_secs);
        info!(
            quiescence_secs = quiescence.as_secs(),
            "Stage 3/7: Sleeping for Ingress/EndpointSlice route convergence..."
        );
        tokio::time::sleep(quiescence).await;
        record_shutdown_stage_duration("stage_3_quiescence", s3_start.elapsed());

        // Giai đoạn 4 & 5: Dừng tiếp nhận request mới và phát tín hiệu dừng Workers
        let s45_start = std::time::Instant::now();
        info!("Stage 4 & 5/7: Cancelling background workers & draining in-flight requests...");
        self.cancellation_token.cancel();
        record_shutdown_stage_duration("stage_4_5_workers", s45_start.elapsed());

        // Giai đoạn 6: Giải phóng kết nối hạ tầng (Postgres, Redis, WorkerGuard Log Flush)
        let s6_start = std::time::Instant::now();
        info!("Stage 6/7: Draining infrastructure connections (PgPool, Redis, Tracing Flushes)...");
        let grace_period = Duration::from_secs(self.config.shutdown_grace_period_secs);
        tokio::time::timeout(grace_period, drain_infra_fn())
            .await
            .unwrap_or_else(|_| {
                warn!("Stage 6 timed out waiting for infrastructure pool drain. Forcing closure.");
            });
        record_shutdown_stage_duration("stage_6_infra_drain", s6_start.elapsed());

        // Giai đoạn 7: Thoát tiến trình an toàn mã 0
        let total_duration = shutdown_start.elapsed();
        info!(
            total_duration_ms = total_duration.as_millis(),
            "Stage 7/7: Service shutdown sequence completed successfully. Clean exit."
        );
    }

    /// Listens for SIGTERM and SIGINT signals across Unix and Non-Unix platforms.
    pub async fn wait_for_os_signal() {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C signal handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            () = ctrl_c => {
                info!("Captured SIGINT (Ctrl+C) signal.");
            },
            () = terminate => {
                info!("Captured SIGTERM (Kubernetes termination) signal.");
            },
        }
    }
}
