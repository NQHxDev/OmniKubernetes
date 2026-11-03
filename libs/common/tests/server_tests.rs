//! Integration tests for Production HTTP Server and Graceful Shutdown.

use std::sync::Arc;

use axum::{Router, routing::get};
use omni_common::{
    config::ResilienceConfig,
    resilience::{AtomicDrainGate, GracefulShutdownCoordinator},
    server::serve_with_graceful_shutdown,
};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_serve_with_graceful_shutdown_token_cancellation() {
    let app = Router::new().route("/ping", get(|| async { "pong" }));

    let config = ResilienceConfig {
        shutdown_quiescence_delay_secs: 0,
        shutdown_grace_period_secs: 1,
        ..Default::default()
    };

    let drain_gate = Arc::new(AtomicDrainGate::new());
    let coordinator = Arc::new(GracefulShutdownCoordinator::new(config, drain_gate));
    let shutdown_token = CancellationToken::new();

    let token_clone = shutdown_token.clone();
    let coord_clone = Arc::clone(&coordinator);

    let server_handle = tokio::spawn(async move {
        serve_with_graceful_shutdown(app, 0, coord_clone, token_clone).await
    });

    // Wait briefly for the TCP listener to bind
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Trigger graceful shutdown
    shutdown_token.cancel();

    let res = server_handle.await.expect("Server task join failed");
    assert!(res.is_ok(), "Server should exit cleanly with Ok(())");
    assert!(
        coordinator.drain_gate().is_shutting_down(),
        "Drain gate should be in shutting down state"
    );
}
