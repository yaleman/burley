use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

pub fn init_logging(cli: &crate::cli::Cli) -> Result<(), ExitCode> {
    if cli.tokio_console {
        console_subscriber::init();
        if cli.debug {
            eprintln!("Tokio console enabled, won't be debug-logging!");
        }
    } else {
        let env_filter = match cli.debug {
            true => EnvFilter::new("debug"),
            false => EnvFilter::new("info"),
        };
        if let Err(err) = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(cli.debug)
            .try_init()
        {
            eprintln!("Failed to initialize logging: {err}");
            return Err(ExitCode::FAILURE);
        }
    }
    Ok(())
}
