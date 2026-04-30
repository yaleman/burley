use clap::Parser;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::new("info"),
    };
    if let Err(err) = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init()
    {
        eprintln!("Failed to initialize logging: {err}");
        std::process::exit(1);
    }

    let cli = burley::cli::Cli::parse();

    info!(
        "Starting Burley on HTTP port {} and HTTPS port {}",
        cli.http_port, cli.https_port
    );
    if let (Some(tls_cert), Some(tls_key)) = (&cli.tls_cert, &cli.tls_key) {
        if !tls_cert.exists() {
            error!("TLS cert file {} does not exist", tls_cert.display());
            std::process::exit(1);
        }
        if !tls_key.exists() {
            error!("TLS key file {} does not exist", tls_key.display());
            std::process::exit(1);
        }
        info!(
            "Using TLS with cert {} and key {}",
            tls_cert.display(),
            tls_key.display()
        );
    } else {
        debug!("Not using TLS");
    }

    tokio::select! {
        Ok(()) = tokio::signal::ctrl_c() => {
            // Return
        }
        Some(()) = async move {
            let sigterm = tokio::signal::unix::SignalKind::terminate();
            #[allow(clippy::unwrap_used)]
            tokio::signal::unix::signal(sigterm).unwrap().recv().await
        } => {
            // Return
        }
        Some(()) = async move {
            let sigterm = tokio::signal::unix::SignalKind::alarm();
            #[allow(clippy::unwrap_used)]
            tokio::signal::unix::signal(sigterm).unwrap().recv().await
        } => {
            // Return
        }
        Some(()) = async move {
            let sigterm = tokio::signal::unix::SignalKind::hangup();
            #[allow(clippy::unwrap_used)]
            tokio::signal::unix::signal(sigterm).unwrap().recv().await
        } => {
            // Return
        }
        Some(()) = async move {
            let sigterm = tokio::signal::unix::SignalKind::user_defined1();
            #[allow(clippy::unwrap_used)]
            tokio::signal::unix::signal(sigterm).unwrap().recv().await
        } => {
            // Return
        }
        Some(()) = async move {
            let sigterm = tokio::signal::unix::SignalKind::user_defined2();
            #[allow(clippy::unwrap_used)]
            tokio::signal::unix::signal(sigterm).unwrap().recv().await
        } => {
            // Return
        }

        Err(err) = burley::server::run_server(cli)  => {
            error!("Server error: {err}");
            std::process::exit(1);
        }
    }
    info!("Shutting down...");
}
