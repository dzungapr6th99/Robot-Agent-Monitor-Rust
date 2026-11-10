mod startup_app;

use anyhow::Result;
use startup_app::StartupApp;
use std::{sync::Arc, time::Duration};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    let startup = StartupApp::init_services().await?;
    let listener = TcpListener::bind(startup.address).await?;
    let camera = startup.camera;
    let cleanup_camera = Arc::clone(&camera);
    let cleanup = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            cleanup_camera.retry_cleanup().await;
        }
    });
    eprintln!("Robot agent listening on {}", listener.local_addr()?);
    let result = axum::serve(listener, startup.http_server.router())
        .with_graceful_shutdown(async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("Shutdown signal: {error}");
            }
            let _ = startup.shutdown.send(true);
        })
        .await;
    cleanup.abort();
    let _ = cleanup.await;
    let _ = tokio::time::timeout(Duration::from_secs(15), camera.shutdown()).await;
    result?;
    Ok(())
}
