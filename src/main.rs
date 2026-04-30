use clap::Parser;
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

    eprintln!(
        "Starting Burley on HTTP port {} and HTTPS port {}",
        cli.http_port, cli.https_port
    );
    if let (Some(tls_cert), Some(tls_key)) = (&cli.tls_cert, &cli.tls_key) {
        if !tls_cert.exists() {
            eprintln!("TLS cert file {} does not exist", tls_cert.display());
            std::process::exit(1);
        }
        if !tls_key.exists() {
            eprintln!("TLS key file {} does not exist", tls_key.display());
            std::process::exit(1);
        }
        eprintln!(
            "Using TLS with cert {} and key {}",
            tls_cert.display(),
            tls_key.display()
        );
    } else {
        eprintln!("Not using TLS");
    }

    if let Err(err) = burley::server::run_server(cli).await {
        eprintln!("Server error: {err}");
        std::process::exit(1);
    }
}
