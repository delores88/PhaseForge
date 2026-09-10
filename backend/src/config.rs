use std::{env, fs, net::IpAddr, path::PathBuf};

use anyhow::{Context, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub bind_address: IpAddr,
    pub port: u16,
    pub allowed_frontend_origins: Vec<String>,
    pub data_directory: PathBuf,
    pub log_filter: String,
    pub default_compute_policy: String,
    pub max_parallel_jobs: usize,
    pub gpu_enabled: bool,
    pub gpu_preference: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        let data_directory = ProjectDirs::from("science", "PhaseForge", "PhaseForge")
            .map(|dirs| dirs.data_local_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./data"));

        Self {
            bind_address: "127.0.0.1".parse().expect("valid loopback address"),
            port: 7331,
            allowed_frontend_origins: vec![
                "http://127.0.0.1:3000".to_owned(),
                "http://localhost:3000".to_owned(),
            ],
            data_directory,
            log_filter: "phaseforge_backend=info,tower_http=info".to_owned(),
            default_compute_policy: "balanced".to_owned(),
            max_parallel_jobs: 0,
            gpu_enabled: true,
            gpu_preference: "auto".to_owned(),
        }
    }
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from(None)
    }

    pub fn load_from(config_path: Option<&str>) -> anyhow::Result<Self> {
        let defaults = Self::default();
        let mut config = defaults.clone();

        if let Some(path) = config_path.map(str::to_owned).or_else(|| env::var("PHASEFORGE_CONFIG").ok()) {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("unable to read PHASEFORGE_CONFIG at {path}"))?;
            config = toml::from_str(&text).context("invalid PhaseForge TOML configuration")?;
        }

        if config.data_directory.as_os_str().is_empty() {
            config.data_directory = defaults.data_directory;
        }

        if let Ok(value) = env::var("PHASEFORGE_PORT") {
            config.port = value.parse().context("PHASEFORGE_PORT must be a number")?;
        }
        if let Ok(value) = env::var("PHASEFORGE_BIND") {
            config.bind_address = value.parse().context("PHASEFORGE_BIND must be an IP")?;
        }
        if let Ok(value) = env::var("PHASEFORGE_DATA_DIR") {
            config.data_directory = PathBuf::from(value);
        }
        if let Ok(value) = env::var("PHASEFORGE_DISABLE_GPU") {
            config.gpu_enabled = !matches!(value.as_str(), "1" | "true" | "TRUE");
        }

        if !config.bind_address.is_loopback()
            && env::var("PHASEFORGE_ALLOW_REMOTE_BIND").ok().as_deref() != Some("1")
        {
            bail!(
                "PhaseForge refuses a non-loopback bind by default. Set PHASEFORGE_ALLOW_REMOTE_BIND=1 only after configuring authentication."
            );
        }

        fs::create_dir_all(&config.data_directory).with_context(|| {
            format!(
                "unable to create data directory {}",
                config.data_directory.display()
            )
        })?;
        fs::create_dir_all(config.artifacts_directory())?;

        Ok(config)
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_directory.join("phaseforge.sqlite3")
    }

    pub fn artifacts_directory(&self) -> PathBuf {
        self.data_directory.join("artifacts")
    }
}
