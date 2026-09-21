use platform_server::{AppState, config::Config, database, router};
use std::{net::SocketAddr, process::ExitCode};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::new("platform_server=info"),
    };
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => {
            tracing::error!(code, "server_failed");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), &'static str> {
    let config = Config::from_env()?;
    let pool = database::pool(&config);
    let state = AppState::new(pool.clone(), config.auth)?;
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .map_err(|_| "SERVER_BIND_FAILED")?;
    tracing::info!(bind = %config.bind, "server_started");
    let result = axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await;
    pool.close().await;
    result.map_err(|_| "SERVER_IO_FAILED")
}

async fn shutdown() {
    let interrupt = async {
        if tokio::signal::ctrl_c().await.is_err() {
            tracing::error!("signal_registration_failed");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => {
                tracing::error!("signal_registration_failed");
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { () = interrupt => {}, () = terminate => {} }
}
