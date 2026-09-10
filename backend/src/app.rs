use std::{io::Write, sync::Arc, time::Instant};

use anyhow::Context;
use axum::Router;
use tokio::net::TcpListener;
use tracing::info;

use crate::{
    agent::AgentService,
    api,
    compute::{HardwareManager, Scheduler},
    config::AppConfig,
    persistence::Database,
};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub database: Database,
    pub scheduler: Scheduler,
    pub agent: AgentService,
    pub discovery: crate::discovery::DiscoveryService,
    pub assurance: crate::assurance::AssuranceService,
    pub started_at: Instant,
    pub telemetry: crate::compute::telemetry::Telemetry,
}

pub struct Application {
    state: Arc<AppState>,
    router: Router,
    listener: TcpListener,
    desktop_endpoint: Option<String>,
}

impl Application {
    pub async fn initialize(mut config: AppConfig) -> anyhow::Result<Self> {
        // Capture and validate the per-launch identity once. The announcement
        // and readiness route must refer to this same owned process launch.
        let readiness = api::desktop_ready::Readiness::from_environment()?;
        if readiness.enabled() && !config.bind_address.is_loopback() {
            anyhow::bail!("A desktop backend must bind a loopback address");
        }
        // Keep the OS-assigned socket open throughout initialization; never
        // reserve/release a candidate port or construct admission with port 0.
        let listener = bind_listener(&mut config).await?;
        let desktop_endpoint = endpoint_announcement(&config, readiness.enabled())?;
        let database = Database::open(&config.database_path())?;
        let hardware = HardwareManager::discover(&config)
            .await
            .context("hardware discovery failed")?;
        if let Err(error) = rayon::ThreadPoolBuilder::new()
            .num_threads(hardware.profile().scheduler.cpu_worker_threads)
            .thread_name(|index| format!("phaseforge-cpu-{index}"))
            .build_global()
        {
            tracing::debug!(%error, "Rayon global pool was already initialized");
        }
        let scheduler = Scheduler::new(database.clone(), hardware)?;
        let agent = AgentService::new(database.clone(), scheduler.clone())?;

        let discovery = crate::discovery::DiscoveryService::new(database.clone(), scheduler.clone(), agent.clone())?;
        let assurance = crate::assurance::AssuranceService::new(database.clone())?;
        let state = Arc::new(AppState {
            config: config.clone(),
            database,
            scheduler,
            agent,
            discovery,
            assurance,
            started_at: Instant::now(),
            telemetry: crate::compute::telemetry::Telemetry::start(),
        });
        let router = api::router_with_readiness(Arc::clone(&state), readiness)?;

        Ok(Self { state, router, listener, desktop_endpoint })
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        let profile = self.state.scheduler.hardware().profile();
        info!(
            address = %self.listener.local_addr()?,
            gpu_available = profile.gpu_available,
            adapters = profile.adapters.len(),
            parallel_jobs = profile.scheduler.max_parallel_jobs,
            "PhaseForge backend ready"
        );

        if let Some(line) = self.desktop_endpoint {
            let mut output = std::io::stdout().lock();
            writeln!(output, "{line}").context("unable to announce the desktop endpoint")?;
            output.flush().context("unable to flush the desktop endpoint")?;
        }
        axum::serve(self.listener, self.router)
            .await
            .context("PhaseForge HTTP server stopped unexpectedly")
    }
}

pub(crate) async fn bind_listener(config: &mut AppConfig) -> anyhow::Result<TcpListener> {
    let listener = TcpListener::bind((config.bind_address, config.port))
        .await
        .with_context(|| format!("unable to bind PhaseForge backend to {}:{}", config.bind_address, config.port))?;
    config.port = listener.local_addr()?.port();
    Ok(listener)
}

fn endpoint_announcement(config: &AppConfig, enabled: bool) -> anyhow::Result<Option<String>> {
    if !enabled { return Ok(None); }
    anyhow::ensure!(config.bind_address.is_loopback() && config.port != 0, "Desktop endpoint must be a bound loopback socket");
    // Bounded metadata only. The launch secret is never serialized or logged.
    Ok(Some(format!("PHASEFORGE_DESKTOP_ENDPOINT {}", serde_json::json!({
        "address": config.bind_address.to_string(), "port": config.port,
        "version": env!("CARGO_PKG_VERSION"),
    }))))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn dynamic_listener_remains_owned_and_announcement_contains_only_public_endpoint() {
        let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = AppConfig {port: 0, ..AppConfig::default()};
        let listener = bind_listener(&mut config).await.unwrap();
        assert_ne!(config.port, 0);
        assert_ne!(config.port, occupied.local_addr().unwrap().port());
        assert_eq!(listener.local_addr().unwrap().port(), config.port);
        assert!(TcpListener::bind(listener.local_addr().unwrap()).await.is_err());
        assert_eq!(endpoint_announcement(&config, false).unwrap(), None);
        let line = endpoint_announcement(&config, true).unwrap().unwrap();
        assert!(line.len() < 512 && !line.contains('\n'));
        let metadata: serde_json::Value = serde_json::from_str(line.strip_prefix("PHASEFORGE_DESKTOP_ENDPOINT ").unwrap()).unwrap();
        assert_eq!(metadata.as_object().unwrap().len(), 3);
        assert_eq!(metadata["address"], "127.0.0.1");
        assert_eq!(metadata["port"], config.port);
        assert_eq!(metadata["version"], env!("CARGO_PKG_VERSION"));
        assert!(endpoint_announcement(&AppConfig {port: 0, ..config.clone()}, true).is_err());
        assert!(endpoint_announcement(&AppConfig {bind_address: "0.0.0.0".parse().unwrap(), ..config}, true).is_err());
        let address = listener.local_addr().unwrap();
        drop(listener);
        assert!(TcpListener::bind(address).await.is_ok());
    }
}
