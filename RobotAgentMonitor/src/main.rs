mod startup_app;

use anyhow::Result;
use axum::serve::ListenerExt;
use robot_common_lib::operation_log;
use startup_app::StartupApp;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    let startup = StartupApp::init_services().await?;
    let listener = TcpListener::bind(startup.address).await?;
    eprintln!("Robot agent listening on {}", listener.local_addr()?);
    let listener = listener.tap_io(|_| {
        operation_log::info(module_path!(), format_args!("Accepted HTTP TCP connection"));
    });
    axum::serve(listener, startup.http_server.router())
        .with_graceful_shutdown(async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                eprintln!("Shutdown signal: {error}");
            }
            let _ = startup.shutdown.send(true);
        })
        .await?;
    Ok(())
}
