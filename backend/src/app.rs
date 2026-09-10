use std::{sync::Arc, time::Instant};

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
}

impl Application {
    pub async fn initialize(config: AppConfig) -> anyhow::Result<Self> {
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
        let router = api::router(Arc::clone(&state))?;

        Ok(Self { state, router })
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        let address = (
            self.state.config.bind_address,
            self.state.config.port,
        );
        let listener = TcpListener::bind(address)
            .await
            .with_context(|| format!(
                "unable to bind PhaseForge backend to {}:{}",
                self.state.config.bind_address,
                self.state.config.port
            ))?;

        let profile = self.state.scheduler.hardware().profile();
        info!(
            address = %listener.local_addr()?,
            gpu_available = profile.gpu_available,
            adapters = profile.adapters.len(),
            parallel_jobs = profile.scheduler.max_parallel_jobs,
            "PhaseForge backend ready"
        );

        axum::serve(listener, self.router)
            .await
            .context("PhaseForge HTTP server stopped unexpectedly")
    }
}
