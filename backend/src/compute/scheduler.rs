use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering as AtomicOrdering},
    },
};

use anyhow::{Context, bail};
use chrono::Utc;
use parking_lot::RwLock;
use serde_json::json;
use tokio::sync::{Semaphore, broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    domain::{ComputePreference, RunRecord, RunRequest, RunStatus, ServerEvent},
    persistence::Database,
    sandbox::validate_manifest,
    science::{self, ProgressCallback},
};

use super::HardwareManager;
use super::recovery::{CheckpointStore, RecoveryState, ResourcePressure};

#[cfg(test)]
use crate::domain::RunPriority;

#[derive(Clone)]
pub struct Scheduler {
    inner: Arc<SchedulerInner>,
    command_tx: mpsc::UnboundedSender<SchedulerCommand>,
}

struct SchedulerInner {
    database: Database,
    hardware: HardwareManager,
    runs: RwLock<HashMap<Uuid, RunRecord>>,
    cancellations: RwLock<HashMap<Uuid, CancellationToken>>,
    events: broadcast::Sender<ServerEvent>,
    sequence: AtomicU64,
    numerical_gate: Arc<Semaphore>,
}

enum SchedulerCommand {
    Submit(QueuedWork),
}

#[derive(Clone)]
struct QueuedWork {
    run_id: Uuid,
    priority: u8,
    sequence: u64,
}

impl Eq for QueuedWork {}
impl PartialEq for QueuedWork {
    fn eq(&self, other: &Self) -> bool { self.run_id == other.run_id }
}
impl Ord for QueuedWork {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}
impl PartialOrd for QueuedWork {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

impl Scheduler {
    pub fn new(database: Database, hardware: HardwareManager) -> anyhow::Result<Self> {
        let (events, _) = broadcast::channel(2048);
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let mut initial_runs = HashMap::new();
        let mut restored = Vec::new();
        let mut initial_cancellations = HashMap::new();
        // Agent tasks pause on application restart. Resolve their ownership before
        // spawning the dispatcher so no numerical worker can race that pause.
        let suspended_tasks = suspended_task_ownership(&database)?;

        for mut run in database.list_runs(1000)? {
            if matches!(run.status, RunStatus::Queued | RunStatus::Running) {
                if suspended_tasks.runs.contains(&(run.project_id, run.id))
                    || suspended_tasks.manifests.contains(&(run.project_id, run.manifest_id)) {
                    run.status = RunStatus::Cancelled;
                    run.phase = "Research task interrupted; resume the task to continue".to_owned();
                    run.error = None;
                    run.finished_at = Some(Utc::now());
                    database.put_run(&run)?;
                    initial_runs.insert(run.id, run);
                    continue;
                }
                let mut recovery = RecoveryState::load(&database, run.id)?;
                if recovery.deadline.is_none() {
                    recovery.deadline = run.started_at.map(|time| time + chrono::Duration::seconds(run.compute.max_wall_seconds as i64));
                }
                match recovery.restart("Backend interrupted; resuming the most recent completed generation, or restarting the immutable experiment when no checkpoint exists.") {
                    Ok(()) => {
                        run.status = RunStatus::Queued;
                        run.error = None;
                        run.finished_at = None;
                        run.phase = "Recovering saved experiment".to_owned();
                        if run.compute.batch_size > 1 { run.compute.batch_size /= 2; }
                        initial_cancellations.insert(run.id, CancellationToken::new());
                        restored.push((run.queued_at, run.id, run.priority.queue_weight()));
                    }
                    Err(error) => {
                        run.status = RunStatus::Failed;
                        run.error = Some(format!("Interrupted run could not resume: {error:#}"));
                        run.phase = "Recovery stopped".to_owned();
                        run.finished_at = Some(Utc::now());
                    }
                }
                recovery.save(&database, run.id)?;
                database.put_run(&run)?;
            }
            initial_runs.insert(run.id, run);
        }

        let inner = Arc::new(SchedulerInner {
            database,
            hardware,
            runs: RwLock::new(initial_runs),
            cancellations: RwLock::new(initial_cancellations),
            events,
            sequence: AtomicU64::new(0),
            // A single high-throughput numerical run may use the full Rayon pool or GPU.
            // Queueing additional runs keeps the browser and host responsive and prevents VRAM oversubscription.
            numerical_gate: Arc::new(Semaphore::new(1)),
        });

        spawn_dispatcher(
            Arc::clone(&inner),
            command_rx,
            inner.hardware.profile().scheduler.max_parallel_jobs,
        );
        restored.sort_by_key(|(queued_at, _, _)| *queued_at);
        for (_, run_id, priority) in restored {
            let sequence = inner.sequence.fetch_add(1, AtomicOrdering::Relaxed);
            command_tx.send(SchedulerCommand::Submit(QueuedWork { run_id, priority, sequence }))
                .context("unable to restore scheduler queue")?;
        }
        Ok(Self { inner, command_tx })
    }

    pub fn submit(&self, request: RunRequest) -> anyhow::Result<RunRecord> {
        let manifest = self
            .inner
            .database
            .get_manifest(request.manifest_id)?
            .context("manifest not found")?;
        validate_manifest(&manifest)?;
        validate_request(&request, &manifest.compute)?;

        if request
            .compute
            .as_ref()
            .map(|value| value.preference == ComputePreference::Gpu)
            .unwrap_or(manifest.compute.preference == ComputePreference::Gpu)
            && !self.inner.hardware.profile().gpu_available
        {
            bail!("GPU execution was requested, but no hardware GPU initialized. Use auto or CPU, or review the Hardware view.");
        }

        let mut record = RunRecord::queued(request, &manifest);
        let mut effective=manifest.clone();effective.compute=record.compute.clone();
        let advice=super::advisor::advise(&effective,&self.inner.hardware)?;
        if !advice.admissible {bail!("insufficient memory headroom for this capture/search plan; review Compute advice");}
        record.compute=advice.recommended_compute;
        let cancellation = CancellationToken::new();
        self.inner.database.put_run(&record)?;
        self.inner.runs.write().insert(record.id, record.clone());
        self.inner
            .cancellations
            .write()
            .insert(record.id, cancellation);

        let sequence = self.inner.sequence.fetch_add(1, AtomicOrdering::Relaxed);
        if self
            .command_tx
            .send(SchedulerCommand::Submit(QueuedWork {
                run_id: record.id,
                priority: record.priority.queue_weight(),
                sequence,
            }))
            .is_err()
        {
            self.inner.cancellations.write().remove(&record.id);
            let _ = self.inner.mutate_run(record.id, |run| {
                run.status = RunStatus::Failed;
                run.phase = "Dispatcher unavailable".to_owned();
                run.error = Some("The scheduler dispatcher is unavailable.".to_owned());
                run.finished_at = Some(Utc::now());
            });
            bail!("scheduler dispatcher is unavailable");
        }

        self.inner.emit(ServerEvent::new(
            "run_queued",
            Some(record.id),
            json!({
                "status": record.status,
                "phase": record.phase,
                "priority": record.priority,
                "manifest_id": record.manifest_id,
                "manifest_revision": record.manifest_revision,
                "capability_id": record.capability_id,
            }),
        ));
        Ok(record)
    }

    pub fn cancel(&self, id: Uuid) -> anyhow::Result<RunRecord> {
        let token = self
            .inner
            .cancellations
            .read()
            .get(&id)
            .cloned()
            .context("run not found or no longer cancellable")?;
        token.cancel();
        let updated = self.inner.mutate_run(id, |run| {
            if matches!(run.status, RunStatus::Queued | RunStatus::Running) {
                run.status = RunStatus::Cancelled;
                run.phase = "Cancellation requested".to_owned();
                run.finished_at = Some(Utc::now());
            }
        })?;
        self.inner.emit(ServerEvent::new(
            "run_cancelled",
            Some(id),
            json!({"status": updated.status, "phase": updated.phase}),
        ));
        Ok(updated)
    }

    pub fn get(&self, id: Uuid) -> Option<RunRecord> {
        self.inner.runs.read().get(&id).cloned()
    }

    pub fn list(&self, limit: usize) -> Vec<RunRecord> {
        let mut values: Vec<_> = self.inner.runs.read().values().cloned().collect();
        values.sort_by(|left, right| right.queued_at.cmp(&left.queued_at));
        values.truncate(limit.clamp(1, 1000));
        values
    }

    pub fn workflow_status(&self) -> serde_json::Value {
        let runs = self.inner.runs.read();
        let mut items = runs.values().collect::<Vec<_>>();
        items.sort_by(|a,b| b.queued_at.cmp(&a.queued_at));
        json!(items.into_iter().take(100).map(|r| json!({
            "id":r.id,"project_id":r.project_id,"manifest_id":r.manifest_id,"status":r.status,
            "phase":r.phase,"progress":r.progress,"started_at":r.started_at,"queued_at":r.queued_at,
            "finished_at":r.finished_at,"backend_used":r.backend_used,"error":r.error
        })).collect::<Vec<_>>())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.inner.events.subscribe()
    }

    pub fn hardware(&self) -> &HardwareManager { &self.inner.hardware }
}

#[derive(Default)]
struct SuspendedTaskOwnership {
    runs: HashSet<(Uuid, Uuid)>,
    manifests: HashSet<(Uuid, Uuid)>,
}

fn suspended_task_ownership(database: &Database) -> anyhow::Result<SuspendedTaskOwnership> {
    use crate::agent::tasks::TaskState;
    let mut ownership = SuspendedTaskOwnership::default();
    for task in database.list_agent_tasks()?.into_iter().filter(|task|
        matches!(task.state, TaskState::Running | TaskState::Paused)) {
        for run_id in task.run_ids.iter().copied().chain(task.current_run_id) {
            ownership.runs.insert((task.project_id, run_id));
        }
        if let Some(manifest_id) = task.current_manifest_id {
            ownership.manifests.insert((task.project_id, manifest_id));
        }
        // Older builders could commit a chat receipt before recording its run in
        // the task. Include that exact receipt, without claiming unrelated runs.
        if let Some(request_id) = task.current_request_id {
            if let Some(message) = database.list_messages(task.project_id, 5000)?.into_iter().rev().find(|message|
                message.metadata["request_id"] == json!(request_id)
                    && message.role == crate::domain::ConversationRole::Assistant
                    && message.kind != crate::domain::MessageKind::Error) {
                if let Some(manifest_id) = message.manifest_id {
                    ownership.manifests.insert((task.project_id, manifest_id));
                }
                if let Some(run_id) = message.metadata["submitted_run_id"].as_str().and_then(|value| Uuid::parse_str(value).ok()) {
                    ownership.runs.insert((task.project_id, run_id));
                }
            }
        }
    }
    Ok(ownership)
}

fn spawn_dispatcher(
    inner: Arc<SchedulerInner>,
    mut command_rx: mpsc::UnboundedReceiver<SchedulerCommand>,
    max_parallel_jobs: usize,
) {
    tokio::spawn(async move {
        let (done_tx, mut done_rx) = mpsc::unbounded_channel::<Uuid>();
        let mut pending = BinaryHeap::<QueuedWork>::new();
        let mut active = 0usize;
        let max_parallel_jobs = max_parallel_jobs.max(1);

        loop {
            tokio::select! {
                command = command_rx.recv() => {
                    match command {
                        Some(SchedulerCommand::Submit(work)) => pending.push(work),
                        None if active == 0 => break,
                        None => {}
                    }
                }
                completed = done_rx.recv(), if active > 0 => {
                    if completed.is_some() { active = active.saturating_sub(1); }
                }
            }

            while active < max_parallel_jobs {
                let Some(work) = pending.pop() else { break; };
                active += 1;
                let task_inner = Arc::clone(&inner);
                let task_done = done_tx.clone();
                tokio::spawn(async move {
                    execute_run(Arc::clone(&task_inner), work.run_id).await;
                    let _ = task_done.send(work.run_id);
                });
            }
        }
    });
}

async fn execute_run(inner: Arc<SchedulerInner>, run_id: Uuid) {
    let cancellation = match inner.cancellations.read().get(&run_id).cloned() {
        Some(value) => value,
        None => return,
    };
    if cancellation.is_cancelled() {
        inner.cancellations.write().remove(&run_id);
        return;
    }

    let run = match inner.mutate_run(run_id, |run| {
        run.status = RunStatus::Running;
        run.started_at.get_or_insert_with(Utc::now);
        run.progress = 0.01;
        run.phase = "Loading manifest".to_owned();
    }) {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, %run_id, "unable to start run");
            inner.cancellations.write().remove(&run_id);
            return;
        }
    };

    inner.emit(ServerEvent::new(
        "run_started",
        Some(run_id),
        json!({
            "status": "running",
            "manifest_id": run.manifest_id,
            "manifest_revision": run.manifest_revision,
            "capability_id": run.capability_id,
        }),
    ));

    let manifest = match inner.database.get_manifest(run.manifest_id) {
        Ok(Some(value)) => value,
        Ok(None) => {
            finish_failed(&inner, run_id, "The immutable manifest for this run no longer exists.".to_owned());
            inner.cancellations.write().remove(&run_id);
            return;
        }
        Err(error) => {
            finish_failed(&inner, run_id, format!("Unable to load the run manifest: {error:#}"));
            inner.cancellations.write().remove(&run_id);
            return;
        }
    };

    if manifest.revision != run.manifest_revision || manifest.project_id != run.project_id {
        finish_failed(
            &inner,
            run_id,
            "Run provenance does not match the stored manifest revision.".to_owned(),
        );
        inner.cancellations.write().remove(&run_id);
        return;
    }
    if let Err(error) = validate_manifest(&manifest) {
        finish_failed(&inner, run_id, format!("Manifest validation failed: {error:#}"));
        inner.cancellations.write().remove(&run_id);
        return;
    }

    let progress_inner = Arc::clone(&inner);
    let last_progress = parking_lot::Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(1));
    let progress: ProgressCallback = Arc::new(move |value, phase| {
        {
            let mut last = last_progress.lock();
            if value < 0.98 && last.elapsed() < std::time::Duration::from_millis(150) { return; }
            *last = std::time::Instant::now();
        }
        if let Ok(updated) = progress_inner.mutate_run(run_id, |run| {
            if run.status == RunStatus::Running {
                run.progress = value.clamp(0.0, 1.0);
                run.phase = phase.to_owned();
            }
        }) {
            progress_inner.emit(ServerEvent::new(
                "run_progress",
                Some(run_id),
                json!({
                    "progress": updated.progress,
                    "phase": updated.phase,
                    "status": updated.status,
                }),
            ));
        }
    });

    let _ = inner.mutate_run(run_id, |run| { run.phase = "Waiting for numerical compute slot".to_owned(); });
    let acquire = Arc::clone(&inner.numerical_gate).acquire_owned();
    let permit = tokio::select! {
        _ = cancellation.cancelled() => { inner.cancellations.write().remove(&run_id); return; }
        result = acquire => match result {
            Ok(value) => value,
            Err(_) => { finish_failed(&inner, run_id, "Numerical compute slot unavailable".to_owned()); return; }
        }
    };
    let _ = inner.mutate_run(run_id, |run| { run.phase = "Preparing numerical runtime".to_owned(); });
    let recovery = (|| -> anyhow::Result<RecoveryState> {
        let mut recovery = RecoveryState::load(&inner.database, run_id)?;
        recovery.deadline.get_or_insert_with(|| Utc::now() + chrono::Duration::seconds(run.compute.max_wall_seconds as i64));
        recovery.save(&inner.database, run_id)?;
        Ok(recovery)
    })();
    let mut recovery = match recovery {
        Ok(value) => value,
        Err(error) => {
            finish_failed(&inner, run_id, format!("Unable to persist recovery budget: {error:#}"));
            inner.cancellations.write().remove(&run_id); return;
        }
    };
    let remaining = (recovery.deadline.expect("deadline initialized") - Utc::now()).to_std().unwrap_or_default();
    let execution_future = async {
        let checkpoint = CheckpointStore::new(inner.database.clone(), run_id, &manifest)?;
        let mut allocation = run.compute.clone();
        loop {
            if cancellation.is_cancelled() { bail!("run cancelled"); }
            let mut effective = manifest.clone();
            effective.compute = allocation.clone();
            let advice = super::advisor::advise(&effective, &inner.hardware)?;
            let execution = if advice.admissible {
                allocation = advice.recommended_compute.clone();
                inner.mutate_run(run_id, |record| record.compute = allocation.clone())?;
                science::execute_checkpointed(manifest.clone(), allocation.clone(), inner.hardware.clone(), cancellation.clone(), progress.clone(), Some(checkpoint.clone())).await
            } else {
                Err(ResourcePressure("Available memory changed while queued or running; the current allocation no longer fits.".into()).into())
            };
            match execution {
                Ok(mut outcome) => {
                    outcome.value["numerical"]["compute_advice"] = json!(advice);
                    outcome.value["numerical"]["recovery"] = json!({
                        "restarts": recovery.restarts, "events": recovery.events, "deadline": recovery.deadline,
                        "scope": "Generation checkpoints preserve the population, random generator, scores and completed search history. Interrupted generations, final replay and falsification restart. The original wall-time deadline is never extended."
                    });
                    return Ok::<_, anyhow::Error>(outcome);
                }
                Err(error) if error.downcast_ref::<ResourcePressure>().is_some() && !cancellation.is_cancelled() => {
                    let message = format!("Resource recovery: {error:#}");
                    recovery.restart(&message)?;
                    allocation.preference = ComputePreference::Cpu;
                    allocation.batch_size = (allocation.batch_size / 2).max(1);
                    recovery.save(&inner.database, run_id)?;
                    inner.mutate_run(run_id, |record| {
                        record.compute = allocation.clone();
                        record.phase = format!("Recovering with {} CPU candidate(s) per batch", allocation.batch_size);
                    })?;
                    inner.emit(ServerEvent::new("run_recovering", Some(run_id), json!({"attempt": recovery.restarts, "batch_size": allocation.batch_size, "reason": message})));
                    tokio::select! {
                        _ = cancellation.cancelled() => bail!("run cancelled"),
                        _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}
                    }
                }
                Err(error) => return Err(error),
            }
        }
    };
    tokio::pin!(execution_future);
    let execution = tokio::select! {
        value = &mut execution_future => value,
        _ = tokio::time::sleep(remaining) => {
            cancellation.cancel();
            let _ = execution_future.await;
            drop(permit);
            finish_failed(&inner, run_id, "Run exceeded the configured wall-time budget. Workers were cooperatively cancelled.".to_owned());
            inner.cancellations.write().remove(&run_id);
            return;
        }
    };
    drop(permit);

    if cancellation.is_cancelled() {
        let _ = inner.mutate_run(run_id, |run| {
            run.status = RunStatus::Cancelled;
            run.phase = "Cancelled".to_owned();
            run.finished_at = Some(Utc::now());
        });
        inner.emit(ServerEvent::new(
            "run_cancelled",
            Some(run_id),
            json!({"status": "cancelled"}),
        ));
    } else {
        match execution {
            Ok(outcome) => {
                if let Ok(updated) = inner.mutate_run(run_id, |run| {
                    run.status = RunStatus::Completed;
                    run.progress = 1.0;
                    run.phase = "Complete".to_owned();
                    run.backend_used = Some(outcome.backend_used.clone());
                    run.result = Some(outcome.value.clone());
                    run.finished_at = Some(Utc::now());
                }) {
                    if let Err(error) = inner.database.clear_run_checkpoint(run_id) {
                        tracing::warn!(%error, %run_id, "completed run checkpoint cleanup failed");
                    }
                    inner.emit(ServerEvent::new(
                        "run_completed",
                        Some(run_id),
                        json!({
                            "status": updated.status,
                            "phase": updated.phase,
                            "backend_used": updated.backend_used,
                            "manifest_id": updated.manifest_id,
                        }),
                    ));
                }
            }
            Err(error) => finish_failed(&inner, run_id, format!("{error:#}")),
        }
    }

    inner.cancellations.write().remove(&run_id);
}

fn finish_failed(inner: &Arc<SchedulerInner>, run_id: Uuid, message: String) {
    let _ = inner.mutate_run(run_id, |run| {
        run.status = RunStatus::Failed;
        run.phase = "Failed".to_owned();
        run.error = Some(message.clone());
        run.finished_at = Some(Utc::now());
    });
    inner.emit(ServerEvent::new(
        "run_failed",
        Some(run_id),
        json!({"status": "failed", "error": message}),
    ));
}

impl SchedulerInner {
    fn mutate_run<F>(&self, id: Uuid, mutation: F) -> anyhow::Result<RunRecord>
    where
        F: FnOnce(&mut RunRecord),
    {
        let mut runs = self.runs.write();
        let run = runs.get_mut(&id).context("run not found")?;
        mutation(run);
        self.database.put_run(run)?;
        Ok(run.clone())
    }

    fn emit(&self, event: ServerEvent) { let _ = self.events.send(event); }
}

fn validate_request(
    request: &RunRequest,
    manifest_compute: &crate::domain::ComputeRequest,
) -> anyhow::Result<()> {
    let compute = request.compute.as_ref().unwrap_or(manifest_compute);
    if compute.candidate_count == 0 {
        bail!("candidate_count must be at least one");
    }
    if compute.candidate_count > 1_000_000 {
        bail!("candidate_count exceeds the one-million candidate safety limit");
    }
    if compute.batch_size > 1_000_000 {
        bail!("batch_size exceeds the one-million item safety limit");
    }
    if compute.max_wall_seconds == 0 || compute.max_wall_seconds > 86_400 {
        bail!("max_wall_seconds must be between 1 and 86400");
    }
    if compute.max_memory_mb < 128 || compute.max_memory_mb > 262_144 {
        bail!("max_memory_mb must be between 128 and 262144");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_queue_prefers_interactive_then_fifo() {
        let mut heap = BinaryHeap::new();
        let background = QueuedWork {
            run_id: Uuid::new_v4(),
            priority: RunPriority::Background.queue_weight(),
            sequence: 0,
        };
        let first_interactive = QueuedWork {
            run_id: Uuid::new_v4(),
            priority: RunPriority::Interactive.queue_weight(),
            sequence: 1,
        };
        let second_interactive = QueuedWork {
            run_id: Uuid::new_v4(),
            priority: RunPriority::Interactive.queue_weight(),
            sequence: 2,
        };
        heap.push(background.clone());
        heap.push(second_interactive.clone());
        heap.push(first_interactive.clone());
        assert_eq!(heap.pop().expect("first").run_id, first_interactive.run_id);
        assert_eq!(heap.pop().expect("second").run_id, second_interactive.run_id);
        assert_eq!(heap.pop().expect("third").run_id, background.run_id);
    }

    #[tokio::test]
    async fn startup_recovers_interrupted_run_but_never_cancelled_or_expired_work() {
        let temp = tempfile::tempdir().unwrap();
        let database = Database::open(&temp.path().join("scheduler.sqlite3")).unwrap();
        let draft = serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();
        let manifest = crate::domain::ExperimentManifest::from_draft(Uuid::nil(), None, 1, draft, "scheduler recovery test");
        database.put_manifest(&manifest).unwrap();
        let request = RunRequest { manifest_id: manifest.id, name: None, priority: RunPriority::Interactive, compute: None };
        let mut interrupted = RunRecord::queued(request.clone(), &manifest);
        interrupted.status = RunStatus::Running;
        interrupted.started_at = Some(Utc::now());
        database.put_run(&interrupted).unwrap();
        let mut cancelled = RunRecord::queued(request.clone(), &manifest);
        cancelled.status = RunStatus::Cancelled;
        database.put_run(&cancelled).unwrap();
        let mut expired = RunRecord::queued(request, &manifest);
        expired.status = RunStatus::Running;
        expired.started_at = Some(Utc::now() - chrono::Duration::seconds(60));
        database.put_run(&expired).unwrap();
        let config = crate::config::AppConfig { gpu_enabled: false, data_directory: temp.path().to_owned(), ..Default::default() };
        let hardware = HardwareManager::discover(&config).await.unwrap();
        let scheduler = Scheduler::new(database.clone(), hardware).unwrap();
        assert_eq!(scheduler.get(cancelled.id).unwrap().status, RunStatus::Cancelled);
        assert_eq!(scheduler.get(expired.id).unwrap().status, RunStatus::Failed);
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            loop {
                let current = scheduler.get(interrupted.id).unwrap();
                if matches!(current.status, RunStatus::Completed | RunStatus::Failed) {
                    assert_eq!(current.status, RunStatus::Completed, "{:?}", current.error);
                    assert_eq!(current.result.unwrap()["numerical"]["recovery"]["restarts"], 1);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }).await.unwrap();
        assert!(database.get_run_checkpoint(interrupted.id).unwrap().is_none());
    }

    #[tokio::test]
    async fn startup_never_dispatches_solvers_owned_by_interrupted_tasks() {
        use crate::agent::tasks::{ResearchTask, TaskStage, TaskState};
        let temp = tempfile::tempdir().unwrap();
        let database = Database::open(&temp.path().join("task-startup.sqlite3")).unwrap();
        let project_id = Uuid::new_v4();
        let mut protected = Vec::new();
        for state in [TaskState::Running, TaskState::Paused] {
            for source in ["current_run", "history_run", "manifest", "receipt"] {
                let draft = serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();
                let manifest = crate::domain::ExperimentManifest::from_draft(project_id, None, 1, draft, "task startup test");
                database.put_manifest(&manifest).unwrap();
                let run = RunRecord::queued(RunRequest { manifest_id: manifest.id, name: None,
                    priority: RunPriority::Interactive, compute: None }, &manifest);
                database.put_run(&run).unwrap();
                database.put_run_checkpoint(run.id, &json!({"retained_without_execution":true})).unwrap();
                let request_id = Uuid::new_v4();
                let task = ResearchTask {
                    id: Uuid::new_v4(), project_id, objective: "Protect the shared research budget after restart".into(),
                    state, stage: TaskStage::Simulating, cycle: 1, max_cycles: 2, duration_minutes: 10,
                    specialist_count: 1, auto_run: true, provider: crate::domain::ProviderKind::OpenAi,
                    model: "test-model".into(), reasoning_effort: None, experiment_options: Default::default(),
                    created_at: Utc::now(), updated_at: Utc::now(), deadline_at: Utc::now() + chrono::Duration::minutes(10),
                    remaining_seconds: 600, children: vec![], artifacts: vec![],
                    run_ids: if source == "history_run" { vec![run.id] } else { vec![] },
                    current_run_id: (source == "current_run").then_some(run.id),
                    current_manifest_id: (source == "manifest").then_some(manifest.id),
                    current_request_id: (source == "receipt").then_some(request_id),
                    next_step: None, failure: None, notice: String::new(),
                };
                database.put_agent_task(&task).unwrap();
                if source == "receipt" {
                    let mut receipt = crate::domain::ConversationMessage::new(project_id,
                        crate::domain::ConversationRole::Assistant, crate::domain::MessageKind::Proposal, "Saved builder receipt");
                    receipt.manifest_id = Some(manifest.id);
                    receipt.metadata = json!({"request_id":request_id,"submitted_run_id":run.id});
                    database.put_message(&receipt).unwrap();
                }
                protected.push(run.id);
            }
        }
        // An unrelated manifest in the same project must retain ordinary recovery.
        let draft = serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();
        let standalone_manifest = crate::domain::ExperimentManifest::from_draft(project_id, None, 1, draft, "standalone control");
        database.put_manifest(&standalone_manifest).unwrap();
        let standalone = RunRecord::queued(RunRequest { manifest_id: standalone_manifest.id, name: None,
            priority: RunPriority::Interactive, compute: None }, &standalone_manifest);
        database.put_run(&standalone).unwrap();
        let hardware = HardwareManager::discover(&crate::config::AppConfig { gpu_enabled: false,
            data_directory: temp.path().to_owned(), ..Default::default() }).await.unwrap();
        // No AgentService initialization: cancellation must already be durable.
        let scheduler = Scheduler::new(database.clone(), hardware).unwrap();
        for id in &protected {
            let run = scheduler.get(*id).unwrap();
            assert_eq!(run.status, RunStatus::Cancelled);
            assert!(run.started_at.is_none());
            assert!(scheduler.cancel(*id).is_err(), "protected runs must not have queued cancellation tokens");
            assert_eq!(database.get_run_checkpoint(*id).unwrap().unwrap()["retained_without_execution"], true);
            assert!(database.get_run_recovery(*id).unwrap().is_none());
        }
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            loop {
                let run = scheduler.get(standalone.id).unwrap();
                if matches!(run.status, RunStatus::Completed | RunStatus::Failed) {
                    assert_eq!(run.status, RunStatus::Completed, "{:?}", run.error);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }).await.unwrap();
        assert!(protected.iter().all(|id| scheduler.get(*id).unwrap().status == RunStatus::Cancelled));
    }
}
