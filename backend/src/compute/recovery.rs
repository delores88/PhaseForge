//! Durable generation boundaries and bounded recovery. A trajectory interrupted inside a
//! generation is replayed; this does not claim arbitrary integrator-state resumption.
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    domain::ExperimentManifest,
    persistence::Database,
    science::search::{Candidate, PopulationState, SearchGeneration},
};

pub const MAX_RESTARTS: u32 = 3;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RecoveryState {
    pub restarts: u32,
    pub deadline: Option<DateTime<Utc>>,
    pub events: Vec<String>,
}

impl RecoveryState {
    pub fn load(database: &Database, id: Uuid) -> anyhow::Result<Self> {
        database.get_run_recovery(id)?.map(serde_json::from_value).transpose()
            .context("invalid durable run recovery state").map(|v| v.unwrap_or_default())
    }

    pub fn save(&self, database: &Database, id: Uuid) -> anyhow::Result<()> {
        database.put_run_recovery(id, &serde_json::to_value(self)?)
    }

    pub fn restart(&mut self, reason: &str) -> anyhow::Result<()> {
        if self.restarts >= MAX_RESTARTS { bail!("Automatic recovery stopped after {MAX_RESTARTS} restarts; the last checkpoint is retained."); }
        if self.deadline.map(|d| d <= Utc::now()).unwrap_or(false) {
            bail!("The original wall-time budget expired. The last checkpoint is retained; automatic recovery does not extend the budget.");
        }
        self.restarts += 1;
        self.events.push(reason.chars().take(2000).collect());
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SearchCheckpoint {
    pub version: u32,
    pub manifest_hash: String,
    pub population_size: usize,
    pub next_generation: usize,
    pub population: Vec<Candidate>,
    pub best: Candidate,
    pub history: Vec<SearchGeneration>,
    pub engine: PopulationState,
    pub used_gpu: bool,
    pub used_cpu: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
pub struct CheckpointStore {
    database: Database,
    run_id: Uuid,
    manifest_hash: String,
}

impl CheckpointStore {
    pub fn new(database: Database, run_id: Uuid, manifest: &ExperimentManifest) -> anyhow::Result<Self> {
        let manifest_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(manifest)?));
        Ok(Self { database, run_id, manifest_hash })
    }

    pub(crate) fn load(&self, population_size: usize, generations: usize) -> anyhow::Result<Option<SearchCheckpoint>> {
        let Some(value) = self.database.get_run_checkpoint(self.run_id)? else { return Ok(None); };
        let checkpoint: SearchCheckpoint = serde_json::from_value(value).context("invalid search checkpoint")?;
        if checkpoint.version != 1 || checkpoint.manifest_hash != self.manifest_hash
            || checkpoint.population_size != population_size || checkpoint.population.len() != population_size
            || checkpoint.next_generation > generations || checkpoint.history.len() != checkpoint.next_generation {
            bail!("Stored checkpoint does not match this immutable manifest and candidate budget; refusing to mix experiments.");
        }
        Ok(Some(checkpoint))
    }

    pub(crate) fn save(&self, mut checkpoint: SearchCheckpoint) -> anyhow::Result<()> {
        checkpoint.version = 1;
        checkpoint.manifest_hash.clone_from(&self.manifest_hash);
        self.database.put_run_checkpoint(self.run_id, &serde_json::to_value(checkpoint)?)
            .context("unable to commit search generation checkpoint")
    }
}

/// Only resource pressure is retryable. Invalid scientific models and numerical
/// divergence must not silently change equations or consume repeated allocations.
#[derive(Debug, thiserror::Error)]
#[error("memory pressure: {0}")]
pub struct ResourcePressure(pub String);

pub fn check_memory_headroom(required_bytes: u64) -> anyhow::Result<()> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let available = system.available_memory();
    let reserve = (system.total_memory() / 20).clamp(128 * 1024 * 1024, 1024 * 1024 * 1024);
    if available < required_bytes.saturating_add(reserve) {
        return Err(ResourcePressure(format!("{} MiB available; {} MiB needed including desktop reserve", available / 1048576, required_bytes.saturating_add(reserve) / 1048576)).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::{ExperimentManifestDraft, SearchAlgorithm}, science::{ode, ProgressCallback}};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    fn manifest(algorithm: SearchAlgorithm) -> ExperimentManifest {
        let mut draft: ExperimentManifestDraft = serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();
        draft.search.enabled = true;
        draft.search.algorithm = algorithm;
        draft.search.variables = serde_json::from_value(serde_json::json!([{"name":"initial_x","target":"initial:x","minimum":0.5,"maximum":2.0}])).unwrap();
        draft.search.objectives = serde_json::from_value(serde_json::json!([{"name":"endpoint","expression":"x*x","goal":"minimize","weight":1.0}])).unwrap();
        draft.search.population = 8;
        draft.search.generations = 4;
        draft.compute.candidate_count = 8;
        draft.compute.batch_size = 2;
        ExperimentManifest::from_draft(Uuid::nil(), None, 1, draft, "recovery test")
    }

    #[tokio::test]
    async fn interrupted_search_resumes_identically_with_rng_and_de_targets() {
        for algorithm in [SearchAlgorithm::DifferentialEvolution, SearchAlgorithm::Evolutionary, SearchAlgorithm::LatinHypercube] {
            let manifest = manifest(algorithm);
            let temp = tempfile::tempdir().unwrap();
            let db = Database::open(&temp.path().join("recovery.sqlite3")).unwrap();
            let id = Uuid::new_v4();
            let store = CheckpointStore::new(db.clone(), id, &manifest).unwrap();
            let noop: ProgressCallback = Arc::new(|_, _| {});
            let expected = ode::execute(manifest.clone(), manifest.compute.clone(), None, CancellationToken::new(), noop.clone()).await.unwrap();
            let cancellation = CancellationToken::new();
            let target = cancellation.clone();
            let interrupt: ProgressCallback = Arc::new(move |_, phase| {
                if phase == "Evaluating candidate generation 2/4" { target.cancel(); }
            });
            assert!(ode::execute_checkpointed(manifest.clone(), manifest.compute.clone(), None, cancellation, interrupt, Some(store.clone())).await.is_err());
            assert_eq!(store.load(8, 4).unwrap().unwrap().next_generation, 1);
            // Reopen the database, as a new backend process would.
            drop(store);
            drop(db);
            let db = Database::open(&temp.path().join("recovery.sqlite3")).unwrap();
            let store = CheckpointStore::new(db.clone(), id, &manifest).unwrap();
            let mut smaller_compute = manifest.compute.clone();
            smaller_compute.batch_size = 1;
            let actual = ode::execute_checkpointed(manifest.clone(), smaller_compute, None, CancellationToken::new(), noop.clone(), Some(store.clone())).await.unwrap();
            assert_eq!(actual.output.best_candidate, expected.output.best_candidate, "algorithm {algorithm:?}");
            assert_eq!(actual.output.metrics, expected.output.metrics, "algorithm {algorithm:?}");
            assert_eq!(serde_json::to_value(actual.output.search_history).unwrap(), serde_json::to_value(expected.output.search_history).unwrap());
            assert_eq!(actual.output.numerical["resumed_after_generation"], 1);
            let mut changed = manifest.clone();
            changed.integration.time_step *= 0.5;
            assert!(CheckpointStore::new(db, id, &changed).unwrap().load(8, 4).is_err());
            assert!(store.load(4, 4).is_err());
        }
    }

    #[test]
    fn unevaluated_and_nonfinite_scores_survive_json_round_trip() {
        let candidate = Candidate::unevaluated(vec![1.0]);
        let saved = serde_json::to_value(candidate).unwrap();
        assert!(saved["score"].is_null());
        let loaded: Candidate = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded.score, f64::INFINITY);
    }
    #[test]
    fn restart_limit_and_deadline_are_not_reset() {
        let mut state = RecoveryState { deadline: Some(Utc::now() + chrono::Duration::seconds(10)), ..Default::default() };
        let deadline = state.deadline;
        for _ in 0..MAX_RESTARTS { state.restart("memory").unwrap(); }
        assert!(state.restart("memory").is_err());
        assert_eq!(state.deadline, deadline);
        state.restarts = 0;
        state.deadline = Some(Utc::now() - chrono::Duration::seconds(1));
        assert!(state.restart("process interrupted").is_err());
    }
}
