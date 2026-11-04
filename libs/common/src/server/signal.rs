//! Cross-platform OS termination signal listeners.

use tracing::info;

/// Waits for POSIX SIGTERM termination signal on Unix platforms.
#[cfg(unix)]
pub async fn wait_for_sigterm() {
    use tokio::signal::unix::{SignalKind, signal};
    if let Ok(mut sig) = signal(SignalKind::terminate()) {
        sig.recv().await;
    } else {
        std::future::pending::<()>().await;
    }
}

/// No-op fallback for non-Unix platforms.
#[cfg(not(unix))]
pub async fn wait_for_sigterm() {
    std::future::pending::<()>().await;
}

/// Waits for either SIGINT (Ctrl+C) or SIGTERM (Kubernetes).
pub async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for Ctrl+C");
    };

    tokio::select! {
        () = ctrl_c => {
            info!("Received SIGINT (Ctrl+C). Initiating graceful shutdown...");
        }
        () = wait_for_sigterm() => {
            info!("Received SIGTERM from Kubernetes Kubelet. Initiating graceful shutdown...");
        }
    }
}
