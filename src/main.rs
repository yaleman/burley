use burley::DataStore;
use clap::Parser;
use tempfile::tempdir;

#[tokio::main]
async fn main() {
    let cli = burley::cli::Cli::parse();

    eprintln!(
        "Starting Burley on HTTP port {} and HTTPS port {}",
        cli.http_port, cli.https_port
    );
    if let (Some(tls_cert), Some(tls_key)) = (cli.tls_cert, cli.tls_key) {
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

    let Ok(store_dir) = tempdir() else {
        eprintln!("Failed to create temporary directory for data store");
        std::process::exit(1);
    };

    let mut datastore = DataStore::new(1024 * 1024 * 1024, store_dir.path().to_path_buf());
}
