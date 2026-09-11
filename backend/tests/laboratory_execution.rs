//! Behavioral acceptance for durable jobs and trusted-worker supervision.
//! Run Windows process cases with PHASEFORGE_TEST_PYTHON set to an absolute Python path.
use std::{
    fs,
    sync::{Arc, Barrier},
};

use phaseforge_backend::{
    config::AppConfig,
    domain::{CreateProjectRequest, ResearchProject},
    laboratory::{safe_relative, LaboratoryService},
    persistence::Database,
};
use serde_json::{json, Value};
use tempfile::TempDir;
use uuid::Uuid;

fn laboratory() -> (TempDir, LaboratoryService, Uuid) {
    let temp = tempfile::tempdir().unwrap();
    let config = AppConfig {
        data_directory: temp.path().into(),
        ..Default::default()
    };
    let database = Database::open(&config.database_path()).unwrap();
    let project = ResearchProject::new(CreateProjectRequest {
        name: Some("Runtime acceptance fixture".into()),
        question: "Test saved execution boundaries".into(),
    });
    database.put_project(&project).unwrap();
    let service = LaboratoryService::new(database, config).unwrap();
    (temp, service, project.id)
}

fn input() -> Value {
    json!({"engine":"openmm_argon","parameters":{"steps":100,"seed":101}})
}

#[test]
fn concurrent_request_replay_is_idempotent_and_conflicts_are_rejected() {
    let (_temp, service, project) = laboratory();
    let id = Uuid::new_v4();
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let service = service.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                service
                    .create(id, project, None, "solver", "Same request", input(), None)
                    .unwrap()
            })
        })
        .collect();
    let records: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert!(records
        .iter()
        .all(|record| record.id == id && record.events.len() == 1));
    assert_eq!(service.list(None).unwrap().len(), 1);
    assert_eq!(service.read_json(id, "input.json").unwrap(), input());
    assert!(service
        .create(
            id,
            project,
            None,
            "solver",
            "Changed",
            json!({"engine":"different"}),
            None
        )
        .is_err());
    assert!(service
        .create(id, project, None, "session", "Changed", input(), None)
        .is_err());
    assert!(service
        .create(
            id,
            project,
            Some(Uuid::new_v4()),
            "solver",
            "Changed",
            input(),
            None
        )
        .is_err());
    let other = ResearchProject::new(CreateProjectRequest {
        name: None,
        question: "Other project".into(),
    });
    service.database.put_project(&other).unwrap();
    assert!(service
        .create(id, other.id, None, "solver", "Changed", input(), None)
        .is_err());
    assert_eq!(service.get(id).unwrap().events.len(), 1);
}

#[test]
fn process_restart_pauses_active_jobs_and_new_attempt_preserves_the_original() {
    let (_temp, service, project) = laboratory();
    let config = service.config.clone();
    let states = [
        "queued",
        "running",
        "provisioning",
        "waiting",
        "completed",
        "failed",
        "cancelled",
    ];
    let mut ids = vec![];
    for state in states {
        let id = Uuid::new_v4();
        service
            .create(id, project, None, "solver", state, input(), None)
            .unwrap();
        service
            .update(id, |job| {
                job.state = state.into();
                job.result = json!({"retained":state});
            })
            .unwrap();
        fs::write(service.directory(id).join("retained.bin"), state.as_bytes()).unwrap();
        ids.push(id);
    }
    drop(service);
    let database = Database::open(&config.database_path()).unwrap();
    let restored = LaboratoryService::new(database, config.clone()).unwrap();
    for (index, id) in ids.iter().enumerate() {
        let job = restored.get(*id).unwrap();
        assert_eq!(job.state, if index < 4 { "paused" } else { states[index] });
        assert_eq!(job.result, json!({"retained":states[index]}));
        assert_eq!(
            job.events
                .iter()
                .filter(|event| event.kind == "interrupted")
                .count(),
            usize::from(index < 4)
        );
        assert_eq!(
            fs::read(restored.path(*id, "retained.bin").unwrap()).unwrap(),
            states[index].as_bytes()
        );
        assert!(job
            .events
            .iter()
            .enumerate()
            .all(|(index, event)| event.sequence == index + 1));
    }
    let source = restored.get(ids[1]).unwrap();
    let original = serde_json::to_value(&source).unwrap();
    let next_id = Uuid::new_v4();
    let mut next_input = source.input.clone();
    next_input["restarted_from_job_id"] = json!(source.id);
    let next = restored
        .create(
            next_id,
            project,
            source.parent_id,
            "solver",
            &source.title,
            next_input,
            None,
        )
        .unwrap();
    assert_eq!(next.state, "queued");
    assert_eq!(next.input["restarted_from_job_id"], source.id.to_string());
    assert_ne!(restored.directory(next_id), restored.directory(source.id));
    assert!(!restored.directory(next_id).join("retained.bin").exists());
    assert_eq!(
        serde_json::to_value(restored.get(source.id).unwrap()).unwrap(),
        original
    );
    drop(restored);
    let again =
        LaboratoryService::new(Database::open(&config.database_path()).unwrap(), config).unwrap();
    assert_eq!(
        again
            .get(source.id)
            .unwrap()
            .events
            .iter()
            .filter(|event| event.kind == "interrupted")
            .count(),
        1
    );
}

#[test]
fn stopping_a_parent_cancels_active_descendants_and_keeps_completed_evidence() {
    let (_temp, service, project) = laboratory();
    let parent = Uuid::new_v4();
    let child = Uuid::new_v4();
    let grandchild = Uuid::new_v4();
    let complete = Uuid::new_v4();
    service
        .create(
            parent,
            project,
            None,
            "session",
            "Parent",
            json!({"content":"Run"}),
            None,
        )
        .unwrap();
    for (id, owner) in [(child, parent), (grandchild, child), (complete, parent)] {
        service
            .create(id, project, Some(owner), "solver", "Child", input(), None)
            .unwrap();
    }
    let tokens: Vec<_> = [parent, child, grandchild]
        .iter()
        .map(|id| service.acquire(*id).unwrap())
        .collect();
    assert!(
        service.acquire(child).is_err(),
        "Concurrent execution of one identity must be refused"
    );
    service
        .update(complete, |job| job.state = "completed".into())
        .unwrap();
    fs::write(
        service.directory(complete).join("result.json"),
        b"{\"retained\":true}",
    )
    .unwrap();
    service.stop(parent, "paused").unwrap();
    assert!(tokens.iter().all(|token| token.is_cancelled()));
    for id in [parent, child, grandchild] {
        assert_eq!(service.get(id).unwrap().state, "paused");
        service.release(id);
    }
    assert_eq!(service.get(complete).unwrap().state, "completed");
    assert_eq!(
        service.read_json(complete, "result.json").unwrap(),
        json!({"retained":true})
    );
    let replay = service
        .create(
            child,
            project,
            Some(parent),
            "solver",
            "Child",
            input(),
            None,
        )
        .unwrap();
    assert_eq!(
        replay.state, "paused",
        "A duplicate submission must not resurrect work"
    );
}

#[test]
fn artifact_reads_are_bounded_and_cannot_cross_job_directories() {
    let (_temp, service, project) = laboratory();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    for id in [first, second] {
        service
            .create(id, project, None, "solver", "Artifact", input(), None)
            .unwrap();
    }
    fs::create_dir(service.directory(first).join("trajectory")).unwrap();
    fs::write(
        service.directory(first).join("trajectory/chunk.json"),
        b"{\"frames\":[]}",
    )
    .unwrap();
    fs::write(
        service.directory(second).join("private.txt"),
        b"Sibling evidence",
    )
    .unwrap();
    assert_eq!(
        service.read_json(first, "trajectory/chunk.json").unwrap(),
        json!({"frames":[]})
    );
    for path in [
        "",
        ".",
        "..",
        "../private.txt",
        "/absolute",
        "C:/absolute",
        "a\\b",
        "file:stream",
        "//server/share",
        "trajectory/../../private.txt",
    ] {
        assert!(safe_relative(path).is_err(), "Unexpectedly admitted {path}");
        assert!(service.path(first, path).is_err());
    }
    assert!(service
        .path(first, &format!("../{second}/private.txt"))
        .is_err());
    assert!(service.path(first, "trajectory").is_err());
    assert!(service.path(first, "private.txt").is_err());
    fs::File::create(service.directory(first).join("oversized.json"))
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(service.read_json(first, "oversized.json").is_err());
    let inventory = service.artifact_inventory(first).unwrap();
    assert!(inventory
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["path"] == "trajectory/chunk.json"));
    assert!(!inventory.to_string().contains("private.txt"));
}

#[cfg(windows)]
mod windows {
    use super::*;
    use phaseforge_backend::laboratory::process::{clean_command, OwnedProcess};
    use std::{
        path::{Path, PathBuf},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    use tokio_util::sync::CancellationToken;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, WaitForSingleObject},
    };

    fn python() -> Option<PathBuf> {
        let Some(path) = std::env::var_os("PHASEFORGE_TEST_PYTHON").map(PathBuf::from) else {
            eprintln!("SKIP Windows process acceptance: set PHASEFORGE_TEST_PYTHON to an absolute Python executable");
            return None;
        };
        assert!(
            path.is_absolute() && path.is_file(),
            "PHASEFORGE_TEST_PYTHON must identify an existing absolute executable"
        );
        Some(path)
    }

    // Keep an OS handle, rather than checking only a reusable PID.
    struct ProcessHandle(HANDLE);
    impl ProcessHandle {
        fn open(pid: u32) -> Self {
            let handle = unsafe { OpenProcess(0x00100000, 0, pid) }; // SYNCHRONIZE
            assert!(
                !handle.is_null(),
                "Cannot retain process handle: {}",
                std::io::Error::last_os_error()
            );
            Self(handle)
        }
        fn running(&self) -> bool {
            unsafe { WaitForSingleObject(self.0, 0) == WAIT_TIMEOUT }
        }
        fn assert_exited(&self) {
            assert_eq!(
                unsafe { WaitForSingleObject(self.0, 3000) },
                WAIT_OBJECT_0,
                "A supervised process survived termination"
            );
        }
    }
    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    async fn receipt(path: &Path) -> Value {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(bytes) = fs::read(path) {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    return value;
                }
            }
            assert!(
                Instant::now() < until,
                "Worker did not produce fixture receipt at {}",
                path.display()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn tree(python: &Path, dir: &Path) -> (OwnedProcess, Vec<ProcessHandle>) {
        let script = r#"import json, os, pathlib, subprocess, sys, time
root = pathlib.Path(sys.argv[1])
child = subprocess.Popen([sys.executable, '-I', '-c', 'import time; time.sleep(120)'], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
(root / 'tree.json').write_text(json.dumps({'parent': os.getpid(), 'child': child.pid}))
time.sleep(120)
"#;
        let mut command = clean_command(python, dir);
        command
            .args(["-I", "-c", script])
            .arg(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let process = OwnedProcess::spawn(&mut command, 128).unwrap();
        let evidence = receipt(&dir.join("tree.json")).await;
        // Windows venv python.exe may be a launcher whose PID differs from the
        // running interpreter. Verify the launcher and both fixture processes.
        let mut ids = vec![
            process.id(),
            evidence["parent"].as_u64().unwrap() as u32,
            evidence["child"].as_u64().unwrap() as u32,
        ];
        ids.sort_unstable();
        ids.dedup();
        assert!(
            ids.len() >= 2,
            "Fixture must actually spawn a child process"
        );
        let handles: Vec<_> = ids.into_iter().map(ProcessHandle::open).collect();
        assert!(
            handles.iter().all(ProcessHandle::running),
            "All fixture processes must first be alive"
        );
        (process, handles)
    }

    #[tokio::test]
    async fn token_cancellation_terminates_parent_and_child_in_both_stop_modes() {
        let Some(python) = python() else {
            return;
        };
        for cooperative in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let (mut process, handles) = tree(&python, dir.path()).await;
            let token = CancellationToken::new();
            token.cancel();
            let start = Instant::now();
            let result = if cooperative {
                tokio::time::timeout(
                    Duration::from_secs(6),
                    process.wait_cooperative(&token, &dir.path().join("cancel.request")),
                )
                .await
                .unwrap()
            } else {
                tokio::time::timeout(Duration::from_secs(3), process.wait(&token))
                    .await
                    .unwrap()
            };
            assert!(result.is_err());
            if cooperative {
                assert!(dir.path().join("cancel.request").is_file());
                assert!(start.elapsed() >= Duration::from_millis(2800));
            }
            for handle in handles {
                handle.assert_exited();
            }
        }
    }

    #[tokio::test]
    async fn dropping_supervisor_terminates_parent_and_child() {
        let Some(python) = python() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let (process, handles) = tree(&python, dir.path()).await;
        drop(process);
        for handle in handles {
            handle.assert_exited();
        }
    }

    #[tokio::test]
    async fn cooperative_stop_allows_a_real_checkpoint_before_exit() {
        let Some(python) = python() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let script = r#"import json, pathlib, sys, time
root = pathlib.Path(sys.argv[1])
(root / 'ready.json').write_text('{}')
while not (root / 'cancel.request').exists(): time.sleep(.01)
(root / 'checkpoint.json').write_text(json.dumps({'step': 42, 'saved_after_signal': True}))
sys.exit(3)
"#;
        let mut command = clean_command(&python, dir.path());
        command
            .args(["-I", "-c", script])
            .arg(dir.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut process = OwnedProcess::spawn(&mut command, 128).unwrap();
        receipt(&dir.path().join("ready.json")).await;
        let parent = ProcessHandle::open(process.id());
        let token = CancellationToken::new();
        token.cancel();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            process.wait_cooperative(&token, &dir.path().join("cancel.request")),
        )
        .await
        .unwrap();
        assert!(
            result.is_err(),
            "Checkpointed cancellation is not completion"
        );
        assert_eq!(
            receipt(&dir.path().join("checkpoint.json")).await,
            json!({"step":42,"saved_after_signal":true})
        );
        parent.assert_exited();
    }

    #[tokio::test]
    async fn windows_memory_limit_rejects_allocation_but_allows_small_control() {
        let Some(python) = python() else {
            return;
        };
        for (megabytes, expected_exit) in [(16, 0), (512, 37)] {
            let dir = tempfile::tempdir().unwrap();
            let script = "import sys\ntry:\n a=bytearray(int(sys.argv[1])*1024*1024)\n a[-1]=1\nexcept MemoryError:\n sys.exit(37)\n";
            let mut command = clean_command(&python, dir.path());
            command
                .args(["-I", "-c", script])
                .arg(megabytes.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut process = OwnedProcess::spawn(&mut command, 128).unwrap();
            let status = tokio::time::timeout(
                Duration::from_secs(10),
                process.wait(&CancellationToken::new()),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                status.code(),
                Some(expected_exit),
                "Unexpected exit for {megabytes} MiB allocation under 128 MiB process cap"
            );
        }
    }

    #[test]
    fn worker_environment_excludes_provider_credentials_and_injection_paths() {
        let Some(python) = python() else {
            return;
        };
        let keys = [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "PHASEFORGE_LAUNCH_TOKEN",
            "AWS_SECRET_ACCESS_KEY",
            "PYTHONPATH",
            "PHASEFORGE_TEST_NOT_ALLOWED",
        ];
        if std::env::var("PHASEFORGE_TEST_ENV_HELPER").ok().as_deref() != Some("1") {
            // A helper copy of this test gets synthetic sentinels. Never mutate the
            // main test process's environment or inspect real credential values.
            let mut helper = Command::new(std::env::current_exe().unwrap());
            helper
                .args([
                    "--exact",
                    "windows::worker_environment_excludes_provider_credentials_and_injection_paths",
                    "--nocapture",
                ])
                .env("PHASEFORGE_TEST_ENV_HELPER", "1")
                .env("PHASEFORGE_TEST_PYTHON", &python);
            for key in keys {
                helper.env(key, "phaseforge-synthetic-test-sentinel");
            }
            use std::os::windows::process::CommandExt;
            helper.creation_flags(0x08000000);
            let output = helper.output().unwrap();
            assert!(
                output.status.success(),
                "Credential helper failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("SANITIZED_WORKER_EXECUTED"));
            return;
        }
        for key in keys {
            assert_eq!(
                std::env::var(key).unwrap(),
                "phaseforge-synthetic-test-sentinel"
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let script = "import json,os,pathlib,sys\nkeys=json.loads(sys.argv[1])\npathlib.Path(sys.argv[2]).write_text(json.dumps({'present': [k for k in keys if k in os.environ], 'isolated': sys.flags.isolated, 'no_user_site': os.getenv('PYTHONNOUSERSITE')}))";
        let mut command = clean_command(&python, dir.path());
        command
            .args(["-I", "-c", script])
            .arg(serde_json::to_string(&keys).unwrap())
            .arg(dir.path().join("environment.json"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut process = OwnedProcess::spawn(&mut command, 128).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let status = runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(10),
                process.wait(&CancellationToken::new()),
            )
            .await
            .unwrap()
            .unwrap()
        });
        assert!(status.success());
        let observed: Value =
            serde_json::from_slice(&fs::read(dir.path().join("environment.json")).unwrap())
                .unwrap();
        assert_eq!(
            observed,
            json!({"present":[],"isolated":1,"no_user_site":"1"})
        );
        println!("SANITIZED_WORKER_EXECUTED");
    }

    #[test]
    fn directory_links_cannot_escape_or_enter_inventory() {
        let (_temp, service, project) = laboratory();
        let id = Uuid::new_v4();
        service
            .create(id, project, None, "solver", "Symlink", input(), None)
            .unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::write(external.path().join("outside.txt"), b"External fixture").unwrap();
        let link = service.directory(id).join("external-link");
        if let Err(error) = std::os::windows::fs::symlink_dir(external.path(), &link) {
            assert_eq!(
                error.raw_os_error(),
                Some(1314),
                "Cannot prepare symlink fixture: {error}"
            );
            // Directory junctions are another real reparse-point escape and do
            // not require Developer Mode. Both paths are our temporary fixtures.
            let mut command = Command::new(
                std::env::var_os("COMSPEC")
                    .expect("COMSPEC is required for the Windows junction fixture"),
            );
            command
                .args(["/D", "/C", "mklink", "/J"])
                .arg(&link)
                .arg(external.path())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
            assert!(
                command.status().unwrap().success(),
                "Cannot prepare the directory junction escape fixture"
            );
            println!("Directory escape exercised with an actual junction (symlink privilege unavailable)");
        }
        assert!(service.path(id, "external-link/outside.txt").is_err());
        assert!(!service
            .artifact_inventory(id)
            .unwrap()
            .to_string()
            .contains("outside.txt"));
        assert!(!service
            .artifact_inventory(id)
            .unwrap()
            .to_string()
            .contains("external-link"));
        fs::remove_dir(link).unwrap();
    }
}
