use anyhow::Context;
use phaseforge_backend::{app::Application, config::AppConfig};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let mut args=std::env::args().skip(1);
    let mut config_path=None;
    let mut cpu_only=false;
    while let Some(arg)=args.next(){match arg.as_str(){
        "--config"=>config_path=Some(args.next().context("--config requires a TOML path")?),
        "--cpu-only"=>cpu_only=true,
        "--version"=>{println!("PhaseForge {}",env!("CARGO_PKG_VERSION"));return Ok(());},
        "--help"|"-h"=>{println!("PhaseForge local research engine\n  --config PATH  Load TOML configuration\n  --cpu-only     Disable GPU computation\n  --version      Print version");return Ok(());},
        _=>anyhow::bail!("Unknown option: {arg}. Use --help."),
    }}
    let mut config = AppConfig::load_from(config_path.as_deref()).context("failed to load PhaseForge configuration")?;
    if cpu_only{config.gpu_enabled=false;}

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_new(&config.log_filter)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("phaseforge_backend=info")),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    let app = Application::initialize(config).await?;
    app.serve().await
}
