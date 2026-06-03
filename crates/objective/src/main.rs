mod app;
mod pipeline;

use std::path::PathBuf;

use anyhow::Context;
use objective_core::{telemetry::init_tracing, ObjectiveConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "serve".to_string());
    let config_path = std::env::var("OBJECTIVE_CONFIG").ok().map(PathBuf::from);
    let config =
        ObjectiveConfig::load(config_path).context("failed to load Objective configuration")?;
    init_tracing(&config.logging.level);

    match command.as_str() {
        "serve" => app::serve(config).await,
        "setup" => app::setup(config).await,
        "--version" | "version" => {
            println!("objective {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        other => anyhow::bail!("unknown command: {other}"),
    }
}
