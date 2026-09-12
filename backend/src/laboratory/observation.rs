//! Native additional-view instrument: render retained scientific state without rerunning a solver.
use super::{process, write_json, LabJob, LaboratoryService};
use crate::app::AppState;
use anyhow::Context;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub fn create(
    state: &Arc<AppState>,
    session: &LabJob,
    id: Uuid,
    args: &Value,
) -> anyhow::Result<LabJob> {
    let job = state.laboratory.create_observation(session.id, id, args)?;
    if job.state == "queued" && !state.laboratory.executing(job.id) {
        state.laboratory.start_observation(job.id)?;
    }
    Ok(job)
}

fn requested_time(value: f64, minimum: f64, maximum: f64) -> anyhow::Result<f64> {
    anyhow::ensure!(
        value.is_finite() && minimum.is_finite() && maximum.is_finite() && maximum >= minimum,
        "Choose a finite time within committed scientific states"
    );
    let tolerance = 64.0
        * f64::EPSILON
        * minimum
            .abs()
            .max(maximum.abs())
            .max((maximum - minimum).abs())
            .max(f64::MIN_POSITIVE);
    anyhow::ensure!(
        value >= minimum - tolerance && value <= maximum + tolerance,
        "Requested observation time is outside committed scientific states"
    );
    Ok(value.clamp(minimum, maximum))
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn bounded_bytes(path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        std::fs::metadata(path)?.len() <= 64 * 1024 * 1024,
        "Observation source exceeds the 64 MiB artifact limit"
    );
    let bytes = std::fs::read(path)?;
    anyhow::ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "Observation source exceeds the 64 MiB artifact limit"
    );
    Ok(bytes)
}

fn expected_digest(value: &Value) -> anyhow::Result<&str> {
    let value = value
        .as_str()
        .context("A committed scientific SHA-256 is required")?;
    anyhow::ensure!(
        value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "A committed scientific SHA-256 is required"
    );
    Ok(value)
}

fn entry_at(entries: &[Value], time: f64, key: &str) -> anyhow::Result<usize> {
    anyhow::ensure!(
        !entries.is_empty(),
        "There are no retained numerical states"
    );
    let times = entries
        .iter()
        .map(|entry| {
            entry[key]
                .as_f64()
                .filter(|v| v.is_finite())
                .context("Invalid retained time")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(
        times.windows(2).all(|pair| pair[1] > pair[0]),
        "Retained states must be time ordered"
    );
    let next = times.partition_point(|value| *value <= time);
    if next < times.len() {
        let mut tolerance =
            64.0 * f64::EPSILON * time.abs().max(times[next].abs()).max(f64::MIN_POSITIVE);
        if next > 0 {
            tolerance = tolerance.min((times[next] - times[next - 1]) / 4.0);
        }
        if times[next] - time <= tolerance {
            return Ok(next);
        }
    }
    Ok(next.saturating_sub(1))
}

fn index_metadata(index: &Value) -> anyhow::Result<Value> {
    let mut metadata = index
        .as_object()
        .context("Invalid scientific index")?
        .clone();
    for key in ["chunks", "frames", "end_time", "frame_count"] {
        metadata.remove(key);
    }
    Ok(Value::Object(metadata))
}

fn pin_source(
    service: &LaboratoryService,
    source: &LabJob,
    observation: Uuid,
    time: f64,
) -> anyhow::Result<Value> {
    let field = source.field_output();
    let index_path = if field {
        "fields/index.json"
    } else {
        "trajectory/index.json"
    };
    let raw = bounded_bytes(&service.path(source.id, index_path)?)?;
    let index: Value = serde_json::from_slice(&raw)?;
    let key = if field { "frames" } else { "chunks" };
    let entries = index[key]
        .as_array()
        .context("No committed source records")?;
    let number = entry_at(entries, time, if field { "time" } else { "start_time" })?;
    let entry = &entries[number];
    let mut files = serde_json::Map::new();
    for (path_key, hash_key) in if field {
        vec![("path", "sha256"), ("view_path", "view_sha256")]
    } else {
        vec![("path", "sha256")]
    } {
        let relative = entry[path_key]
            .as_str()
            .context("Missing committed source path")?;
        let hash = expected_digest(&entry[hash_key])?;
        anyhow::ensure!(
            digest(&bounded_bytes(&service.path(source.id, relative)?)?) == hash,
            "Committed scientific artifact changed before observation admission: {relative}"
        );
        files.insert(relative.into(), json!(hash));
    }
    let snapshot = service.directory(observation).join("source-pin/index.json");
    std::fs::create_dir_all(snapshot.parent().unwrap())?;
    std::fs::write(&snapshot, &raw)?;
    let mut pin = json!({"index_snapshot_path":snapshot,"index_sha256":digest(&raw),"entry_number":number,"files":files});
    if !field {
        pin["topology_sha256"] = json!(digest(&bounded_bytes(
            &service.path(source.id, "topology.json")?
        )?));
        if index.get("topology_sha256").is_some() {
            anyhow::ensure!(pin["topology_sha256"] == expected_digest(&index["topology_sha256"])?, "Scientific topology differs from its committed index digest");
        }
    }
    validate_pin(service, source.id, field, &pin)?;
    Ok(pin)
}

fn validate_pin(
    service: &LaboratoryService,
    source: Uuid,
    field: bool,
    pin: &Value,
) -> anyhow::Result<()> {
    let snapshot = std::path::Path::new(
        pin["index_snapshot_path"]
            .as_str()
            .context("This observation has no source pin; request a new observation")?,
    );
    let raw = bounded_bytes(snapshot)?;
    anyhow::ensure!(
        digest(&raw) == expected_digest(&pin["index_sha256"])?,
        "The observation index snapshot changed"
    );
    let frozen: Value = serde_json::from_slice(&raw)?;
    let live = service.read_json(
        source,
        if field {
            "fields/index.json"
        } else {
            "trajectory/index.json"
        },
    )?;
    anyhow::ensure!(
        index_metadata(&frozen)? == index_metadata(&live)?,
        "Scientific source metadata changed after observation admission"
    );
    let key = if field { "frames" } else { "chunks" };
    let number = pin["entry_number"]
        .as_u64()
        .context("Missing pinned source record")? as usize;
    let selected = frozen[key]
        .as_array()
        .and_then(|entries| entries.get(number))
        .context("Pinned scientific record is missing")?;
    anyhow::ensure!(live[key].as_array().and_then(|entries| entries.get(number)) == Some(selected), "Selected scientific record changed after observation admission; append-only source growth is allowed");
    let files = pin["files"]
        .as_object()
        .filter(|files| !files.is_empty())
        .context("Missing pinned scientific files")?;
    for (relative, hash) in files {
        anyhow::ensure!(
            digest(&bounded_bytes(&service.path(source, relative)?)?) == expected_digest(hash)?,
            "Pinned scientific artifact changed: {relative}"
        );
    }
    if !field {
        anyhow::ensure!(
            digest(&bounded_bytes(&service.path(source, "topology.json")?)?)
                == expected_digest(&pin["topology_sha256"])?,
            "Scientific topology changed after observation admission"
        );
    }
    Ok(())
}

fn validate_job_pin(service: &LaboratoryService, job: &LabJob) -> anyhow::Result<()> {
    let source = Uuid::parse_str(
        job.input["source_job_id"]
            .as_str()
            .context("Observation has no numerical source")?,
    )?;
    let source_job=service.get(source)?;
    anyhow::ensure!(
        source_job.project_id == job.project_id,
        "Observation source belongs to another project"
    );
    let metadata=super::exports::source_metadata(service,&source_job)?;
    if !metadata.is_null(){anyhow::ensure!(job.input["settings"]["source_metadata"]==metadata,"Published observation source metadata changed after admission");}
    validate_pin(
        service,
        source,
        source_job.field_output(),
        &job.input["settings"]["source_pin"],
    )
}

fn render_input(
    service: &LaboratoryService,
    source: &LabJob,
    observation: Uuid,
    args: &Value,
) -> anyhow::Result<Value> {
    let field = source.field_output();
    let metadata=super::exports::source_metadata(service,source)?;
    anyhow::ensure!(
        source.kind == "published_simulation" || (source.kind == "solver" && (field || source.input["engine"] == "openmm_argon" || source.input["engine"] == "newtonian_nbody")),
        "Additional views require a supported numerical solver result"
    );
    let index = service.read_json(
        source.id,
        if field {
            "fields/index.json"
        } else {
            "trajectory/index.json"
        },
    )?;
    let minimum = index["start_time"]
        .as_f64()
        .or_else(|| index["frames"].as_array()?.first()?["time"].as_f64())
        .context("No committed start time")?;
    let maximum = index["end_time"]
        .as_f64()
        .or_else(|| index["frames"].as_array()?.last()?["time"].as_f64())
        .context("No committed end time")?;
    let time = requested_time(
        args["time"]
            .as_f64()
            .context("A scientific time is required")?,
        minimum,
        maximum,
    )?;
    let width = args
        .get("width")
        .map(|value| value.as_u64().context("Width must be an integer"))
        .transpose()?
        .unwrap_or(1280);
    let height = args
        .get("height")
        .map(|value| value.as_u64().context("Height must be an integer"))
        .transpose()?
        .unwrap_or(720);
    anyhow::ensure!(
        (320..=2048).contains(&width)
            && (180..=2048).contains(&height)
            && width % 2 == 0
            && height % 2 == 0,
        "Observation dimensions must be even and within 320–2048 × 180–2048"
    );
    let mut presentation =
        match std::fs::read(service.directory(source.id).join("presentation.json")) {
            Ok(bytes) => serde_json::from_slice::<Value>(&bytes)?["settings"]
                .as_object()
                .cloned()
                .unwrap_or_default(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
            Err(error) => return Err(error.into()),
        };
    if let Some(patch) = args.get("presentation").filter(|value| !value.is_null()) {
        let patch = patch
            .as_object()
            .context("Presentation must be an object")?;
        anyhow::ensure!(
            serde_json::to_vec(patch)?.len() <= 128 * 1024,
            "Observation presentation exceeds 128 KiB"
        );
        presentation.extend(patch.clone());
    }
    // Native observations are exact saved states. A rendering interpolation must
    // not become evidence of a state that the numerical solver never computed.
    presentation.insert("interpolate".into(), json!(false));
    let labels = args
        .get("labels")
        .map(|value| value.as_bool().context("Labels must be true or false"))
        .transpose()?
        .unwrap_or(true);
    let pin = pin_source(service, source, observation, time)?;
    Ok(
        json!({"source_directory":service.directory(source.id),"source_pin":pin,"source_metadata":metadata,"mode":"png","width":width,"height":height,"fps":1,"playback_duration_seconds":1.0,"start_time":time,"end_time":time,"renderer":"eevee","samples":64,"labels":labels,"presentation":presentation}),
    )
}

impl LaboratoryService {
    pub fn create_observation(
        &self,
        parent: Uuid,
        id: Uuid,
        args: &Value,
    ) -> anyhow::Result<LabJob> {
        let _guard = self.gate.lock();
        let session = self.get(parent)?;
        if let Some(existing) = self.database.lab_record(id)? {
            anyhow::ensure!(
                existing.kind == "observation"
                    && existing.project_id == session.project_id
                    && existing.parent_id == Some(parent)
                    && existing.input["request"] == *args,
                "Observation request ID belongs to another request"
            );
            validate_job_pin(self, &existing)?;
            return Ok(existing);
        }
        anyhow::ensure!(
            matches!(session.kind.as_str(), "session" | "specialist")
                && session.active()
                && session.deadline_at.is_none_or(|at| at > chrono::Utc::now()),
            "An active agent session with remaining time must request the observation"
        );
        let source_id = Uuid::parse_str(
            args["source_job_id"]
                .as_str()
                .context("source_job_id is required")?,
        )?;
        let source = self.get(source_id)?;
        anyhow::ensure!(
            source.project_id == session.project_id,
            "Numerical source belongs to another project"
        );
        let settings = render_input(self, &source, id, args)?;
        self.create_locked(id,source.project_id,Some(parent),"observation",args["title"].as_str().unwrap_or("Additional numerical view"),json!({"engine":source.input["engine"],"representation":source.input["representation"],"source_job_id":source_id,"request":args,"settings":settings,"scientific_rerun":false}),session.deadline_at)
    }
    pub fn restart_observation(
        &self,
        id: Uuid,
        deadline: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<LabJob> {
        let next = {
            let _guard = self.gate.lock();
            let _reservation = self
                .acquire(id)
                .context("The observation worker is still stopping")?;
            let result = (|| -> anyhow::Result<LabJob> {
                let mut original = self.get(id)?;
                anyhow::ensure!(
                    original.kind == "observation"
                        && matches!(original.state.as_str(), "paused" | "failed" | "timed_out"),
                    "Observation is not resumable"
                );
                validate_job_pin(self, &original)?;
                if let Some(existing) = self.database.lab_records()?.into_iter().find(|job| {
                    job.kind == "observation"
                        && job.project_id == original.project_id
                        && job.input["restarted_from_observation_id"] == json!(id)
                }) {
                    return Ok(existing);
                }
                let parent = original
                    .parent_id
                    .map(|id| self.get(id))
                    .transpose()?
                    .filter(|job| {
                        job.active() && job.deadline_at.is_none_or(|at| at > chrono::Utc::now())
                    });
                let effective_deadline = parent
                    .as_ref()
                    .map(|job| job.deadline_at)
                    .unwrap_or(deadline);
                let mut input = original.input.clone();
                input["restarted_from_observation_id"] = json!(id);
                let next = self.create_locked(
                    Uuid::new_v4(),
                    original.project_id,
                    parent.as_ref().map(|job| job.id),
                    "observation",
                    &original.title,
                    input,
                    effective_deadline,
                )?;
                original.event("observation_restart", "A separate attempt will render the pinned numerical state and presentation.", json!({"new_job_id":next.id,"detached_from_parent":parent.is_none(),"deadline_at":effective_deadline}));
                self.database.put_lab_record(&original)?;
                Ok(next)
            })();
            self.release(id);
            result?
        };
        if next.state == "queued" && !self.executing(next.id) {
            self.start_observation(next.id)?;
        }
        Ok(next)
    }
    pub fn start_observation(&self, id: Uuid) -> anyhow::Result<()> {
        let token = self.acquire(id)?;
        let service = self.clone();
        tokio::spawn(async move {
            let job = match service.get(id) {
                Ok(job) => job,
                Err(_) => {
                    service.release(id);
                    return;
                }
            };
            let expiry_service = service.clone();
            let expiry = job.deadline_at.map(|deadline| {
                tokio::spawn(async move {
                    tokio::time::sleep(
                        (deadline - chrono::Utc::now()).to_std().unwrap_or_default(),
                    )
                    .await;
                    let _ = expiry_service.stop(id, "timed_out");
                })
            });
            let result = service.render_observation(id, &token).await;
            if let Some(expiry) = expiry {
                expiry.abort();
            }
            if let Err(error) = result {
                let _ = service.update(id, |job| {
                    if job.active() {
                        job.state = "failed".into();
                        job.error = Some(format!("{error:#}"));
                    }
                    job.event("observation_stopped", format!("{error:#}"), json!({}));
                });
            }
            service.release(id);
        });
        Ok(())
    }
    async fn render_observation(&self, id: Uuid, token: &CancellationToken) -> anyhow::Result<()> {
        let _slot = tokio::select! {_=token.cancelled()=>anyhow::bail!("Observation stopped in render queue"),slot=self.render_slots.acquire()=>slot?};
        let job = self.get(id)?;
        anyhow::ensure!(
            job.kind == "observation" && job.state == "queued" && !token.is_cancelled(),
            "Observation stopped before renderer launch"
        );
        validate_job_pin(self, &job)?;
        let folder = self.directory(id);
        let settings = &job.input["settings"];
        let field = job.field_output();
        write_json(&folder.join("render-input.json"), settings)?;
        std::fs::write(
            folder.join("trajectory_render.py"),
            include_str!("../../../tools/trajectory_render.py"),
        )?;
        let worker = if field {
            let worker = folder.join("field_render.py");
            std::fs::write(&worker, include_str!("../../../tools/field_render.py"))?;
            worker
        } else {
            folder.join("trajectory_render.py")
        };
        let blender = crate::studio::render::find_blender()
            .context("Blender is unavailable; no substitute view was generated")?;
        let mut command = process::clean_command(&blender, &folder);
        command
            .args([
                "--background",
                "--factory-startup",
                "--disable-autoexec",
                "--python-exit-code",
                "1",
                "--python",
            ])
            .arg(worker)
            .arg("--")
            .arg("--input")
            .arg(folder.join("render-input.json"))
            .arg("--output")
            .arg(&folder)
            .stdout(std::fs::File::create(folder.join("stdout.log"))?)
            .stderr(std::fs::File::create(folder.join("stderr.log"))?);
        let running=self.update(id,|job|{if job.state=="queued"&&!token.is_cancelled(){job.state="running".into();job.event("observation_started","Rendering a new camera view of an exact retained numerical state.",json!({"source_job_id":job.input["source_job_id"],"time":settings["start_time"],"width":settings["width"],"height":settings["height"]}));}})?;
        anyhow::ensure!(
            running.state == "running" && !token.is_cancelled(),
            "Observation stopped before process launch"
        );
        let mut child = process::OwnedProcess::spawn(&mut command, 4096)?;
        let progress_service = self.clone();
        let progress_token = token.clone();
        let progress = tokio::spawn(async move {
            loop {
                tokio::select! {_=progress_token.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(400))=>{}}
                if let Ok(value) = progress_service.read_json(id, "progress.json") {
                    let _ = progress_service.update(id, |job| {
                        if job.active() {
                            job.progress = value;
                        }
                    });
                }
            }
        });
        let status = child
            .wait_cooperative(token, &folder.join("cancel.request"))
            .await;
        progress.abort();
        let status = status?;
        anyhow::ensure!(
            status.success(),
            "Numerical view renderer failed ({status}); inspect retained stderr.log"
        );
        let receipt = self.read_json(id, "result.json")?;
        let path = folder.join("first-frame.png");
        let bytes = std::fs::read(&path)?;
        anyhow::ensure!(
            receipt["filename"] == "first-frame.png"
                && receipt["frame_count"] == 1
                && receipt["width"] == settings["width"]
                && receipt["height"] == settings["height"]
                && receipt["scientific_rerun"] == false
                && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                && bytes.len() <= 9 * 1024 * 1024,
            "Numerical observation image failed validation"
        );
        let hash = format!("{:x}", Sha256::digest(&bytes));
        anyhow::ensure!(
            receipt["sha256"] == hash,
            "Observation image digest disagrees with the renderer receipt"
        );
        let result = json!({"engine":job.input["engine"],"source_job_id":job.input["source_job_id"],"path":"first-frame.png","renderer":receipt,"artifacts":[{"path":"first-frame.png","bytes":bytes.len(),"sha256":hash}],"scientific_rerun":false,"scope":"Visual observation of an exact retained numerical state; rendering adds no computed scientific states"});
        write_json(&folder.join("observation.json"), &result)?;
        self.update(id, |job| {
            if job.state == "running" && !token.is_cancelled() {
                job.state = "completed".into();
                job.result = result;
                job.progress = json!({"fraction":1.0});
                job.event(
                    "completed",
                    "The additional numerical view and its source-state provenance are ready.",
                    json!({"path":"first-frame.png"}),
                );
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, LaboratoryService, LabJob, LabJob) {
        let root = tempfile::tempdir().unwrap();
        let config = crate::config::AppConfig {
            data_directory: root.path().into(),
            ..Default::default()
        };
        let db = crate::persistence::Database::open(&config.database_path()).unwrap();
        let project = crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest {
            name: Some("Additional view checks".into()),
            question: "observe retained data".into(),
        });
        db.put_project(&project).unwrap();
        let service = LaboratoryService::new(db, config).unwrap();
        let session = service
            .create(
                Uuid::new_v4(),
                project.id,
                None,
                "session",
                "Parent",
                json!({}),
                Some(chrono::Utc::now() + chrono::Duration::seconds(120)),
            )
            .unwrap();
        let source = service
            .create(
                Uuid::new_v4(),
                project.id,
                Some(session.id),
                "solver",
                "Retained argon",
                json!({"engine":"openmm_argon"}),
                None,
            )
            .unwrap();
        std::fs::create_dir(service.directory(source.id).join("trajectory")).unwrap();
        write_json(&service.directory(source.id).join("topology.json"), &json!({"entities":[{"id":"ar-0","element":"Ar","radius_nm":0.1}],"box_nm":[1,1,1],"position_unit":"nm"})).unwrap();
        let chunk = service.directory(source.id).join("trajectory/chunk-0.json");
        write_json(&chunk, &json!({"frames":[{"step":0,"time":0.0,"entities":[{"id":"ar-0","position":[0.1,0.2,0.3]}]},{"step":1,"time":0.1,"entities":[{"id":"ar-0","position":[0.2,0.2,0.3]}]},{"step":2,"time":0.2,"entities":[{"id":"ar-0","position":[0.3,0.2,0.3]}]}]})).unwrap();
        write_json(&service.directory(source.id).join("trajectory/index.json"), &json!({"start_time":0.0,"end_time":0.2,"frame_count":3,"time_unit":"ps","position_unit":"nm","wrapping":"periodic [0,L)","chunks":[{"path":"trajectory/chunk-0.json","sha256":digest(&std::fs::read(chunk).unwrap()),"start_time":0.0,"end_time":0.2,"start_frame":0,"frame_count":3}]})).unwrap();
        (root, service, session, source)
    }
    #[test]
    fn admission_freezes_presentation_deadline_and_replay_without_rerun() {
        let (_root, service, session, source) = fixture();
        write_json(
            &service.directory(source.id).join("presentation.json"),
            &json!({"settings":{"color":"#abcdef","camera":{"position":[1,2,3],"target":[0,0,0]}}}),
        )
        .unwrap();
        let args = json!({"source_job_id":source.id,"time":0.1,"width":640,"height":360,"presentation":{"contrast":1.2,"interpolate":true}});
        let id = Uuid::new_v4();
        let job = service.create_observation(session.id, id, &args).unwrap();
        assert_eq!(job.deadline_at, session.deadline_at);
        assert_eq!(job.input["settings"]["presentation"]["interpolate"], false);
        assert_eq!(job.input["settings"]["presentation"]["color"], "#abcdef");
        assert_eq!(job.input["scientific_rerun"], false);
        write_json(
            &service.directory(source.id).join("presentation.json"),
            &json!({"settings":{"color":"#000000"}}),
        )
        .unwrap();
        let replay = service.create_observation(session.id, id, &args).unwrap();
        assert_eq!(replay.input, job.input);
        assert_eq!(service.list(None).unwrap().len(), 3);
        let mut invalid = args.clone();
        invalid["time"] = json!(0.3);
        assert!(service
            .create_observation(session.id, Uuid::new_v4(), &invalid)
            .is_err());
        invalid = args.clone();
        invalid["width"] = json!(639);
        assert!(service
            .create_observation(session.id, Uuid::new_v4(), &invalid)
            .is_err());
        service.stop(session.id, "paused").unwrap();
        assert!(service
            .create_observation(session.id, Uuid::new_v4(), &args)
            .is_err());
        assert_eq!(service.get(id).unwrap().state, "paused");
        assert_eq!(requested_time(0.0, 0.0, 0.0).unwrap(), 0.0);
        assert!(requested_time(0.01, 0.0, 0.0).is_err());
    }
    #[tokio::test]
    async fn resume_is_one_new_attempt_and_parent_cancel_stops_the_queue() {
        let (_root, service, session, source) = fixture();
        let permit = service.render_slots.acquire().await.unwrap();
        let original = service
            .create_observation(
                session.id,
                Uuid::new_v4(),
                &json!({"source_job_id":source.id,"time":0.1}),
            )
            .unwrap();
        service.stop(original.id, "paused").unwrap();
        let next = service
            .restart_observation(original.id, session.deadline_at)
            .unwrap();
        let replay = service
            .restart_observation(original.id, session.deadline_at)
            .unwrap();
        assert_eq!(replay.id, next.id);
        assert_ne!(original.id, next.id);
        assert_eq!(service.get(original.id).unwrap().state, "paused");
        assert_eq!(next.input["settings"], original.input["settings"]);
        service.stop(session.id, "cancelled").unwrap();
        drop(permit);
        for _ in 0..30 {
            if !service.executing(next.id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(service.get(next.id).unwrap().state, "cancelled");
        assert!(!service.directory(next.id).join("first-frame.png").exists());
    }
    #[test]
    fn pin_allows_append_but_rejects_changed_selected_record_topology_or_metadata() {
        let (_root, service, session, source) = fixture();
        let args = json!({"source_job_id":source.id,"time":0.1});
        let id = Uuid::new_v4();
        let original = service.create_observation(session.id, id, &args).unwrap();
        let snapshot = std::fs::read(
            original.input["settings"]["source_pin"]["index_snapshot_path"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let mut index = service
            .read_json(source.id, "trajectory/index.json")
            .unwrap();
        index["chunks"].as_array_mut().unwrap().push(json!({"path":"trajectory/chunk-1.json","sha256":"a".repeat(64),"start_time":0.3,"end_time":0.4,"start_frame":3,"frame_count":2}));
        index["end_time"] = json!(0.4);
        index["frame_count"] = json!(5);
        write_json(
            &service.directory(source.id).join("trajectory/index.json"),
            &index,
        )
        .unwrap();
        assert_eq!(
            service
                .create_observation(session.id, id, &args)
                .unwrap()
                .input,
            original.input
        );
        assert_eq!(
            std::fs::read(
                original.input["settings"]["source_pin"]["index_snapshot_path"]
                    .as_str()
                    .unwrap()
            )
            .unwrap(),
            snapshot
        );
        let mut changed = index.clone();
        changed["time_unit"] = json!("s");
        write_json(
            &service.directory(source.id).join("trajectory/index.json"),
            &changed,
        )
        .unwrap();
        assert!(validate_job_pin(&service, &original).is_err());
        write_json(
            &service.directory(source.id).join("trajectory/index.json"),
            &index,
        )
        .unwrap();
        let topology = service.directory(source.id).join("topology.json");
        let original_topology = std::fs::read(&topology).unwrap();
        std::fs::write(&topology, b"{}").unwrap();
        assert!(validate_job_pin(&service, &original).is_err());
        std::fs::write(&topology, original_topology).unwrap();
        let chunk = service.directory(source.id).join("trajectory/chunk-0.json");
        let mut altered = std::fs::read(&chunk).unwrap();
        altered.push(b' ');
        std::fs::write(&chunk, &altered).unwrap();
        assert!(service.create_observation(session.id, id, &args).is_err());
        index["chunks"][0]["sha256"] = json!(digest(&altered));
        write_json(
            &service.directory(source.id).join("trajectory/index.json"),
            &index,
        )
        .unwrap();
        assert!(validate_job_pin(&service, &original).is_err());
    }
    #[test]
    fn missing_committed_digest_is_rejected_before_observation_creation() {
        let (_root, service, session, source) = fixture();
        let mut index = service
            .read_json(source.id, "trajectory/index.json")
            .unwrap();
        for missing in [Value::Null, json!(""), json!("abcd")] {
            index["chunks"][0]["sha256"] = missing;
            write_json(
                &service.directory(source.id).join("trajectory/index.json"),
                &index,
            )
            .unwrap();
            assert!(service
                .create_observation(
                    session.id,
                    Uuid::new_v4(),
                    &json!({"source_job_id":source.id,"time":0.1})
                )
                .is_err());
        }
        assert_eq!(service.list(None).unwrap().len(), 2);
    }

    #[test]
    fn isolated_mechanics_admission_preserves_scaled_units_and_has_no_periodic_box() {
        let (_root, service, session, source) = fixture();
        service.update(source.id, |job| job.input["engine"] = json!("newtonian_nbody")).unwrap();
        let mut index = service.read_json(source.id, "trajectory/index.json").unwrap();
        index.as_object_mut().unwrap().remove("wrapping");
        index["boundary"] = json!("isolated"); index["time_unit"] = json!("T0"); index["position_unit"] = json!("L0");
        index["chunks"][0]["bounds"] = json!({"min":[0.1,0.2,0.3],"max":[0.3,0.2,0.3]});
        write_json(&service.directory(source.id).join("trajectory/index.json"), &index).unwrap();
        write_json(&service.directory(source.id).join("topology.json"), &json!({"boundary":"isolated","units":{"position":"L0","mass":"M0","time":"T0"},"entities":[{"id":"ar-0","mass":2.0,"display_radius":0.03,"color":"#ff9933"}]})).unwrap();
        let observation = service.create_observation(session.id, Uuid::new_v4(), &json!({"source_job_id":source.id,"time":0.1})).unwrap();
        assert_eq!(observation.input["engine"], "newtonian_nbody");
        assert_eq!(observation.input["settings"]["presentation"]["interpolate"], false);
        validate_job_pin(&service, &observation).unwrap();
        assert!(service.read_json(source.id, "topology.json").unwrap().get("box_nm").is_none());
    }
    #[tokio::test]
    async fn restart_inherits_active_budget_or_detaches_stopped_expired_finished_parent() {
        for parent_state in ["active", "paused", "expired", "completed"] {
            let (_root, service, session, source) = fixture();
            let permit = service.render_slots.acquire().await.unwrap();
            let original = service
                .create_observation(
                    session.id,
                    Uuid::new_v4(),
                    &json!({"source_job_id":source.id,"time":0.1}),
                )
                .unwrap();
            service.stop(original.id, "paused").unwrap();
            if parent_state != "active" {
                service
                    .update(session.id, |job| {
                        if parent_state == "expired" {
                            job.deadline_at =
                                Some(chrono::Utc::now() - chrono::Duration::seconds(1));
                        } else {
                            job.state = parent_state.into();
                        }
                    })
                    .unwrap();
            }
            let next = service.restart_observation(original.id, None).unwrap();
            if parent_state == "active" {
                assert_eq!(next.parent_id, Some(session.id));
                assert_eq!(next.deadline_at, session.deadline_at);
            } else {
                assert_eq!(next.parent_id, None);
                assert_eq!(next.deadline_at, None);
            }
            assert_eq!(
                next.input["restarted_from_observation_id"],
                json!(original.id)
            );
            assert_eq!(
                service.restart_observation(original.id, None).unwrap().id,
                next.id
            );
            service.stop(next.id, "cancelled").unwrap();
            drop(permit);
            for _ in 0..30 {
                if !service.executing(next.id) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
    #[tokio::test]
    async fn changed_pin_prevents_restart_without_creating_a_replacement() {
        let (_root, service, session, source) = fixture();
        let original = service
            .create_observation(
                session.id,
                Uuid::new_v4(),
                &json!({"source_job_id":source.id,"time":0.1}),
            )
            .unwrap();
        service.stop(original.id, "paused").unwrap();
        std::fs::write(
            service.directory(source.id).join("trajectory/chunk-0.json"),
            b"{}",
        )
        .unwrap();
        assert!(service.restart_observation(original.id, None).is_err());
        assert_eq!(service.list(None).unwrap().len(), 3);
        assert!(!service.executing(original.id));
    }
    #[tokio::test]
    async fn actual_supervised_views_render_saved_argon_and_field_without_new_solvers() {
        let Some(root) = std::env::var_os("PHASEFORGE_OBSERVATION_TEST_ROOT") else {
            eprintln!("SKIP actual supervised views: supply PHASEFORGE_OBSERVATION_TEST_ROOT and both source fixture paths");
            return;
        };
        fn copy_records(source: &std::path::Path, target: &std::path::Path) {
            std::fs::create_dir_all(target).unwrap();
            for entry in std::fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let metadata = std::fs::symlink_metadata(entry.path()).unwrap();
                if metadata.file_type().is_symlink() {
                    continue;
                }
                let output = target.join(entry.file_name());
                if metadata.is_dir() {
                    copy_records(&entry.path(), &output);
                } else if [Some("json"), Some("npy")]
                    .contains(&entry.path().extension().and_then(|s| s.to_str()))
                {
                    std::fs::copy(entry.path(), output).unwrap();
                }
            }
        }
        let evidence = std::path::PathBuf::from(root).join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&evidence).unwrap();
        let mut cases = vec![];
        for (engine, variable, relative) in [
            (
                "openmm_argon",
                "PHASEFORGE_OBSERVATION_TEST_ARGON_SOURCE",
                "trajectory/index.json",
            ),
            (
                "diffusion_2d",
                "PHASEFORGE_OBSERVATION_TEST_FIELD_SOURCE",
                "fields/index.json",
            ),
        ] {
            let fixture_path = std::path::PathBuf::from(
                std::env::var_os(variable).expect("Both real source fixtures must be provided"),
            );
            let data = evidence.join(engine);
            std::fs::create_dir_all(&data).unwrap();
            let config = crate::config::AppConfig {
                data_directory: data,
                ..Default::default()
            };
            let db = crate::persistence::Database::open(&config.database_path()).unwrap();
            let project =
                crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest {
                    name: Some("Retained-state observation component check".into()),
                    question: "Render additional cameras without recalculation".into(),
                });
            db.put_project(&project).unwrap();
            let service = LaboratoryService::new(db, config).unwrap();
            let session = service
                .create(
                    Uuid::new_v4(),
                    project.id,
                    None,
                    "session",
                    "Observation parent",
                    json!({}),
                    Some(chrono::Utc::now() + chrono::Duration::seconds(120)),
                )
                .unwrap();
            let source = service
                .create(
                    Uuid::new_v4(),
                    project.id,
                    Some(session.id),
                    "solver",
                    "Copied real acceptance artifacts",
                    json!({"engine":engine}),
                    None,
                )
                .unwrap();
            let subdirectory = if engine == "diffusion_2d" {
                "fields"
            } else {
                "trajectory"
            };
            copy_records(
                &fixture_path.join(subdirectory),
                &service.directory(source.id).join(subdirectory),
            );
            if engine != "diffusion_2d" {
                std::fs::copy(
                    fixture_path.join("topology.json"),
                    service.directory(source.id).join("topology.json"),
                )
                .unwrap();
            }
            let before = std::fs::read(service.directory(source.id).join(relative)).unwrap();
            let index = service.read_json(source.id, relative).unwrap();
            let time = index["end_time"].as_f64().unwrap();
            let args = json!({"source_job_id":source.id,"time":time,"width":640,"height":360,"presentation":{"color":"#38a9ec","colorLow":"#1adfc1","colorHigh":"#d71dba","contrast":1.1,"camera":if engine=="diffusion_2d"{let lengths=index["lengths_um"].as_array().unwrap();let lx=lengths[0].as_f64().unwrap();let ly=lengths[1].as_f64().unwrap();json!({"position":[lx*0.7,ly*0.5,lx.max(ly)*2.0],"target":[lx*0.5,ly*0.5,0.0],"up":[0,1,0],"fov":35})}else{json!({"position":[8,5,8],"target":[2,2,2],"up":[0,1,0],"fov":38})}}});
            let job = service
                .create_observation(session.id, Uuid::new_v4(), &args)
                .unwrap();
            service.start_observation(job.id).unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(125), async {
                loop {
                    let current = service.get(job.id).unwrap();
                    if !current.active() {
                        break current;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                completed.state, "completed",
                "{engine}: {:?}",
                completed.error
            );
            assert_eq!(
                completed.result["renderer"]["endpoint_frames"][0]["display_time"],
                time
            );
            assert_eq!(completed.result["renderer"]["frame_count"], 1);
            assert_eq!(completed.deadline_at, session.deadline_at);
            assert_eq!(
                service
                    .list(None)
                    .unwrap()
                    .iter()
                    .filter(|job| job.kind == "solver")
                    .count(),
                1
            );
            assert_eq!(
                std::fs::read(service.directory(source.id).join(relative)).unwrap(),
                before
            );
            cases.push(json!({"engine":engine,"passed":true,"job_id":job.id,"source_job_id":source.id,"directory":service.directory(job.id),"result":completed.result,"source_index_unchanged":true,"solver_count":1}));
        }
        write_json(&evidence.join("report.json"),&json!({"passed":true,"component_check":"Actual supervised PNG render from copied prior numerical artifacts; no ordinary-chat claims","cases":cases})).unwrap();
        println!("Supervised observation evidence {}", evidence.display());
    }
}
