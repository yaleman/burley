use clap::Parser;
use std::{num::NonZeroU16, path::PathBuf};

#[derive(Parser)]
pub struct Cli {
    #[clap(default_value = "3128", env = "BURLEY_HTTP_PORT")]
    pub http_port: NonZeroU16,
    #[clap(default_value = "3129", env = "BURLEY_HTTPS_PORT")]
    pub https_port: NonZeroU16,

    #[clap(long, env = "BURLEY_TLS_CERT")]
    pub tls_cert: Option<PathBuf>,
    #[clap(long, env = "BURLEY_TLS_KEY")]
    pub tls_key: Option<PathBuf>,

    #[clap(short, long, env = "BURLEY_DEBUG")]
    pub debug: bool,

    #[clap(long, short, help = "Tokio console mode")]
    pub tokio_console: bool,
}
