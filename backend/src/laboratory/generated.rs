//! Durable generated-code jobs. Only the validated LPAC Python/NumPy boundary executes code.
use super::{
    isolation::{IsolatedProcess, IsolationSpec},
    safe_relative, write_json, LabJob, LaboratoryService,
};
use anyhow::{bail, ensure, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MIB: u64 = 1024 * 1024;
const RESERVE_BYTES: u64 = 64 * MIB;
const FREE_FLOOR: u64 = 256 * MIB;
const MAX_FILES: usize = 10_000;
const MONITOR_MS: u64 = 250;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GeneratedLimits {
    pub memory_mb: usize,
    pub process_limit: u32,
    pub wall_seconds: Option<u64>,
    pub storage_mb: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceImport {
    pub job_id: Uuid,
    pub path: String,
    /// Relative to the new attempt's imports directory.
    pub destination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GeneratedInput {
    pub engine: String,
    pub code: String,
    pub inputs: Value,
    #[serde(default)]
    pub sources: Vec<SourceImport>,
    pub limits: GeneratedLimits,
    #[serde(default)]
    pub restarted_from_job_id: Option<Uuid>,
}

/// Informational descriptor; callers must deliberately expose any user-facing tool.
pub fn generated_capability() -> Value {
    json!({"id":"python_numpy","runtime":{"python":"3.13.15","numpy":"2.4.6"},
        "packages":["Python standard library","NumPy"],"boundary":"windows_lpac_no_network",
        "dependencies":"Only shipped, hash-pinned dependencies. Dependency extensions require a future approved allowlist manager.",
        "execution":"experiment.py reads input.json and imports/; writes result.json and relative output files in its working directory",
        "result_contract":"result.json must be a JSON object; optional artifacts is a list of existing relative file paths",
        "storage":"Monitored working-directory soft limit, 250 ms sampling; overshoot is possible. No hard disk quota.",
        "scientific_validity":"Successful execution does not establish physical or biological validity."})
}

pub fn validate_generated_input(value: &Value) -> anyhow::Result<GeneratedInput> {
    ensure!(
        value["limits"].get("wall_seconds").is_some(),
        "limits.wall_seconds must explicitly specify seconds or null (Off)"
    );
    let input: GeneratedInput =
        serde_json::from_value(value.clone()).context("Invalid generated experiment input")?;
    ensure!(
        input.engine == "python_numpy",
        "Only the pinned Python/NumPy generated-code runtime is supported"
    );
    ensure!(
        !input.code.trim().is_empty() && input.code.len() <= 256 * 1024,
        "Experiment source must contain 1â€“262144 bytes"
    );
    ensure!(
        input.inputs.is_object() && serde_json::to_vec(&input.inputs)?.len() <= MIB as usize,
        "inputs must be a JSON object of at most 1 MiB"
    );
    ensure!(
        (128..=8192).contains(&input.limits.memory_mb),
        "memory_mb must be between 128 and 8192"
    );
    ensure!(
        (1..=16).contains(&input.limits.process_limit),
        "process_limit must be between 1 and 16"
    );
    ensure!(
        input
            .limits
            .wall_seconds
            .is_none_or(|seconds| (1..=86400).contains(&seconds)),
        "wall_seconds must be null or 1â€“86400"
    );
    ensure!(
        (64..=4096).contains(&input.limits.storage_mb),
        "storage_mb must be between 64 and 4096"
    );
    ensure!(
        input.sources.len() <= 32,
        "At most 32 source artifacts can be imported"
    );
    let mut destinations = std::collections::BTreeSet::new();
    for source in &input.sources {
        strict_relative(&source.path)?;
        strict_relative(&source.destination)?;
        ensure!(
            source.sha256.as_ref().is_none_or(
                |hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            ),
            "Import sha256 must be a 64-character hexadecimal digest"
        );
        ensure!(
            destinations.insert(source.destination.replace('\\', "/").to_ascii_lowercase()),
            "Duplicate import destination"
        );
    }
    Ok(input)
}

impl LaboratoryService {
    /// Bounded byte export for the API. Generated directories are inspected only
    /// after execution ends; provider/API admission still owns project access.
    pub fn read_generated_artifact(&self, id: Uuid, relative: &str) -> anyhow::Result<Vec<u8>> {
        strict_relative(relative)?;
        let job = self.get(id)?;
        ensure!(job.kind == "generated", "Not a generated-code job");
        let root = self.directory(id);
        if relative.starts_with("work/") {
            ensure!(
                !job.active() && !self.executing(id),
                "Generated output is readable after its process tree has stopped"
            );
            let receipt: Value =
                serde_json::from_slice(&bounded_file(&root, "generated-artifacts.json", 4 * MIB)?)?;
            let rows = receipt["artifacts"]
                .as_array()
                .context("Generated artifact inventory is unavailable")?;
            let row = rows
                .iter()
                .find(|row| row["path"] == relative)
                .context("Generated file is not registered in this attempt")?;
            let bytes = bounded_file(&root, relative, 64 * MIB)?;
            ensure!(
                row["bytes"].as_u64() == Some(bytes.len() as u64),
                "Generated file size changed after registration"
            );
            if let Some(expected) = row["sha256"].as_str() {
                ensure!(
                    sha(&bytes) == expected,
                    "Generated artifact hash changed after registration"
                );
            }
            Ok(bytes)
        } else {
            ensure!(
                matches!(
                    relative,
                    "input.json"
                        | "source.py"
                        | "source-input.json"
                        | "prepared.json"
                        | "generated-artifacts.json"
                        | "execution.json"
                        | "manifest.json"
                        | "result.json"
                        | "storage-stop.json"
                        | "provision.log"
                        | "provision-error.log"
                ) || relative.starts_with("source-inputs/"),
                "Artifact is not an exported generated-code receipt or source snapshot"
            );
            bounded_file(&root, relative, 64 * MIB)
        }
    }

    /// Starts one previously created immutable attempt. Never launches a host shell.
    pub fn start_generated(&self, id: Uuid) -> anyhow::Result<()> {
        let job = self.get(id)?;
        ensure!(
            job.kind == "generated" && job.state == "queued",
            "Only queued generated-code attempts can start"
        );
        self.ensure_generated_attempt_identity(id)?;
        if let Err(error) = validate_generated_input(&job.input) {
            self.update(id, |job| {
                job.state = "failed".into();
                job.error = Some(error.to_string());
                job.event("invalid_input", error.to_string(), json!({}));
            })?;
            return Err(error);
        }
        let token = if let Some(parent) = job.parent_id {
            let parent_job = self.get(parent)?;
            ensure!(
                parent_job.project_id == job.project_id && parent_job.active(),
                "Generated jobs require an active parent in the same project"
            );
            let parent_token = self
                .execution_token(parent)
                .context("Parent execution is unavailable")?;
            self.acquire_child(id, &parent_token)?
        } else {
            self.acquire(id)?
        };
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(error) = service.run_generated(id, &token).await {
                let _ = service.update(id, |job| {
                    if job.active() {
                        job.state = if token.is_cancelled() {
                            "cancelled"
                        } else {
                            "failed"
                        }
                        .into();
                    }
                    job.error = Some(format!("{error:#}"));
                    job.event(
                        "generated_stopped",
                        format!("{error:#}"),
                        json!({"outputs_retained":true}),
                    );
                });
            }
            service.release(id);
        });
        Ok(())
    }

    /// Generated code has no implicit checkpoint protocol. Resume is a fresh immutable attempt.
    pub fn restart_generated(
        &self,
        id: Uuid,
        deadline: Option<DateTime<Utc>>,
    ) -> anyhow::Result<LabJob> {
        let new = {
            // Serialize durable replacement creation. A live original worker cannot
            // be resumed, and repeated clicks share one retained replacement ID.
            let _gate = self.gate.lock();
            let original = self.get(id)?;
            for existing in self.database.lab_records()?.into_iter().filter(|job| {
                job.kind == "generated" && job.project_id == original.project_id
                    && job.input["restarted_from_job_id"] == json!(id) && job.state == "queued"
            }) {
                self.ensure_generated_attempt_identity(existing.id)?;
            }
            let _reservation = self
                .acquire(id)
                .context("The old process is still stopping; retry after it exits")?;
            let prepared = (|| -> anyhow::Result<LabJob> {
                let mut old = self.get(id)?;
                ensure!(
                    old.kind == "generated"
                        && matches!(
                            old.state.as_str(),
                            "paused" | "failed" | "timed_out" | "cancelled"
                        ),
                    "Generated attempt is not restartable"
                );
                // Search durable lineage as well as the event receipt: creation may
                // have committed immediately before an application interruption.
                if let Some(existing) = self.database.lab_records()?.into_iter().find(|job| {
                    job.kind == "generated"
                        && job.project_id == old.project_id
                        && job.input["restarted_from_job_id"] == json!(id)
                }) {
                    if !old
                        .events
                        .iter()
                        .any(|event| event.kind == "generated_restart_requested")
                    {
                        old.event("generated_restart_requested", "A prior replacement attempt was reconciled without duplicate execution.", json!({"new_job_id":existing.id,"strategy":"new_immutable_attempt"}));
                        self.database.put_lab_record(&old)?;
                    }
                    return Ok(existing);
                }
                let mut input = old.input.clone();
                input["restarted_from_job_id"] = json!(id);
                // Explicit continuation after a parent finishes is independent;
                // original ancestry remains in the immutable restart lineage.
                let parent = old.parent_id.filter(|parent| {
                    self.get(*parent).is_ok_and(|job| job.active()) && self.executing(*parent)
                });
                let mut new = self.create_locked(
                    Uuid::new_v4(),
                    old.project_id,
                    parent,
                    "generated",
                    &old.title,
                    input,
                    deadline,
                )?;
                new.event("restart", "A new attempt will execute retained source and input snapshots from the beginning.", json!({"previous_attempt":id,"strategy":"new_immutable_attempt"}));
                self.database.put_lab_record(&new)?;
                old.event(
                    "generated_restart_requested",
                    "A separate immutable attempt will rerun the preserved source and inputs.",
                    json!({"new_job_id":new.id,"strategy":"new_immutable_attempt"}),
                );
                self.database.put_lab_record(&old)?;
                Ok(new)
            })();
            // prepare_generated rejects an executing source attempt; release this
            // reservation before the replacement task can begin preparation.
            self.release(id);
            prepared?
        };
        if new.state == "queued" && !self.executing(new.id) {
            if let Err(error) = self.start_generated(new.id) {
                if !self.executing(new.id) && self.get(new.id)?.state == "queued" {
                    self.update(new.id, |job| {
                        job.state = "failed".into();
                        job.error = Some(error.to_string());
                        job.event("start_failed", error.to_string(), json!({}));
                    })?;
                    return Err(
                        error.context(format!("Replacement attempt {} could not start", new.id))
                    );
                }
            }
        }
        self.get(new.id)
    }

    async fn run_generated(&self, id: Uuid, token: &CancellationToken) -> anyhow::Result<()> {
        self.ensure_generated_attempt_identity(id)?;
        let job = self.get(id)?;
        let input = validate_generated_input(&job.input)?;
        let tool_deadline = input
            .limits
            .wall_seconds
            .map(|seconds| Utc::now() + chrono::Duration::seconds(seconds as i64));
        let parent_deadline = job
            .parent_id
            .map(|parent| self.get(parent).map(|job| job.deadline_at))
            .transpose()?
            .flatten();
        let deadline = [job.deadline_at, parent_deadline, tool_deadline]
            .into_iter()
            .flatten()
            .min();
        self.update(id, |job| job.deadline_at = deadline)?;
        let deadline_task = deadline.map(|at| {
            let token = token.clone();
            tokio::spawn(async move {
                tokio::time::sleep((at - Utc::now()).to_std().unwrap_or_default()).await;
                token.cancel();
            })
        });
        let result = self.execute_generated(id, &input, token).await;
        if let Some(task) = deadline_task {
            task.abort();
        }
        if token.is_cancelled() && deadline.is_some_and(|at| at <= Utc::now()) {
            self.update(id, |job| {
                if job.active() || job.state == "cancelled" {
                    job.state = "timed_out".into();
                    job.event(
                        "deadline",
                        "The explicit experiment or parent time budget expired.",
                        json!({}),
                    );
                }
            })?;
        }
        result
    }

    pub fn generated_runtime_directory(&self) -> PathBuf {
        super::runtime::RuntimeKind::Generated.directory(&self.config.data_directory)
    }

    fn ensure_generated_attempt_identity(&self, id: Uuid) -> anyhow::Result<()> {
        let job = self.get(id)?;
        ensure!(job.kind == "generated", "Not a generated-code attempt");
        let current = super::runtime::RuntimeKind::Generated;
        let mut recorded = false;
        for event in job.events.iter().filter(|event| event.kind == "runtime_verified") {
            recorded = true;
            ensure!(event.data["kind"].as_str() == Some(current.name())
                && event.data["manifest_sha256"].as_str() == Some(current.manifest_sha256().as_str()),
                "Original generated runtime differs; new immutable attempt required. Existing source, outputs and runtime receipts are preserved.");
        }
        let root = self.directory(id);
        if ["work", "prepared.json", "source.py", "source-input.json", "execution.json", "manifest.json", "generated-artifacts.json"]
            .iter().any(|name| root.join(name).exists())
        {
            ensure!(recorded,
                "Original generated runtime identity is unavailable; new immutable attempt required. Existing source and outputs are preserved.");
        }
        Ok(())
    }

    async fn ensure_generated_runtime(
        &self,
        id: Uuid,
        token: &CancellationToken,
    ) -> anyhow::Result<PathBuf> {
        self.ensure_generated_attempt_identity(id)?;
        ensure!(
            cfg!(windows),
            "Generated-code execution requires the validated Windows boundary"
        );
        let _guard = tokio::select! { _ = token.cancelled() => bail!("Cancelled before runtime provisioning"), guard = self.provision.lock() => guard };
        self.event(id, "runtime_provision", "Verifying and copying the bundled Python and NumPy runtime. No host Python or network installation is used.", generated_capability())?;
        let data = self.config.data_directory.clone();
        let copy_token = token.clone();
        let verified = tokio::task::spawn_blocking(move || super::runtime::provision(&data, super::runtime::RuntimeKind::Generated, &copy_token)).await??;
        self.event(id, "runtime_verified", "The compiled runtime inventory and upstream source pins match. Generated code will run inside the Windows LPAC boundary.", serde_json::to_value(&verified)?)?;
        Ok(verified.directory)
    }

    fn prepare_generated(&self, job: &LabJob, input: &GeneratedInput) -> anyhow::Result<Value> {
        let root = self.directory(job.id);
        let work = root.join("work");
        ensure!(
            !work.exists(),
            "Attempt work directory already exists; use a new immutable attempt"
        );
        fs::create_dir(&work)?;
        let inputs = serde_json::to_vec_pretty(&input.inputs)?;
        create_file(&root.join("source.py"), input.code.as_bytes())?;
        create_file(&root.join("source-input.json"), &inputs)?;
        create_file(&work.join("experiment.py"), input.code.as_bytes())?;
        create_file(&work.join("input.json"), &inputs)?;
        let previous = if let Some(old_id) = input.restarted_from_job_id {
            let old = self.get(old_id)?;
            ensure!(
                old.project_id == job.project_id
                    && old.kind == "generated"
                    && !old.active()
                    && !self.executing(old_id),
                "Invalid previous attempt"
            );
            let old_input = validate_generated_input(&old.input)?;
            ensure!(
                old_input.code == input.code
                    && old_input.inputs == input.inputs
                    && old_input.sources == input.sources,
                "Restart must retain identical source and inputs"
            );
            if self.directory(old_id).join("prepared.json").is_file() {
                Some((old_id, self.read_json(old_id, "prepared.json")?))
            } else {
                None
            }
        } else {
            None
        };
        let mut imported = Vec::new();
        let mut total = 0_u64;
        for source in &input.sources {
            let origin = self.get(source.job_id)?;
            self.ensure_study_import_access(job, source.job_id)?;
            ensure!(
                origin.project_id == job.project_id
                    && !origin.active()
                    && !self.executing(origin.id),
                "Imported artifacts must belong to an inactive job in this project"
            );
            let (source_root, relative, expected_hash) = if let Some((old_id, receipt)) = &previous
            {
                let rows = receipt["imports"]
                    .as_array()
                    .context("Previous import receipts are missing")?;
                let row = rows
                    .iter()
                    .find(|row| row["destination"] == source.destination)
                    .context("Previous import snapshot is missing")?;
                (
                    self.directory(*old_id),
                    format!("source-inputs/{}", source.destination),
                    Some(
                        row["sha256"]
                            .as_str()
                            .context("Previous import hash is missing")?
                            .to_owned(),
                    ),
                )
            } else {
                (self.directory(origin.id), source.path.clone(), None)
            };
            let bytes = bounded_file(&source_root, &relative, 64 * MIB)?;
            total += bytes.len() as u64;
            ensure!(
                total <= 128 * MIB,
                "Combined imported artifacts exceed 128 MiB"
            );
            let hash = sha(&bytes);
            ensure!(
                source
                    .sha256
                    .as_ref()
                    .is_none_or(|expected| expected.eq_ignore_ascii_case(&hash)),
                "Imported source bytes do not match the requested SHA256"
            );
            ensure!(
                expected_hash.is_none_or(|expected| expected == hash),
                "Retained source artifact hash changed"
            );
            create_file(
                &root.join("source-inputs").join(&source.destination),
                &bytes,
            )?;
            create_file(&work.join("imports").join(&source.destination), &bytes)?;
            imported.push(json!({"job_id":source.job_id,"source_path":source.path,"destination":source.destination,"bytes":bytes.len(),"sha256":hash}));
        }
        let prepared = json!({"schema_version":1,"job_id":job.id,"source_sha256":sha(input.code.as_bytes()),"inputs_sha256":sha(&inputs),"imports":imported,"restart_strategy":"new_immutable_attempt"});
        write_json(&root.join("prepared.json"), &prepared)?;
        Ok(prepared)
    }

    async fn execute_generated(
        &self,
        id: Uuid,
        input: &GeneratedInput,
        token: &CancellationToken,
    ) -> anyhow::Result<()> {
        self.ensure_generated_attempt_identity(id)?;
        let _slot = tokio::select! { _ = token.cancelled() => bail!("Cancelled in experiment queue"), slot = self.solver_slots.acquire() => slot? };
        self.update(id, |job| {
            if job.active() {
                job.state = "provisioning".into();
                job.event(
                    "environment",
                    "Checking the pinned isolated Python/NumPy runtime.",
                    json!({}),
                );
            }
        })?;
        let runtime = self.ensure_generated_runtime(id, token).await?;
        ensure!(
            !token.is_cancelled(),
            "Cancelled before preparing experiment inputs"
        );
        let job = self.get(id)?;
        let prepared = self.prepare_generated(&job, input)?;
        let root = self.directory(id);
        let work = root.join("work");
        let limit = input.limits.storage_mb * MIB;
        ensure!(
            scan_work(&work, false)?.bytes <= limit,
            "Inputs already exceed the working-directory storage budget"
        );
        ensure!(
            free_bytes(&root)? >= limit + RESERVE_BYTES + FREE_FLOOR,
            "Insufficient free space for the declared output budget and safety reserve"
        );
        let _reserve = StorageReserve::new(root.join("storage-reserve.bin"))?;
        ensure!(!token.is_cancelled(), "Cancelled before isolated launch");
        let spec = IsolationSpec {
            id,
            runtime_directory: runtime,
            working_directory: work.clone(),
            script: work.join("experiment.py"),
            arguments: Vec::new(),
            memory_mb: input.limits.memory_mb,
            process_limit: input.limits.process_limit,
            time_limit: input.limits.wall_seconds.map(Duration::from_secs),
        };
        let mut child = tokio::task::spawn_blocking(move || IsolatedProcess::spawn(spec)).await??;
        self.update(id, |job| { if job.active() { job.state = "running".into(); job.event("isolated_started", "Executing generated Python inside the verified Windows LPAC boundary.", json!({"limits":input.limits,"boundary":"windows_lpac_no_network","storage_monitor_ms":MONITOR_MS,"storage_is_hard_quota":false})); } })?;
        let mut peak = 0_u64;
        let outcome = {
            let future = child.wait(token);
            tokio::pin!(future);
            loop {
                tokio::select! {
                    result = &mut future => break result,
                    _ = tokio::time::sleep(Duration::from_millis(MONITOR_MS)) => {
                        let inventory = scan_work(&work, false)?;
                        peak = peak.max(inventory.bytes);
                        let available = free_bytes(&root)?;
                        self.update(id, |job| job.progress = json!({"phase":"executing","storage_bytes":inventory.bytes,"storage_limit_bytes":limit,"peak_storage_bytes":peak,"file_count":inventory.rows.len()}))?;
                        if inventory.bytes > limit || available < FREE_FLOOR {
                            write_json(&root.join("storage-stop.json"), &json!({"observed_bytes":inventory.bytes,"peak_bytes":peak,"limit_bytes":limit,"overshoot_bytes":peak.saturating_sub(limit),"free_bytes":available,"monitor_interval_ms":MONITOR_MS,"hard_quota":false}))?;
                            break Err(anyhow::anyhow!("Stopped on monitored storage limit or low free space; observed {} bytes, limit {} bytes (soft limit, possible overshoot)", inventory.bytes, limit));
                        }
                    }
                }
            }
        };
        child.stop_and_reap()?; // Verify every descendant is gone before reading generated content.
        drop(child);
        drop(_reserve); // Release private reserve before retaining the completion/error receipt.
        let observed_inventory = scan_work(&work, false)?;
        // An over-budget attempt cannot force an unbounded post-stop hashing pass.
        let final_inventory = if observed_inventory.bytes <= limit {
            scan_work(&work, true)?
        } else {
            observed_inventory
        };
        peak = peak.max(final_inventory.bytes);
        let storage = json!({"peak_observed_bytes":peak,"final_bytes":final_inventory.bytes,"limit_bytes":limit,"observed_overshoot_bytes":peak.saturating_sub(limit),"monitor_interval_ms":MONITOR_MS,"hard_quota":false,"scope":"experiment working directory; excludes OS AppContainer profile storage"});
        write_json(
            &root.join("generated-artifacts.json"),
            &json!({"schema_version":1,"artifacts":final_inventory.rows,"storage":storage}),
        )?;
        let outcome = outcome?;
        write_json(
            &root.join("execution.json"),
            &json!({"outcome":outcome,"storage":storage}),
        )?;
        ensure!(
            !token.is_cancelled(),
            "Generated attempt was stopped; partial files retained"
        );
        ensure!(
            outcome.exit_code == 0,
            "Generated Python exited with code {}; inspect work/stderr.log",
            outcome.exit_code
        );
        ensure!(
            final_inventory.bytes <= limit,
            "Generated output exceeded its monitored storage budget before the next sample"
        );
        let reported: Value = serde_json::from_slice(&bounded_file(&work, "result.json", 4 * MIB)?)
            .context("result.json must contain finite JSON data")?;
        ensure!(
            reported.is_object(),
            "Generated result.json must be a JSON object"
        );
        if let Some(artifacts) = reported.get("artifacts") {
            let artifacts = artifacts
                .as_array()
                .context("result.artifacts must be an array of relative file paths")?;
            ensure!(
                artifacts.len() <= 256,
                "At most 256 named result artifacts are allowed"
            );
            for artifact in artifacts {
                resolve_plain_file(
                    &work,
                    artifact
                        .as_str()
                        .context("Artifact references must be relative path strings")?,
                )?;
            }
        }
        let manifest = json!({"schema_version":1,"job_id":id,"engine":"python_numpy","capability":generated_capability(),"prepared":prepared,"limits":input.limits,"outcome":outcome,"storage":storage,"artifacts":final_inventory.rows});
        write_json(&root.join("manifest.json"), &manifest)?;
        let result = json!({"status":"completed","engine":"python_numpy","reported_result":reported,"scientific_validation":"not_established_by_execution","artifacts":final_inventory.rows,"manifest_path":"manifest.json","storage":storage});
        write_json(&root.join("result.json"), &result)?;
        self.update(id, |job| { if job.active() && !token.is_cancelled() { job.state = "completed".into(); job.result = result; job.progress = json!({"fraction":1.0}); job.event("completed", "Isolated computation finished; source snapshots, numerical outputs and execution receipts are retained.", json!({"manifest_path":"manifest.json","scientific_validity":"requires independent validation"})); } })?;
        Ok(())
    }
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn create_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn strict_relative(value: &str) -> anyhow::Result<()> {
    safe_relative(value)?;
    ensure!(
        value.len() <= 240 && !value.ends_with(['.', ' ']),
        "Invalid bounded artifact path"
    );
    for part in value.split(['/', '\\']) {
        ensure!(
            !part.ends_with(['.', ' ']) && !part.is_empty(),
            "Invalid artifact path component"
        );
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(
            !matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) && !(stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9')),
            "Windows device paths are forbidden"
        );
    }
    Ok(())
}
fn plain_metadata(path: &Path) -> anyhow::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "Artifact links are forbidden"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Artifact reparse points are forbidden"
        );
    }
    Ok(metadata)
}
fn resolve_plain_file(root: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    strict_relative(relative)?;
    plain_metadata(root)?;
    let mut path = root.to_path_buf();
    for part in relative.split(['/', '\\']) {
        path.push(part);
        plain_metadata(&path)?;
    }
    ensure!(path.is_file(), "Artifact is not a regular file");
    ensure!(
        path.canonicalize()?.starts_with(root.canonicalize()?),
        "Artifact escaped its run directory"
    );
    Ok(path)
}
fn check_file(file: &File) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
            "Cannot inspect artifact handle"
        );
        ensure!(
            info.nNumberOfLinks == 1 && info.dwFileAttributes & 0x400 == 0,
            "Hard-linked or reparse artifacts are forbidden"
        );
    }
    Ok(())
}
pub(super) fn bounded_file(root: &Path, relative: &str, limit: u64) -> anyhow::Result<Vec<u8>> {
    let path = resolve_plain_file(root, relative)?;
    let file = File::open(path)?;
    check_file(&file)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "Artifact exceeds the bounded import/read limit"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Artifact grew beyond the bounded read limit"
    );
    Ok(bytes)
}

#[derive(Default)]
struct Inventory {
    bytes: u64,
    rows: Vec<Value>,
}
fn scan_work(root: &Path, hashes: bool) -> anyhow::Result<Inventory> {
    fn walk(
        root: &Path,
        dir: &Path,
        depth: usize,
        hashes: bool,
        count: &mut usize,
        out: &mut Inventory,
    ) -> anyhow::Result<()> {
        ensure!(depth <= 16, "Output directory depth exceeds 16");
        plain_metadata(dir)?;
        for entry in fs::read_dir(dir)? {
            *count += 1;
            ensure!(
                *count <= MAX_FILES,
                "Output directory exceeds 10000 entries"
            );
            let path = entry?.path();
            let metadata = plain_metadata(&path)?;
            if metadata.is_dir() {
                walk(root, &path, depth + 1, hashes, count, out)?;
            } else {
                ensure!(metadata.is_file(), "Non-file outputs are forbidden");
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                strict_relative(&relative)?;
                out.bytes = out
                    .bytes
                    .checked_add(metadata.len())
                    .context("Storage size overflow")?;
                let mut row = json!({"path":format!("work/{relative}"),"bytes":metadata.len()});
                if hashes {
                    let mut file = File::open(&path)?;
                    check_file(&file)?;
                    let mut hasher = Sha256::new();
                    let mut buffer = [0_u8; 65536];
                    loop {
                        let size = file.read(&mut buffer)?;
                        if size == 0 {
                            break;
                        }
                        hasher.update(&buffer[..size]);
                    }
                    row["sha256"] = json!(format!("{:x}", hasher.finalize()));
                }
                out.rows.push(row);
            }
        }
        Ok(())
    }
    let mut result = Inventory::default();
    walk(root, root, 0, hashes, &mut 0, &mut result)?;
    result
        .rows
        .sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(result)
}

#[cfg(windows)]
pub(super) fn free_bytes(path: &Path) -> anyhow::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    ensure!(
        unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } != 0,
        "Cannot check free storage"
    );
    Ok(available)
}
#[cfg(not(windows))]
pub(super) fn free_bytes(_: &Path) -> anyhow::Result<u64> {
    bail!("Validated generated-code storage monitoring requires Windows")
}

struct StorageReserve {
    path: PathBuf,
    file: Option<File>,
}
impl StorageReserve {
    fn new(path: PathBuf) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut reserve = Self {
            path,
            file: Some(file),
        };
        let buffer = vec![0_u8; MIB as usize];
        for _ in 0..(RESERVE_BYTES / MIB) {
            reserve.file.as_mut().unwrap().write_all(&buffer)?;
        }
        reserve.file.as_mut().unwrap().sync_all()?;
        Ok(reserve)
    }
}
impl Drop for StorageReserve {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::AppConfig,
        domain::{CreateProjectRequest, ResearchProject},
        persistence::Database,
    };

    fn input(code: &str) -> Value {
        json!({"engine":"python_numpy","code":code,"inputs":{},"sources":[],"limits":{"memory_mb":256,"process_limit":2,"wall_seconds":null,"storage_mb":64}})
    }
    fn service(directory: &Path) -> (LaboratoryService, Uuid) {
        fs::create_dir_all(directory).unwrap();
        let config = AppConfig {
            data_directory: directory.into(),
            ..Default::default()
        };
        let database = Database::open(&config.database_path()).unwrap();
        let project = ResearchProject::new(CreateProjectRequest {
            name: Some("Isolated generated execution acceptance".into()),
            question: "Verify actual computation and retained evidence".into(),
        });
        database.put_project(&project).unwrap();
        (
            LaboratoryService::new(database, config).unwrap(),
            project.id,
        )
    }
    fn create(service: &LaboratoryService, project: Uuid, input: Value) -> LabJob {
        service
            .create(
                Uuid::new_v4(),
                project,
                None,
                "generated",
                "Acceptance computation",
                input,
                None,
            )
            .unwrap()
    }
    async fn finished(service: &LaboratoryService, id: Uuid) -> LabJob {
        tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                let job = service.get(id).unwrap();
                if !job.active() && !service.executing(id) {
                    return job;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("Generated fixture did not finish")
    }

    #[test]
    fn generated_contract_requires_explicit_time_choice_and_bounded_safe_paths() {
        let valid = input("print('test')");
        assert_eq!(
            validate_generated_input(&valid)
                .unwrap()
                .limits
                .wall_seconds,
            None
        );
        let mut missing = valid.clone();
        missing["limits"]
            .as_object_mut()
            .unwrap()
            .remove("wall_seconds");
        assert!(validate_generated_input(&missing).is_err());
        for bad in [
            "../secret",
            "C:/secret",
            "\\\\host\\share",
            "file:stream",
            "NUL.txt",
            "folder/../file",
            "folder/name.",
        ] {
            assert!(strict_relative(bad).is_err(), "{bad}");
        }
        for (field, value) in [
            ("memory_mb", 127),
            ("process_limit", 17),
            ("storage_mb", 4097),
            ("wall_seconds", 0),
        ] {
            let mut bad = valid.clone();
            bad["limits"][field] = json!(value);
            assert!(validate_generated_input(&bad).is_err());
        }
    }

    #[tokio::test]
    async fn generated_runtime_v4_rejects_prior_attempt_before_mutation_or_provisioning() {
        for (prior,mixed) in ["python-numpy-v2","python-numpy-v3"].into_iter().flat_map(|prior| [false,true].map(|mixed|(prior,mixed))) {
            let temp = tempfile::tempdir().unwrap();
            let (service, project) = service(temp.path());
            let job = create(&service, project, input("pass"));
            service.event(job.id, "runtime_verified", "Previous runtime", json!({"kind":prior,"manifest_sha256":"bb2bbd5163b1cc488fa32e7b663974b3bb23b3edf6dc579af6e9fd7c138d3a52"})).unwrap();
            if mixed {
                service.event(job.id, "runtime_verified", "A later matching event cannot erase prior identity", json!({"kind":super::super::runtime::RuntimeKind::Generated.name(),"manifest_sha256":super::super::runtime::RuntimeKind::Generated.manifest_sha256()})).unwrap();
            }
            let root = service.directory(job.id);
            fs::write(root.join("source.py"), b"retained original source").unwrap();
            let before = serde_json::to_vec(&service.get(job.id).unwrap()).unwrap();
            let token = CancellationToken::new();
            assert!(service.start_generated(job.id).unwrap_err().to_string().contains("Original generated runtime differs"));
            assert!(service.run_generated(job.id, &token).await.unwrap_err().to_string().contains("Original generated runtime differs"));
            assert!(service.ensure_generated_runtime(job.id, &token).await.unwrap_err().to_string().contains("Original generated runtime differs"));
            let request = validate_generated_input(&job.input).unwrap();
            assert!(service.execute_generated(job.id, &request, &token).await.unwrap_err().to_string().contains("Original generated runtime differs"));
            assert_eq!(serde_json::to_vec(&service.get(job.id).unwrap()).unwrap(), before);
            assert_eq!(service.read_generated_artifact(job.id, "source.py").unwrap(), b"retained original source");
            assert!(!service.executing(job.id));
            assert!(!service.generated_runtime_directory().exists());
            assert!(!root.join("work").exists());
        }
    }

    #[test]
    fn generated_runtime_v4_requires_identity_for_retained_evidence_and_exact_current_hash() {
        let temp = tempfile::tempdir().unwrap();
        let (service, project) = service(temp.path());
        let job = create(&service, project, input("pass"));
        service.ensure_generated_attempt_identity(job.id).unwrap();
        fs::write(service.directory(job.id).join("prepared.json"), b"{}").unwrap();
        let before = serde_json::to_vec(&service.get(job.id).unwrap()).unwrap();
        assert!(service.start_generated(job.id).unwrap_err().to_string().contains("identity is unavailable"));
        assert_eq!(serde_json::to_vec(&service.get(job.id).unwrap()).unwrap(), before);
        service.event(job.id,"runtime_verified","Current runtime",json!({"kind":super::super::runtime::RuntimeKind::Generated.name(),"manifest_sha256":super::super::runtime::RuntimeKind::Generated.manifest_sha256()})).unwrap();
        service.ensure_generated_attempt_identity(job.id).unwrap();
        service.event(job.id,"runtime_verified","Wrong current hash",json!({"kind":super::super::runtime::RuntimeKind::Generated.name(),"manifest_sha256":"wrong"})).unwrap();
        assert!(service.ensure_generated_attempt_identity(job.id).unwrap_err().to_string().contains("runtime differs"));
        assert_eq!(service.generated_runtime_directory(), service.config.data_directory.join("environments/python-numpy-v4/runtime"));
    }

    #[test]
    fn generated_runtime_v4_rejects_old_queued_replacement_before_restart_receipt_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let (service, project) = service(temp.path());
        let original = create(&service, project, input("pass"));
        service.update(original.id, |job| job.state = "paused".into()).unwrap();
        let mut request = original.input.clone();request["restarted_from_job_id"] = json!(original.id);
        let replacement = create(&service, project, request);
        service.event(replacement.id,"runtime_verified","Prior runtime",json!({"kind":"python-numpy-v2","manifest_sha256":"bb2bbd5163b1cc488fa32e7b663974b3bb23b3edf6dc579af6e9fd7c138d3a52"})).unwrap();
        let old = serde_json::to_vec(&service.get(original.id).unwrap()).unwrap();
        let queued = serde_json::to_vec(&service.get(replacement.id).unwrap()).unwrap();
        assert!(service.restart_generated(original.id,None).unwrap_err().to_string().contains("Original generated runtime differs"));
        assert_eq!(serde_json::to_vec(&service.get(original.id).unwrap()).unwrap(),old);
        assert_eq!(serde_json::to_vec(&service.get(replacement.id).unwrap()).unwrap(),queued);
        assert!(!service.executing(original.id) && !service.executing(replacement.id));
        assert_eq!(service.list(Some(project)).unwrap().len(),2);
    }

    #[tokio::test]
    async fn generated_runtime_v4_restart_of_v2_source_uses_a_new_immutable_attempt() {
        let temp = tempfile::tempdir().unwrap();
        let (service, project) = service(temp.path());
        let original = create(&service, project, input("pass"));
        service.event(original.id,"runtime_verified","Prior runtime",json!({"kind":"python-numpy-v2","manifest_sha256":"bb2bbd5163b1cc488fa32e7b663974b3bb23b3edf6dc579af6e9fd7c138d3a52"})).unwrap();
        service.update(original.id, |job| job.state="paused".into()).unwrap();
        fs::write(service.directory(original.id).join("source.py"), b"pass").unwrap();
        let permits = service.solver_slots.acquire_many(2).await.unwrap();
        let replacement = service.restart_generated(original.id,None).unwrap();
        assert_ne!(replacement.id,original.id);
        assert_eq!(replacement.input["restarted_from_job_id"],json!(original.id));
        service.ensure_generated_attempt_identity(replacement.id).unwrap();
        assert_eq!(service.get(original.id).unwrap().events.iter().filter(|e|e.kind=="runtime_verified").count(),1);
        assert_eq!(fs::read(service.directory(original.id).join("source.py")).unwrap(),b"pass");
        service.stop(replacement.id,"paused").unwrap();
        assert_eq!(finished(&service,replacement.id).await.state,"paused");drop(permits);
        assert!(!service.generated_runtime_directory().exists());
    }

    #[test]
    fn generated_imports_are_project_scoped_and_snapshots_cannot_be_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let (service, project) = service(temp.path());
        let other = ResearchProject::new(CreateProjectRequest {
            name: None,
            question: "Other project".into(),
        });
        service.database.put_project(&other).unwrap();
        let source = create(&service, other.id, input("pass"));
        service
            .update(source.id, |job| job.state = "completed".into())
            .unwrap();
        fs::write(
            service.directory(source.id).join("data.json"),
            b"{\"value\":1}",
        )
        .unwrap();
        let mut request = input("pass");
        request["sources"] =
            json!([{"job_id":source.id,"path":"data.json","destination":"data.json"}]);
        let job = create(&service, project, request);
        assert!(service
            .prepare_generated(&job, &validate_generated_input(&job.input).unwrap())
            .unwrap_err()
            .to_string()
            .contains("this project"));
        assert!(!service
            .directory(job.id)
            .join("work/imports/data.json")
            .exists());
        assert!(create_file(&service.directory(job.id).join("source.py"), b"replacement").is_err());
        assert_eq!(
            fs::read(service.directory(job.id).join("source.py")).unwrap(),
            b"pass"
        );
    }

    #[test]
    fn generated_restart_uses_retained_import_bytes_and_rejects_hardlinks() {
        let temp = tempfile::tempdir().unwrap();
        let (service, project) = service(temp.path());
        let source = create(&service, project, input("pass"));
        service
            .update(source.id, |job| job.state = "completed".into())
            .unwrap();
        let source_path = service.directory(source.id).join("data.json");
        fs::write(&source_path, b"{\"value\":1}").unwrap();
        let mut request = input("pass");
        request["sources"] = json!([{"job_id":source.id,"path":"data.json","destination":"data.json","sha256":sha(b"{\"value\":1}")}]);
        let mut mismatched = request.clone();
        mismatched["sources"][0]["sha256"] = json!("0".repeat(64));
        let mismatch_job = create(&service, project, mismatched);
        assert!(service
            .prepare_generated(
                &mismatch_job,
                &validate_generated_input(&mismatch_job.input).unwrap()
            )
            .unwrap_err()
            .to_string()
            .contains("requested SHA256"));
        let first = create(&service, project, request.clone());
        service
            .prepare_generated(&first, &validate_generated_input(&first.input).unwrap())
            .unwrap();
        service
            .update(first.id, |job| job.state = "paused".into())
            .unwrap();
        fs::write(&source_path, b"{\"value\":999}").unwrap();
        request["restarted_from_job_id"] = json!(first.id);
        let second = create(&service, project, request);
        service
            .prepare_generated(&second, &validate_generated_input(&second.input).unwrap())
            .unwrap();
        assert_eq!(
            fs::read(service.directory(second.id).join("work/imports/data.json")).unwrap(),
            b"{\"value\":1}"
        );
        #[cfg(windows)]
        {
            let hardlink = service.directory(source.id).join("linked.json");
            fs::hard_link(&source_path, &hardlink).unwrap();
            assert!(
                bounded_file(&service.directory(source.id), "linked.json", MIB)
                    .unwrap_err()
                    .to_string()
                    .contains("Hard-linked")
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_generated_restart_reuses_one_receipt_and_detaches_finished_parent() {
        let temp = tempfile::tempdir().unwrap();
        let (service, project) = service(temp.path());
        let parent = service
            .create(
                Uuid::new_v4(),
                project,
                None,
                "session",
                "finished parent",
                json!({}),
                None,
            )
            .unwrap();
        service
            .update(parent.id, |job| job.state = "completed".into())
            .unwrap();
        let original = service
            .create(
                Uuid::new_v4(),
                project,
                Some(parent.id),
                "generated",
                "restart admission",
                input("pass"),
                None,
            )
            .unwrap();
        service
            .update(original.id, |job| job.state = "paused".into())
            .unwrap();
        // Hold real solver admission so this tests concurrent durable creation
        // without requiring an installation or launching any placeholder process.
        let permits = service.solver_slots.acquire_many(2).await.unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let service = service.clone();
            let barrier = barrier.clone();
            handles.push(tokio::task::spawn_blocking(move || {
                barrier.wait();
                service.restart_generated(original.id, None).unwrap()
            }));
        }
        let mut attempts = Vec::new();
        for handle in handles {
            attempts.push(handle.await.unwrap());
        }
        let replacement = attempts[0].id;
        assert!(attempts
            .iter()
            .all(|job| job.id == replacement && job.parent_id.is_none()));
        assert_ne!(replacement, original.id);
        assert!(!service.executing(original.id));
        assert_eq!(
            service
                .get(original.id)
                .unwrap()
                .events
                .iter()
                .filter(|event| event.kind == "generated_restart_requested")
                .count(),
            1
        );
        assert_eq!(
            service
                .list(Some(project))
                .unwrap()
                .iter()
                .filter(|job| job.input["restarted_from_job_id"] == json!(original.id))
                .count(),
            1
        );
        service.stop(replacement, "paused").unwrap();
        drop(permits);
        let replacement = finished(&service, replacement).await;
        assert_eq!(replacement.state, "paused");
        let replay = service
            .restart_generated(
                original.id,
                Some(Utc::now() + chrono::Duration::seconds(60)),
            )
            .unwrap();
        assert_eq!(replay.id, replacement.id);
        assert_eq!(replay.state, "paused");
        assert!(replay.deadline_at.is_none());
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn generated_actual_lpac_compute_restart_storage_and_deadline() {
        let Some(probe_root) =
            std::env::var_os("PHASEFORGE_ISOLATION_TEST_ROOT").map(PathBuf::from)
        else {
            eprintln!("SKIP generated actual LPAC: PHASEFORGE_ISOLATION_TEST_ROOT unavailable");
            return;
        };
        let output_dir = probe_root.join(format!("service-{}", Uuid::new_v4()));
        let (service, project) = service(&output_dir);
        eprintln!(
            "Generated service acceptance artifacts: {}",
            output_dir.display()
        );
        // Start without a runtime: production copies the explicit bundled seed,
        // verifies the compiled outer/inner inventories and never installs online.
        assert!(std::env::var_os("PHASEFORGE_RUNTIME_SEED_ROOT").is_some(), "Set the explicit v2 seed root for this development acceptance");
        let code = r#"import json, pathlib, numpy as np
source = pathlib.Path('experiment.py')
source.write_text('# changed only in the disposable working copy')
a = np.array([[4., 1., 0.], [1., 3., 1.], [0., 1., 2.]])
b = np.array([1., 2., 3.])
x = np.linalg.solve(a, b)
np.savez_compressed('calculation.npz', x=x)
saved = np.load('calculation.npz', allow_pickle=False)['x']
pathlib.Path('result.json').write_text(json.dumps({'residual': float(np.linalg.norm(a @ saved - b)), 'artifacts': ['calculation.npz']}))
print('actual NumPy computation finished')
"#;
        let first = create(&service, project, input(code));
        service.start_generated(first.id).unwrap();
        let first = finished(&service, first.id).await;
        assert_eq!(first.state, "completed", "{:?}", first.error);
        assert!(
            first.result["reported_result"]["residual"]
                .as_f64()
                .unwrap()
                < 1e-12
        );
        assert_eq!(
            fs::read_to_string(service.directory(first.id).join("source.py")).unwrap(),
            code
        );
        assert_ne!(
            fs::read_to_string(service.directory(first.id).join("work/experiment.py")).unwrap(),
            code
        );
        assert!(
            service.read_json(first.id, "manifest.json").unwrap()["outcome"]["boundary"]
                == "windows_lpac_no_network"
        );
        assert!(!service
            .directory(first.id)
            .join("storage-reserve.bin")
            .exists());
        assert_eq!(
            service
                .read_generated_artifact(first.id, "work/calculation.npz")
                .unwrap(),
            fs::read(service.directory(first.id).join("work/calculation.npz")).unwrap()
        );

        // Off is represented as no deadline, remains active, then pauses on explicit cancellation.
        let hold = create(
            &service,
            project,
            input("import time\nprint('holding', flush=True)\ntime.sleep(120)\n"),
        );
        service.start_generated(hold.id).unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            while service.get(hold.id).unwrap().state != "running" {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(service.get(hold.id).unwrap().deadline_at.is_none());
        assert_eq!(service.get(hold.id).unwrap().state, "running");
        assert!(service
            .read_generated_artifact(hold.id, "work/stdout.log")
            .is_err());
        service.stop(hold.id, "paused").unwrap();
        let paused = finished(&service, hold.id).await;
        assert_eq!(paused.state, "paused");
        let old_input = fs::read(service.directory(hold.id).join("input.json")).unwrap();
        let restarted = service
            .restart_generated(hold.id, Some(Utc::now() + chrono::Duration::seconds(2)))
            .unwrap();
        assert_ne!(restarted.id, hold.id);
        assert_eq!(restarted.input["restarted_from_job_id"], json!(hold.id));
        let restarted = finished(&service, restarted.id).await;
        assert_eq!(restarted.state, "timed_out", "{:?}", restarted.error);
        assert_eq!(
            fs::read(service.directory(hold.id).join("input.json")).unwrap(),
            old_input
        );
        assert_eq!(service.get(hold.id).unwrap().state, "paused");

        let storage_code = "import time\nwith open('large.bin', 'wb') as f:\n for _ in range(200):\n  f.write(b'x' * (1024*1024)); f.flush(); time.sleep(0.008)\ntime.sleep(60)\n";
        let oversized = create(&service, project, input(storage_code));
        service.start_generated(oversized.id).unwrap();
        let oversized = finished(&service, oversized.id).await;
        assert_eq!(oversized.state, "failed");
        assert!(oversized.error.unwrap().contains("monitored storage"));
        let receipt = service
            .read_json(oversized.id, "storage-stop.json")
            .unwrap();
        assert!(receipt["observed_bytes"].as_u64().unwrap() > 64 * MIB);
        assert_eq!(receipt["hard_quota"], false);
        assert!(!service
            .directory(oversized.id)
            .join("storage-reserve.bin")
            .exists());

        // Only safe finite JSON is registered, even when Python exits successfully.
        let invalid = create(
            &service,
            project,
            input("open('result.json', 'w').write('{\"value\": NaN}')\n"),
        );
        service.start_generated(invalid.id).unwrap();
        let invalid = finished(&service, invalid.id).await;
        assert_eq!(invalid.state, "failed");
        assert!(invalid.error.unwrap().contains("finite JSON"));
    }
}
