//! Production HTTP Server Runtime & Kubernetes Graceful Shutdown Standard (`omni-common::server`).

pub mod listener;
pub mod signal;

use std::{net::SocketAddr, sync::Arc};

use axum::Router;
pub use listener::bind_reuse_port_listener;
pub use signal::{wait_for_shutdown_signal, wait_for_sigterm};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::resilience::GracefulShutdownCoordinator;

/// Launches an Axum production server with a 7-stage Kubernetes zero-downtime graceful shutdown protocol.
pub async fn serve_with_graceful_shutdown(
    app: Router,
    port: u16,
    coordinator: Arc<GracefulShutdownCoordinator>,
    shutdown_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = bind_reuse_port_listener(addr)?;
    info!(
        transport.port = port,
        transport.bind_address = %addr,
        "Production HTTP server is listening and ready for incoming connections."
    );

    let (server_shutdown_tx, server_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let coord_clone = Arc::clone(&coordinator);
    let token_clone = shutdown_token.clone();

    tokio::spawn(async move {
        tokio::select! {
            res = tokio::signal::ctrl_c() => {
                if let Err(e) = res {
                    tracing::warn!(error = %e, "Error listening for SIGINT");
                }
                info!("Captured SIGINT (Ctrl+C). Initiating 7-stage graceful shutdown...");
            }
            () = wait_for_sigterm() => {
                info!("Captured SIGTERM from Kubernetes Kubelet. Initiating 7-stage graceful shutdown...");
            }
            () = token_clone.cancelled() => {
                info!("Captured internal CancellationToken trigger. Initiating 7-stage graceful shutdown...");
            }
        }

        // Execute Stages 1, 2, and 3: Drain Gate & Quiescence Wait Window
        coord_clone.start_draining().await;

        // Signal Hyper to stop accepting new TCP connections and drain in-flight requests
        let _ = server_shutdown_tx.send(());
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = server_shutdown_rx.await;
        })
        .await?;

    info!("Axum HTTP server completed draining all in-flight HTTP requests.");

    // Execute Stages 4, 5, 6, and 7: Stop workers, drain stateful connections, flush logs
    coordinator.complete_shutdown();
    info!("All microservice resources cleanly drained. Safe exit.");

    Ok(())
}
