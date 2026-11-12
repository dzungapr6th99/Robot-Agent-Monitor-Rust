mod startup_app;
use robot_crud_business::traits::CameraBLTrait;

use anyhow::Result;
use axum::serve::ListenerExt;
use robot_common_lib::operation_log;
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
        operation_log::info(module_path!(), format_args!("Starting camera cleanup task"));
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            cleanup_camera.retry_cleanup().await;
        }
    });
    eprintln!("Robot agent listening on {}", listener.local_addr()?);
    let listener = listener.tap_io(|_| {
        operation_log::info(module_path!(), format_args!("Accepted HTTP TCP connection"));
    });
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
    match tokio::time::timeout(Duration::from_secs(15), camera.shutdown()).await {
        Ok(true) => {}
        Ok(false) => operation_log::error(
            module_path!(),
            format_args!("Camera shutdown left cleanup pending"),
        ),
        Err(_) => operation_log::error(module_path!(), format_args!("Camera shutdown timed out")),
    }
    result?;
    Ok(())
}
