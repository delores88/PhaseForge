//! Import a stopped NR attempt without executing or changing its scientific input.
use super::{numerical_relativity::{self as nr, RuntimeRegistration}, nr_engines, LabJob, LaboratoryService};
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs::{self, File, OpenOptions}, io::{Read, Write}, path::{Component, Path, PathBuf}};
use uuid::Uuid;
use tokio_util::sync::CancellationToken;

const MIB: u64 = 1024 * 1024;
const ENGINE: &str = "athenak_gauge_wave";
const MAX_NATIVE_FILE: u64 = 256 * MIB;
// Native imports and evidence reads may stream over a GiB from WSL/UNC. Keep
// them off async workers, and avoid simultaneous full-inventory hash storms.
static NR_IO: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

// This context exists only on an owned blocking worker. It lets the common
// immutable readers stop between bounded reads without adding cancellation to
// ordinary read-only evidence callers or to post-execution retention.
thread_local! {
    static RECOVERY_CANCEL: std::cell::RefCell<Option<CancellationToken>> = const { std::cell::RefCell::new(None) };
}
struct RecoveryCancellation(Option<CancellationToken>);
impl RecoveryCancellation {
    fn enter(token: CancellationToken) -> Self {
        Self(RECOVERY_CANCEL.with(|current| current.replace(Some(token))))
    }
}
impl Drop for RecoveryCancellation {
    fn drop(&mut self) { RECOVERY_CANCEL.with(|current| { current.replace(self.0.take()); }); }
}
fn check_recovery_cancellation() -> anyhow::Result<()> {
    ensure!(!RECOVERY_CANCEL.with(|current| current.borrow().as_ref().is_some_and(CancellationToken::is_cancelled)),
        "NR retained-output recovery was cancelled; verified files remain available");
    Ok(())
}

pub(super) fn recovery_summary(job: &LabJob) -> Value {
    let Some(event) = job.events.iter().rev().find(|event| event.kind == "nr_recovery") else { return Value::Null; };
    json!({"operation_id":event.data["operation_id"],"status":event.data["status"],"phase":event.data["phase"],
        "started_at":event.data["started_at"],"updated_at":event.at,"error":event.data["error"],"reason":event.data["reason"]})
}
fn recovery_pending(job: &LabJob) -> bool {
    matches!(recovery_summary(job)["status"].as_str(),Some("started"|"cancel_requested"))
}
fn recovery_event(job: &mut LabJob, operation: Uuid, status: &str, phase: &str, message: &str, error: Option<String>, reason: Option<&str>) {
    let started_at = job.events.iter().find(|event|event.kind=="nr_recovery"&&event.data["operation_id"]==json!(operation))
        .map(|event|event.data["started_at"].clone()).unwrap_or_else(||json!(chrono::Utc::now()));
    job.event("nr_recovery",message,json!({"operation_id":operation,"status":status,"phase":phase,
        "started_at":started_at,"error":error,"reason":reason,"no_execution":true}));
}
pub(super) fn interrupt_recovery_after_restart(job: &mut LabJob) -> bool {
    if !recovery_pending(job) { return false; }
    let operation = recovery_summary(job)["operation_id"].as_str().and_then(|id|Uuid::parse_str(id).ok());
    if let Some(operation) = operation {
        recovery_event(job,operation,"cancelled","finished","The runtime restarted during retained-output recovery. Verified files remain available; recovery was not restarted automatically.",None,Some("backend_restart"));
        true
    } else { false }
}

pub(crate) async fn blocking_io<T: Send + 'static>(operation: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> anyhow::Result<T> {
    let permit = NR_IO.acquire().await.context("NR retained-file queue closed")?;
    tokio::task::spawn_blocking(move || {
        // Own the permit inside the blocking closure even if its HTTP caller
        // disconnects. Never detach a writer and admit another overlapping one.
        let _permit = permit;
        operation()
    }).await.context("NR retained-file worker failed")?
}

fn engine(value: &Value) -> anyhow::Result<&str> {
    let name = value.as_str().context("Missing retained NR engine identity")?;
    ensure!(nr_engines::supports(name), "Unsupported retained NR engine identity");
    Ok(name)
}

fn plain(metadata: &fs::Metadata) -> anyhow::Result<()> {
    ensure!(!metadata.file_type().is_symlink() && (metadata.is_file() || metadata.is_dir()), "NR retention refuses linked or special paths");
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        ensure!(metadata.file_attributes() & 0x400 == 0, "NR retention refuses reparse points");
    }
    Ok(())
}

#[cfg(windows)]
fn verify_handle(file: &File, path: &Path, directory: bool) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, GetFinalPathNameByHandleW, GetLongPathNameW, BY_HANDLE_FILE_INFORMATION};
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    ensure!(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } != 0, "Cannot inspect retained NR handle");
    ensure!(information.dwFileAttributes & 0x400 == 0 && (directory || information.nNumberOfLinks == 1), "NR retention refuses handle links/reparse points");
    let mut name = vec![0u16; 32768];
    let length = unsafe { GetFinalPathNameByHandleW(file.as_raw_handle(), name.as_mut_ptr(), name.len() as u32, 0) };
    ensure!(length > 0 && (length as usize) < name.len(), "Cannot inspect retained NR final path");
    let final_path = String::from_utf16(&name[..length as usize])?;
    // Windows may supply a legitimate 8.3 TEMP spelling while the opened
    // handle reports long names. Expand spelling only, not canonicalize links:
    // every ancestor remains checked and held without FILE_SHARE_DELETE.
    let spelling = path.as_os_str().encode_wide().map(|unit|if unit == b'/' as u16 {b'\\' as u16}else{unit}).collect::<Vec<_>>();
    // Unlike Rust's file open, this API does not add a verbatim namespace for
    // long absolute paths. Preserve an existing namespace; add spelling only.
    let mut requested = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            std::path::Prefix::Disk(_) => r"\\?\".encode_utf16().chain(spelling).collect::<Vec<_>>(),
            std::path::Prefix::UNC(..) => r"\\?\UNC\".encode_utf16().chain(spelling.into_iter().skip(2)).collect(),
            _ => spelling,
        },
        _ => spelling,
    };
    requested.push(0);
    let mut long_name = vec![0u16; 32768];
    let length = unsafe { GetLongPathNameW(requested.as_ptr(), long_name.as_mut_ptr(), long_name.len() as u32) };
    ensure!(length > 0 && (length as usize) < long_name.len(), "Cannot inspect retained NR long path spelling");
    let requested_path = String::from_utf16(&long_name[..length as usize])?;
    let normalize = |value: &str| {
        let value = if let Some(rest) = value.strip_prefix(r"\\?\UNC\") { format!(r"\\{rest}") }
            else { value.strip_prefix(r"\\?\").unwrap_or(value).to_owned() };
        value.replace('/', r"\").trim_end_matches('\\').to_lowercase()
    };
    ensure!(normalize(&final_path) == normalize(&requested_path), "Retained NR handle resolved through an unexpected ancestor");
    Ok(())
}

#[cfg(not(windows))]
fn verify_handle(file: &File, path: &Path, directory: bool) -> anyhow::Result<()> {
    #[cfg(unix)] {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        ensure!(directory || metadata.nlink() == 1, "NR retention refuses hard-linked files");
        #[cfg(target_os = "linux")] {
            use std::os::fd::AsRawFd;
            ensure!(fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))? == path, "Retained NR handle resolved through an unexpected ancestor");
        }
    }
    let _ = (file, path, directory);
    Ok(())
}

// Keep directory handles alive for the complete operation. On Windows denying
// FILE_SHARE_DELETE prevents an ancestor rename/reparse replacement while held.
fn directory_chain(path: &Path, create: bool) -> anyhow::Result<Vec<File>> {
    ensure!(path.is_absolute(), "NR retention requires absolute paths");
    let mut cursor = PathBuf::new();
    let mut handles = Vec::new();
    for component in path.components() {
        ensure!(!matches!(component, Component::ParentDir | Component::CurDir), "NR retention path contains traversal");
        cursor.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) { continue; }
        if create && matches!(component, Component::Normal(_)) {
            match fs::create_dir(&cursor) {
                Ok(()) => {},
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
                Err(error) => return Err(error.into()),
            }
        }
        let metadata = fs::symlink_metadata(&cursor)?;
        plain(&metadata)?;
        ensure!(metadata.is_dir(), "NR retention ancestor is not a directory");
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)] {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x02000000 | 0x00200000).share_mode(1 | 2); // BACKUP_SEMANTICS | OPEN_REPARSE_POINT; READ | WRITE
        }
        #[cfg(target_os = "linux")] {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(0x20000 | 0x10000); // O_NOFOLLOW | O_DIRECTORY
        }
        let handle = options.open(&cursor)?;
        plain(&handle.metadata()?)?;
        verify_handle(&handle, &cursor, true)?;
        handles.push(handle);
    }
    Ok(handles)
}

fn bounded(path: &Path, maximum: u64) -> anyhow::Result<Vec<u8>> {
    check_recovery_cancellation()?;
    let _parents = directory_chain(path.parent().context("NR retained file has no parent")?, false)?;
    let metadata = fs::symlink_metadata(path)?;
    plain(&metadata)?;
    ensure!(metadata.is_file() && metadata.len() <= maximum, "NR retained file exceeds its type or size limit");
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000).share_mode(1); // no writer/delete while hashing
    }
    #[cfg(target_os = "linux")] {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000);
    }
    let mut file = options.open(path)?;
    verify_handle(&file, path, false)?;
    let before = file.metadata()?;
    plain(&before)?;
    ensure!(before.len() <= maximum, "NR retained handle exceeds its size limit");
    let mut bytes = Vec::new();
    Read::by_ref(&mut file).take(maximum.saturating_add(1)).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    ensure!(bytes.len() as u64 == before.len() && before.len() == after.len() && before.modified()? == after.modified()?, "NR retained file changed during import");
    ensure!(bytes.len() as u64 <= maximum, "NR retained file grew beyond admission");
    check_recovery_cancellation()?;
    Ok(bytes)
}

fn immutable_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let _parents = directory_chain(path.parent().context("NR retained destination has no parent")?, true)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000).share_mode(1);
    }
    #[cfg(target_os = "linux")] {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000).mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(bounded(path, bytes.len() as u64)? == bytes, "An existing retained NR artifact differs; it will not be overwritten");
            return Ok(());
        },
        Err(error) => return Err(error.into()),
    };
    verify_handle(&file, path, false)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

// Native restart files can exceed 180 MiB. Hash and copy in a fixed 64 KiB
// buffer while holding the same path/handle protections as small JSON receipts.
fn stream_pinned(path: &Path, expected: u64, expected_sha: &str, mut sink: Option<&mut File>) -> anyhow::Result<()> {
    check_recovery_cancellation()?;
    ensure!(expected <= MAX_NATIVE_FILE, "NR retained file exceeds the 256 MiB per-file limit");
    let _parents = directory_chain(path.parent().context("NR source has no parent")?, false)?;
    let metadata = fs::symlink_metadata(path)?;
    plain(&metadata)?;
    ensure!(metadata.is_file() && metadata.len() == expected, "NR retained source length differs");
    let mut options = OpenOptions::new(); options.read(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000).share_mode(1);
    }
    #[cfg(target_os = "linux")] {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000);
    }
    let mut file = options.open(path)?;
    verify_handle(&file, path, false)?;
    let before = file.metadata()?; plain(&before)?;
    ensure!(before.len() == expected, "NR retained source changed before import");
    let mut buffer = [0u8; 64*1024];
    let mut total = 0u64; let mut digest = Sha256::new();
    loop {
        check_recovery_cancellation()?;
        let count = file.read(&mut buffer)?;
        if count == 0 { break; }
        total = total.checked_add(count as u64).context("NR retained source length overflow")?;
        ensure!(total <= expected, "NR retained source grew during import");
        digest.update(&buffer[..count]);
        if let Some(output) = sink.as_mut() { output.write_all(&buffer[..count])?; }
    }
    let after = file.metadata()?;
    ensure!(total == expected && after.len() == expected && before.modified()? == after.modified()?, "NR retained source changed during import");
    ensure!(format!("{:x}", digest.finalize()) == expected_sha, "NR output changed after its terminal receipt");
    Ok(())
}

fn immutable_stream(source: &Path, destination: &Path, expected: u64, expected_sha: &str) -> anyhow::Result<()> {
    // Reject linked source ancestors before creating any native destination.
    // Keep these handles alive through publication so the accepted source path
    // cannot be replaced while the destination is prepared.
    let _source_parents = directory_chain(source.parent().context("NR source has no parent")?, false)?;
    let _parents = directory_chain(destination.parent().context("NR destination has no parent")?, true)?;
    let temporary = destination.with_file_name(format!(".nr-import-{}.tmp", Uuid::new_v4()));
    let mut options = OpenOptions::new(); options.write(true).create_new(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000).share_mode(1);
    }
    #[cfg(target_os = "linux")] {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000).mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let copied = (|| -> anyhow::Result<()> {
        verify_handle(&file, &temporary, false)?;
        stream_pinned(source, expected, expected_sha, Some(&mut file))?;
        file.sync_all()?;
        check_recovery_cancellation()?;
        // Create an atomic, exclusive final name only after the complete source
        // has matched its pin. Never overwrite a prior retained result.
        match fs::hard_link(&temporary, destination) {
            Ok(()) => {},
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                stream_pinned(destination, expected, expected_sha, None)
                    .context("An existing retained NR artifact differs; it will not be overwritten")?;
            },
            Err(error) => return Err(error.into()),
        }
        Ok(())
    })();
    drop(file);
    let cleanup = fs::remove_file(&temporary);
    copied?;
    cleanup?;
    // The temporary hard link is gone; final retained files must again have one
    // link and still match the original immutable receipt.
    stream_pinned(destination, expected, expected_sha, None)
}

fn relative(value: &str) -> anyhow::Result<()> {
    super::safe_relative(value)?;
    // Retained Linux files must also have an unambiguous Windows destination.
    for part in value.split('/') {
        ensure!(!part.is_empty() && !part.ends_with(['.', ' ']) && !part.chars().any(|c| c.is_control() || "<>\"|?*".contains(c)), "NR artifact has a Windows path alias");
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(!matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") && !(stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && matches!(stem.as_bytes()[3], b'1'..=b'9')), "NR artifact has a Windows device alias");
    }
    Ok(())
}

fn pin(value: &Value) -> anyhow::Result<&str> {
    let text = value.as_str().context("Missing retained NR SHA-256")?;
    ensure!(text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "Invalid retained NR SHA-256");
    Ok(text)
}

fn retain_from_root(root: &Path, id: Uuid, directory: &Path, manifest: &Value, request: &Value, staging: &Value, cap: u64) -> anyhow::Result<Value> {
    let engine = engine(&request["engine_id"])?;
    ensure!(request["job_id"] == json!(id) && manifest["engine_id"] == engine, "NR retained job/engine identity differs");
    ensure!(staging["schema"] == "phaseforge.nr-staging.v1" && staging["job_id"] == json!(id) && staging["executed"] == false && staging["input_sha256"] == request["input"]["sha256"], "NR staging identity differs");
    pin(&request["engine_manifest_sha256"])?;
    pin(&request["input"]["sha256"])?;
    let request_pin = pin(&staging["request_sha256"])?;
    let remote_job = root.join("jobs").join(id.to_string());
    let staged_bytes = bounded(&directory.join("nr-staged-request.json"), 64 * 1024)?;
    ensure!(nr::hash(&staged_bytes) == request_pin && serde_json::from_slice::<Value>(&staged_bytes)? == *request, "Staged NR request changed after admission");
    let receipt_bytes = bounded(&remote_job.join("receipt.json"), 8 * MIB).context("No retained terminal NR receipt is available yet")?;
    let receipt: Value = serde_json::from_slice(&receipt_bytes)?;
    nr::validate_receipt(&receipt, request, staging)?;
    ensure!(receipt["executable"]["sha256"] == manifest["executable"]["sha256"] && receipt["executable"]["path"] == manifest["executable"]["path"] && receipt["source"] == manifest["source"] && receipt["build"] == manifest["build"], "NR terminal executable or source/build provenance differs");
    pin(&receipt["executable"]["sha256"])?;
    ensure!(receipt["process_group_drained"] == true, "NR receipt has not confirmed process-group drainage");
    // Preserve the exact terminal facts even if a later import detects an
    // invalid/oversized inventory. Such a failure never certifies completion.
    immutable_file(&directory.join("nr-process-receipt.json"), &receipt_bytes)?;
    let admitted_cap = request["output_limit_bytes"].as_u64().context("NR output admission is missing")?;
    ensure!(cap > 0 && cap <= nr_engines::output_cap(engine) && admitted_cap <= cap, "NR retention cap differs from supported admission");
    let files = receipt["files"].as_array().context("NR terminal receipt has no safe file inventory")?;
    ensure!(files.len() <= 4096, "NR terminal inventory exceeds file-count admission");
    let mut names = HashSet::new();
    let mut total = 0u64;
    for row in files {
        let name = row["path"].as_str().context("NR retained path is missing")?;
        relative(name)?;
        ensure!(names.insert(name.to_lowercase()), "NR inventory contains duplicate or case-aliased paths");
        let bytes = row["bytes"].as_u64().context("NR retained byte count is missing")?;
        total = total.checked_add(bytes).context("NR retained bytes overflow")?;
        ensure!(total <= cap.min(admitted_cap) && bytes <= cap.min(admitted_cap).min(MAX_NATIVE_FILE), "NR retained output exceeds the bounded import cap");
        pin(&row["sha256"])?;
    }
    ensure!(receipt["retained_bytes"] == json!(total), "NR inventory total differs from terminal receipt");
    for row in files {
        let name = row["path"].as_str().unwrap();
        let expected = row["bytes"].as_u64().unwrap();
        immutable_stream(&remote_job.join("work").join(name), &directory.join("native").join(name), expected, pin(&row["sha256"])?)?;
    }
    let completed = receipt["termination_reason"] == "completed" && receipt["exit_code"] == 0;
    // Preserve the original gauge result bytes so historical reconciliation is
    // idempotent across this engine-catalog extension.
    let scope = if nr_engines::is_gauge(engine) { "harmonic gauge-wave benchmark; flat spacetime, not a black-hole collision" } else { nr_engines::scope(engine) };
    let units = if nr_engines::is_gauge(engine) { json!({"length":"L=1","time":"L/c, c=1"}) }
        else { json!({"system":"geometrized input code units, G=c=1; no SI conversion","length":"L","time":"L/c","mass":"c^2 L/G","si_mapping":null}) };
    let result = json!({"engine":engine,"scope":scope,
        "execution_completed":completed,"termination_reason":receipt["termination_reason"],"exit_code":receipt["exit_code"],
        "numerical_validity":"not_assessed_by_process_receipt","merger_validated":false,"native_files":files,
        "process_receipt":"nr-process-receipt.json","process_receipt_sha256":nr::hash(&receipt_bytes),
        "units":units});
    immutable_file(&directory.join("result.json"), &serde_json::to_vec_pretty(&result)?)?;
    Ok(result)
}

pub(super) fn retain(runtime: &RuntimeRegistration, id: Uuid, directory: &Path, manifest: &Value, request: &Value, staging: &Value, cap: u64) -> anyhow::Result<Value> {
    runtime.validate()?;
    ensure!(request["engine_manifest_sha256"] == runtime.engine_manifest_sha256 && request["engine_id"] == runtime.engine_id && manifest["engine_id"] == runtime.engine_id, "NR runtime and request identity or pins differ");
    ensure!(request["output_dir"] == format!("{}/jobs/{id}/work", runtime.linux_root) && staging["directory"] == format!("{}/jobs/{id}", runtime.linux_root), "NR admitted runtime directory differs");
    retain_from_root(&runtime.host_root(), id, directory, manifest, request, staging, cap)
}

pub(super) async fn retain_async(runtime: RuntimeRegistration, id: Uuid, directory: PathBuf, manifest: Value,
    request: Value, staging: Value, cap: u64) -> anyhow::Result<Value> {
    blocking_io(move || retain(&runtime,id,&directory,&manifest,&request,&staging,cap)).await
}

fn saved_attempt(job: &LabJob, directory: &Path) -> anyhow::Result<Value> {
    let runtime: RuntimeRegistration = serde_json::from_slice(&bounded(&directory.join("nr-runtime-registration.json"), 64 * 1024)?)?;
    runtime.validate()?;
    let engine_bytes = bounded(&directory.join("nr-engine.json"), 64 * 1024)?;
    ensure!(nr::hash(&engine_bytes) == runtime.engine_manifest_sha256, "Saved NR engine bytes do not match the original admission pin");
    let manifest: Value = serde_json::from_slice(&engine_bytes)?;
    let request: Value = serde_json::from_slice(&bounded(&directory.join("nr-request.json"), 64 * 1024)?)?;
    let staging: Value = serde_json::from_slice(&bounded(&directory.join("nr-staging.log"), 64 * 1024)?)?;
    ensure!(request["job_id"] == json!(job.id) && request["engine_id"] == job.input["engine"] && request["deadline_at"] == json!(job.deadline_at), "Saved NR request changed original job identity or deadline");
    let admitted = job.events.iter().find(|event| event.kind == "resource_plan").context("The original NR admission event is missing")?;
    ensure!(admitted.data["request"] == request, "Saved NR request differs from the original durable admission");
    let input = bounded(&directory.join("input.athinput"), 128 * 1024)?;
    ensure!(nr::hash(&input) == request["input"]["sha256"], "Saved NR scientific input changed");
    let engine = engine(&job.input["engine"])?;
    ensure!(runtime.engine_id == engine, "Saved runtime differs from the original job engine");
    retain(&runtime, job.id, directory, &manifest, &request, &staging, nr_engines::output_cap(engine))
}

fn merge_diagnostics(job: &LabJob, directory: &Path, mut result: Value) -> anyhow::Result<(Value, Option<bool>)> {
    merge_live_monitor(job, directory, &mut result)?;
    if !nr_engines::is_gauge(engine(&job.input["engine"])?) {
        return merge_black_hole_diagnostics(job, directory, result);
    }
    let Some(saved) = job.result.get("diagnostics") else { return Ok((result, None)); };
    ensure!(saved.is_object() && saved["path"] == "diagnostics/result.json", "Saved NR diagnostic path differs");
    let bytes = bounded(&directory.join("diagnostics/result.json"), 8 * MIB)?;
    ensure!(nr::hash(&bytes) == pin(&saved["sha256"])?, "Saved NR diagnostics changed after their receipt");
    let diagnostic: Value = serde_json::from_slice(&bytes)?;
    ensure!(diagnostic["schema"] == "phaseforge.nr-gauge-result.v1" && diagnostic["engine"] == ENGINE
        && diagnostic["sources"]["execution_receipt"]["sha256"] == result["process_receipt_sha256"], "NR diagnostics refer to another execution receipt");
    ensure!(diagnostic["execution"]["receipt_type"] == "phaseforge.nr-process-receipt.v1" && diagnostic["execution"]["job_id"] == json!(job.id)
        && diagnostic["execution"]["success"] == result["execution_completed"] && diagnostic["execution"]["exit_code"] == result["exit_code"]
        && diagnostic["execution"]["termination_reason"] == result["termination_reason"] && diagnostic["execution"]["process_group_drained"] == true,
        "NR diagnostics changed their original execution outcome");
    ensure!(diagnostic["fulfillment"]["black_hole_collision_validated"] == false && diagnostic["fulfillment"]["physical_radiation"] == false, "Gauge diagnostics cannot certify a collision or physical radiation");
    let valid = diagnostic["fulfillment"]["gauge_reference_validated"].as_bool().context("NR analytic fulfillment is missing")?;
    let analytic_passed = diagnostic["analytic"]["passed"].as_bool().context("NR analytic check result is missing")?;
    ensure!(valid == (analytic_passed && result["execution_completed"] == true), "NR analytic result and fulfillment disagree");
    let artifacts = diagnostic["artifacts"].as_array().context("NR diagnostic artifact pins are missing")?;
    ensure!(artifacts.len() <= 4096, "NR diagnostic artifact count exceeds retention admission");
    let mut names = HashSet::new();
    let mut total = 0u64;
    for artifact in artifacts {
        let name = artifact["path"].as_str().context("NR diagnostic artifact path is missing")?;
        relative(name)?;
        ensure!(name != "result.json" && names.insert(name.to_lowercase()), "NR diagnostic artifact path is duplicated or recursive");
        let count = artifact["bytes"].as_u64().context("NR diagnostic artifact length is missing")?;
        total = total.checked_add(count).context("NR diagnostic artifact bytes overflow")?;
        ensure!(count <= 64*MIB && total <= 256*MIB, "NR diagnostic artifacts exceed the bounded verification budget");
        let raw = bounded(&directory.join("diagnostics").join(name), count)?;
        ensure!(raw.len() as u64 == count && nr::hash(&raw) == pin(&artifact["sha256"])?, "A retained NR diagnostic artifact changed");
    }
    for descriptor in [&diagnostic["analytic"]["report"], &diagnostic["sources"]["execution_receipt"], &diagnostic["slice_index"], &diagnostic["measurements"]] {
        ensure!(artifacts.contains(descriptor), "NR diagnostic source or measurement descriptor is outside its verified inventory");
    }
    ensure!(diagnostic["analytic"]["report"]["path"] == "analytic.json" && diagnostic["sources"]["execution_receipt"]["path"] == "sources/execution-receipt.json", "NR diagnostic source paths differ");
    let analytic: Value = serde_json::from_slice(&bounded(&directory.join("diagnostics/analytic.json"),8*MIB)?)?;
    ensure!(analytic["schema"] == "phaseforge.nr-gauge-analytic.v1" && analytic["passed"] == analytic_passed && analytic["status"] == diagnostic["analytic"]["status"], "NR analytic report disagrees with its summary");
    let summary = json!({"path":"diagnostics/result.json","sha256":nr::hash(&bytes),
        "gauge_reference_validated":valid,"analytic":diagnostic["analytic"],"slice_index":diagnostic["slice_index"],
        "frame_count":diagnostic["frame_count"],"initial_time":diagnostic["initial_time"],"final_time":diagnostic["final_time"],"scope":diagnostic["scientific_scope"]});
    ensure!(*saved == summary, "Saved NR diagnostic summary does not match its pinned original data");
    result["diagnostics"] = summary;
    Ok((result, Some(valid)))
}

fn merge_live_monitor(job: &LabJob, directory: &Path, result: &mut Value) -> anyhow::Result<()> {
    let Some(saved) = job.result.get("live_monitor") else { return Ok(()); };
    ensure!(saved["path"] == "nr-monitor/receipt.json", "Saved NR monitor has no immutable receipt");
    let raw = bounded(&directory.join("nr-monitor/receipt.json"),64*1024)?;
    ensure!(nr::hash(&raw) == pin(&saved["sha256"])?, "Retained NR monitor receipt changed");
    let receipt:Value=serde_json::from_slice(&raw)?;
    ensure!(receipt["schema"] == "phaseforge.nr-live-guard.v1" && receipt["accuracy_validated"] == false,
        "NR monitor cannot introduce numerical accuracy claims");
    for name in ["nr_live_guard.py","nr_black_hole_result.py","athenak_decode.py"] {
        let source=bounded(&directory.join("nr-monitor-tools").join(name),2*MIB)?;
        ensure!(nr::hash(&source) == pin(&receipt["source_sha256"][name])?, "Original NR monitor source changed");
    }
    let thresholds=&receipt["guard_thresholds"];
    ensure!(thresholds["expected_blocks"]==624&&thresholds["static_mesh"]==true&&thresholds["excise_chi"]==0.0625
        &&thresholds["growth_factor"]==100.0&&thresholds["initial_rms_floor"]==1e-10&&thresholds["poll_interval_seconds"]==1.0,
        "NR monitor changed the original diagnostic guard thresholds");
    let observations=bounded(&directory.join("nr-monitor/observations.jsonl"),8*MIB)?;
    ensure!(receipt["observations"]["path"]=="observations.jsonl" && receipt["observations"]["bytes"]==json!(observations.len())
        &&receipt["observations"]["sha256"]==nr::hash(&observations),"NR monitor observations changed");
    let passed=receipt["exit_code"]==0&&receipt["status"]=="stopped"&&receipt["reason"]=="caller_stopped"
        &&receipt["observed_pair_count"].as_u64().is_some_and(|v|v>0);
    let native=result["native_files"].as_array().context("Retained NR native inventory is missing")?;
    let mut chain:Option<String>=None;let mut last=None;let mut previous_time=None;let mut count=0;
    for line in observations.split_inclusive(|byte|*byte==b'\n') {
        ensure!(line.ends_with(b"\n")&&line.len()<=16384&&count<4096,"NR monitor observation line is invalid");
        let mut row:Value=serde_json::from_slice(line)?;
        ensure!(row["previous_entry_sha256"]==json!(chain)&&row["ordinal"]==count&&row["accuracy_validated"]==false,
            "NR monitor observation chain or scope differs");
        let time=row["time"].as_f64().filter(|v|v.is_finite()&&*v>=0.0).context("NR monitor time is invalid")?;
        ensure!(previous_time.is_none_or(|previous|time>previous),"NR monitor times do not increase");
        if passed {ensure!(row["finite"]==true&&row["positive_physical_spatial_metric"]==true&&row["growth_guard_passed"]==true,
            "NR monitor summary falsely claims all observations passed");}
        for name in ["metric","constraints"] {ensure!(native.contains(&row[name]),"NR monitor observation refers to another native execution");}
        chain=Some(nr::hash(line));previous_time=Some(time);count+=1;
        row.as_object_mut().context("NR monitor observation must be an object")?.remove("previous_entry_sha256");last=Some(row);
    }
    ensure!(receipt["observations"]["last_entry_sha256"]==json!(chain)&&receipt["observed_pair_count"]==count
        &&receipt["latest_observation"]==json!(last),"NR monitor terminal observation summary differs");
    if let Some(last)=&last {ensure!(receipt["last_time"]==last["time"]&&receipt["last_cycle"]==last["cycle"],"NR monitor terminal time differs");}
    let summary=json!({"path":"nr-monitor/receipt.json","sha256":nr::hash(&raw),"passed":passed,
        "status":receipt["status"],"reason":receipt["reason"],"observed_pair_count":receipt["observed_pair_count"],
        "last_time":receipt["last_time"],"last_cycle":receipt["last_cycle"],"latest_observation":receipt["latest_observation"],
        "guard_thresholds":receipt["guard_thresholds"],"accuracy_validated":false});
    ensure!(*saved==summary,"Saved NR monitor summary differs from its original receipt");
    result["live_monitor"]=summary;Ok(())
}

fn merge_black_hole_diagnostics(job: &LabJob, directory: &Path, mut result: Value) -> anyhow::Result<(Value, Option<bool>)> {
    let Some(saved) = job.result.get("diagnostics") else { return Ok((result, None)); };
    ensure!(saved.is_object() && saved["path"] == "diagnostics/result.json", "Saved NR diagnostic path differs");
    let engine = engine(&job.input["engine"])?;
    let bytes = bounded(&directory.join("diagnostics/result.json"), 8*MIB)?;
    ensure!(nr::hash(&bytes) == pin(&saved["sha256"])?, "Saved NR diagnostics changed after their receipt");
    let diagnostic: Value = serde_json::from_slice(&bytes)?;
    ensure!(diagnostic["schema"] == "phaseforge.nr-black-hole-result.v1" && diagnostic["job_id"] == json!(job.id)
        && diagnostic["engine_identity"]["engine_id"] == engine && diagnostic["sources"]["receipt_sha256"] == result["process_receipt_sha256"],
        "NR diagnostics refer to another job, engine or execution receipt");
    let artifacts = diagnostic["artifacts"].as_array().context("NR diagnostic artifact pins are missing")?;
    ensure!(artifacts.len() <= 4096, "NR diagnostic artifact count exceeds retention admission");
    let mut names = HashSet::new(); let mut total = 0u64;
    for artifact in artifacts {
        let name = artifact["path"].as_str().context("NR diagnostic artifact path is missing")?;
        relative(name)?;
        ensure!(name != "result.json" && names.insert(name.to_lowercase()), "NR diagnostic artifact path is duplicated or recursive");
        let count = artifact["bytes"].as_u64().context("NR diagnostic artifact length is missing")?;
        total = total.checked_add(count).context("NR diagnostic artifact bytes overflow")?;
        ensure!(count <= MAX_NATIVE_FILE && total <= nr_engines::output_cap(engine)+256*MIB,
            "NR diagnostic artifacts exceed the bounded streaming verification budget");
        stream_pinned(&directory.join("diagnostics").join(name), count, pin(&artifact["sha256"])?, None)?;
    }
    let descriptor = |name: &str| -> anyhow::Result<&Value> {
        artifacts.iter().find(|row| row["path"] == name).context("NR diagnostic source is outside its verified artifact inventory")
    };
    for (name, source_pin) in [("sources/execution-receipt.json", "receipt_sha256"), ("sources/engine.json", "manifest_sha256"),
        ("sources/request.json", "request_sha256"), ("sources/input.athinput", "input_sha256")] {
        ensure!(descriptor(name)?["sha256"] == pin(&diagnostic["sources"][source_pin])?, "NR diagnostic source pin differs from its registered bytes");
    }
    let receipt_bytes = bounded(&directory.join("diagnostics/sources/execution-receipt.json"),8*MIB)?;
    let receipt: Value = serde_json::from_slice(&receipt_bytes)?;
    let manifest_bytes = bounded(&directory.join("diagnostics/sources/engine.json"),64*1024)?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
    let request_bytes = bounded(&directory.join("diagnostics/sources/request.json"),64*1024)?;
    let request: Value = serde_json::from_slice(&request_bytes)?;
    let staged = bounded(&directory.join("nr-staged-request.json"),64*1024)?;
    let staging: Value = serde_json::from_slice(&bounded(&directory.join("nr-staging.log"),64*1024)?)?;
    ensure!(staged == request_bytes && nr::hash(&staged) == staging["request_sha256"], "NR diagnostics changed the staged request bytes");
    nr::validate_receipt(&receipt, &request, &staging)?;
    ensure!(manifest == diagnostic["engine_identity"] && manifest["engine_id"] == engine
        && receipt["engine_manifest_sha256"] == nr::hash(&manifest_bytes)
        && receipt["request_sha256"] == nr::hash(&request_bytes) && receipt["executable"]["path"] == manifest["executable"]["path"]
        && receipt["executable"]["sha256"] == manifest["executable"]["sha256"]
        && receipt["source"] == manifest["source"] && receipt["build"] == manifest["build"],
        "NR diagnostic execution/build/input identity differs from retained admission");
    ensure!(receipt["job_id"] == json!(job.id) && request["engine_id"] == engine
        && request["deadline_at"] == json!(job.deadline_at), "NR diagnostic request changed its original job, engine or deadline");
    ensure!(receipt["input"]["sha256"] == diagnostic["sources"]["input_sha256"], "NR diagnostic scientific input differs");
    ensure!(bounded(&directory.join("nr-engine.json"),64*1024)? == manifest_bytes
        && bounded(&directory.join("input.athinput"),128*1024)? == bounded(&directory.join("diagnostics/sources/input.athinput"),128*1024)?,
        "NR diagnostic scientific sources differ from the original admitted files");
    for row in receipt["files"].as_array().context("Missing NR source inventory")? {
        let name = row["path"].as_str().context("Missing NR native source path")?;
        relative(name)?;
        let retained = descriptor(&format!("native/{name}"))?;
        ensure!(retained["sha256"] == row["sha256"] && retained["bytes"] == row["bytes"], "NR diagnostic native data differs from the terminal inventory");
    }
    ensure!(artifacts.iter().filter(|row| row["path"].as_str().is_some_and(|path|path.starts_with("native/"))).count()
        == receipt["files"].as_array().unwrap().len(), "NR diagnostic contains unregistered native scientific data");
    for field in ["postprocessor_sha256", "decoder_sha256"] { pin(&diagnostic["sources"][field])?; }
    let commit = manifest["source"].get("athenak_commit").or_else(||manifest["source"].get("athenak"));
    ensure!(commit == Some(&diagnostic["sources"]["source_commit"]), "NR diagnostic upstream source identity differs");
    ensure!(diagnostic["sources"]["cpu_cuda_equivalence_tested"] == false, "NR diagnostic cannot introduce an unverified CPU/CUDA equivalence claim");
    let execution = &diagnostic["execution"];
    ensure!(execution["termination_reason"] == result["termination_reason"] && execution["exit_code"] == result["exit_code"]
        && execution["process_completed"] == result["execution_completed"] && execution["process_group_drained"] == true
        && execution["deadline_at"] == json!(job.deadline_at), "NR diagnostics changed the original execution outcome");
    let times = diagnostic["paired_times"].as_array().context("NR paired times are missing")?;
    let times = times.iter().map(|value| value.as_f64().filter(|n|n.is_finite() && *n>=0.0).context("Invalid retained NR time")).collect::<anyhow::Result<Vec<_>>>()?;
    ensure!(times.windows(2).all(|pair|pair[1]>pair[0]) && diagnostic["paired_state_count"] == json!(times.len()), "NR retained time order or count differs");
    let target = execution["time_target"].as_f64().filter(|n|n.is_finite() && *n>=0.0).context("Invalid NR diagnostic time target")?;
    let last = times.last().copied(); let target_met = last.is_some_and(|time|time>=target);
    let complete = result["execution_completed"] == true && target_met;
    let status = if complete {"complete_execution"} else {"partial_execution"};
    ensure!(execution["last_paired_native_time"] == json!(last) && execution["retained_time_target_met"] == target_met
        && execution["complete"] == complete && execution["status"] == status
        && execution["no_final_snapshot_fabricated"] == true, "NR diagnostic execution/time-target facts disagree");
    let unknown_boost = json!({"status":"unknown","gamma":null,"speed_over_c":null,"calibration":null});
    ensure!(diagnostic["physical_boost"] == unknown_boost && diagnostic["fulfillment"] == json!({
        "requested_0_999c_collision":false,"merger_validated":false,"horizon_properties_validated":false,"convergence_validated":false}),
        "Unvalidated NR diagnostics cannot certify boost, convergence or merger fulfillment");
    ensure!(descriptor("frames/index.json")? == &diagnostic["frame_inventory"] && descriptor("measurements.json")? == &diagnostic["measurements"],
        "NR frame or measurement source is outside the verified artifact inventory");
    let index: Value = serde_json::from_slice(&bounded(&directory.join("diagnostics/frames/index.json"),8*MIB)?)?;
    let measurements: Value = serde_json::from_slice(&bounded(&directory.join("diagnostics/measurements.json"),8*MIB)?)?;
    ensure!(index["schema"] == "phaseforge.nr-amr-frame-inventory.v1" && index["representation"] == "nr_amr_native"
        && index["paired_times"] == diagnostic["paired_times"] && index["interpolation"] == "none"
        && measurements["schema"] == "phaseforge.nr-black-hole-measurements.v1" && measurements["physical_boost"] == unknown_boost,
        "NR measurement/index schema or retained time/boost facts differ");
    let summary = json!({"path":"diagnostics/result.json","sha256":nr::hash(&bytes),"execution":execution,
        "frame_inventory":diagnostic["frame_inventory"],"measurements":diagnostic["measurements"],"paired_times":diagnostic["paired_times"],
        "paired_state_count":diagnostic["paired_state_count"],"physical_boost":diagnostic["physical_boost"],"fulfillment":diagnostic["fulfillment"],"scope":diagnostic["scope"]});
    ensure!(*saved == summary, "Saved NR diagnostic summary does not match its pinned original data");
    result["diagnostics"] = summary;
    // Verified bytes and execution/time-target facts are not a numerical-accuracy
    // gate. Reconciliation never upgrades a black-hole diagnostic's prior state.
    Ok((result, None))
}

struct Reservation<'a> { service: &'a LaboratoryService, id: Uuid }
impl Drop for Reservation<'_> { fn drop(&mut self) { self.service.release(self.id); } }
struct RecoveryReservation { service: LaboratoryService, id: Uuid }
impl Drop for RecoveryReservation { fn drop(&mut self) { self.service.release(self.id); } }

impl LaboratoryService {
    pub(crate) async fn retained_nr_io<T: Send + 'static>(operation: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> anyhow::Result<T> {
        blocking_io(operation).await
    }

    /// Verify local immutable diagnostic evidence only. No runtime registration,
    /// WSL access, import, reservation, journal update or process launch occurs.
    pub(crate) fn verified_black_hole_diagnostics(&self, job: &LabJob) -> anyhow::Result<Value> {
        let unchanged = |current: &LabJob| -> anyhow::Result<()> {
            ensure!(current.id == job.id && current.project_id == job.project_id && current.parent_id == job.parent_id
                && current.state == "completed" && current.input == job.input && current.result == job.result
                && current.deadline_at == job.deadline_at, "NR evidence selection is stale or incomplete");
            Ok(())
        };
        unchanged(&self.get(job.id)?)?;
        ensure!(job.kind == "solver" && job.state == "completed" && !nr_engines::is_gauge(engine(&job.input["engine"])?)
            && job.result["engine"] == job.input["engine"], "Puncture evidence requires its exact completed solver");
        let directory = self.directory(job.id);
        let result: Value = serde_json::from_slice(&bounded(&directory.join("result.json"), 8*MIB)?)?;
        for (key, value) in result.as_object().context("NR execution result is missing")? {
            ensure!(job.result.get(key) == Some(value), "Saved NR execution summary changed");
        }
        ensure!(result["execution_completed"] == true && result["exit_code"] == 0 && result["termination_reason"] == "completed"
            && result["merger_validated"] == false && result["process_receipt"] == "nr-process-receipt.json",
            "Puncture process did not complete its original attempt");
        let receipt = bounded(&directory.join("nr-process-receipt.json"), 8*MIB)?;
        ensure!(nr::hash(&receipt) == pin(&result["process_receipt_sha256"])?, "Original NR process receipt changed");
        let request: Value = serde_json::from_slice(&bounded(&directory.join("nr-staged-request.json"),64*1024)?)?;
        let admission = job.events.iter().find(|event|event.kind == "resource_plan").context("Original NR admission event is missing")?;
        ensure!(admission.data["request"] == request, "NR evidence differs from its durable resource admission");
        let mut result = result;
        merge_live_monitor(job, &directory, &mut result)?;
        let (verified, _) = merge_black_hole_diagnostics(job, &directory, result)?;
        ensure!(verified["diagnostics"]["execution"]["complete"] == true, "Puncture retained time target is incomplete");
        // Read the original input's time target; a rehashed diagnostic cannot
        // shorten it and then claim the original experiment has completed.
        let input = String::from_utf8(bounded(&directory.join("input.athinput"),128*1024)?)?;
        let mut time_section = false; let mut targets = Vec::new();
        for line in input.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.starts_with('<') { time_section = line == "<time>"; continue; }
            if time_section {
                if let Some((key,value)) = line.split_once('=') {
                    if key.trim() == "tlim" { targets.push(value.trim().parse::<f64>()?); }
                }
            }
        }
        ensure!(targets.len() == 1 && targets[0].is_finite() && targets[0] > 0.0
            && verified["diagnostics"]["execution"]["time_target"].as_f64() == Some(targets[0]),
            "Puncture diagnostic time target differs from its original scientific input");
        let raw = bounded(&directory.join("diagnostics/result.json"),8*MIB)?;
        ensure!(nr::hash(&raw) == pin(&verified["diagnostics"]["sha256"])?, "NR diagnostic result changed during verification");
        unchanged(&self.get(job.id)?)?;
        Ok(serde_json::from_slice(&raw)?)
    }

    /// Reserve and durably acknowledge recovery before any expensive file I/O.
    /// An HTTP disconnect cannot cancel or duplicate the owned operation.
    pub fn start_nr_recovery(&self, id: Uuid) -> anyhow::Result<LabJob> {
        self.start_nr_recovery_with(id,saved_attempt)
    }

    fn start_nr_recovery_with(&self, id: Uuid, import: impl FnOnce(&LabJob, &Path) -> anyhow::Result<Value> + Send + 'static) -> anyhow::Result<LabJob> {
        let (original, cancellation, operation) = {
            let _gate = self.gate.lock();
            let mut job = self.get(id)?;
            ensure!(job.kind == "solver" && nr_engines::supports(job.input["engine"].as_str().unwrap_or_default()) && !job.active(),
                "Only an inactive NR attempt can reconcile retained outputs");
            if self.executing(id) && recovery_pending(&job) { return Ok(job); }
            let cancellation = self.acquire(id)?;
            let operation = Uuid::new_v4();
            recovery_event(&mut job,operation,"started","queued","Retained-output recovery is queued. The original experiment and its timer are unchanged; no solver will be started.",None,None);
            if let Err(error) = self.database.put_lab_record(&job) { self.release(id); return Err(error); }
            (job,cancellation,operation)
        };
        let accepted = original.clone();
        let service = self.clone();
        let reservation = std::sync::Arc::new(RecoveryReservation { service: service.clone(), id });
        tokio::spawn(async move {
            // Cancellation while waiting never starts an import. The same-job
            // reservation was already acquired before this queue admission.
            let permit = tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    let outcome = Err(anyhow::anyhow!("Recovery cancelled before file verification started"));
                    let _ = service.finish_nr_recovery(id,operation,&outcome,&cancellation);
                    return;
                },
                permit = NR_IO.acquire() => match permit {
                    Ok(permit) => permit,
                    Err(error) => {
                        let outcome = Err(anyhow::anyhow!("NR retained-file queue closed: {error}"));
                        let _ = service.finish_nr_recovery(id,operation,&outcome,&cancellation);
                        return;
                    }
                }
            };
            let worker_service = service.clone();
            let worker_cancellation = cancellation.clone();
            let worker_reservation = reservation.clone();
            let worker = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let _reservation = worker_reservation;
                let _cancellation = RecoveryCancellation::enter(worker_cancellation.clone());
                let outcome = (|| {
                    check_recovery_cancellation()?;
                    worker_service.update(id,|job| {
                        if !worker_cancellation.is_cancelled() && recovery_summary(job)["status"] != "cancel_requested" {
                            recovery_event(job,operation,"started","verifying",
                                "Verifying and retaining the stopped attempt's exact saved files in the background. Existing results remain available.",None,None);
                        }
                    })?;
                    worker_service.reconcile_nr_reserved(original,&worker_cancellation,import)
                })();
                // Record a terminal status only after all copy/read handles have
                // drained. Keep the reservation through that durable write.
                worker_service.finish_nr_recovery(id,operation,&outcome,&worker_cancellation)
            }).await;
            match worker {
                Ok(Ok(())) => {},
                Ok(Err(error)) => { tracing::error!(job_id=%id,operation_id=%operation,error=%error,"Could not persist recovery terminal status"); },
                Err(error) => {
                    let outcome = Err(anyhow::anyhow!("Retained-output worker failed: {error}"));
                    if let Err(error) = service.finish_nr_recovery(id,operation,&outcome,&cancellation) {
                        tracing::error!(job_id=%id,operation_id=%operation,error=%error,"Could not persist failed recovery status");
                    }
                }
            }
        });
        Ok(accepted)
    }

    fn finish_nr_recovery(&self, id: Uuid, operation: Uuid, outcome: &anyhow::Result<LabJob>, cancellation: &CancellationToken) -> anyhow::Result<()> {
        let _gate = self.gate.lock();
        let mut job = self.get(id)?;
        let summary = recovery_summary(&job);
        ensure!(summary["operation_id"] == json!(operation),"Recovery operation identity changed");
        if !recovery_pending(&job) { return Ok(()); }
        let (status,message,error,reason) = match outcome {
            Ok(_) => ("completed","Retained-output recovery completed. Exact saved files were verified; no solver was started.",None,None),
            Err(error) if cancellation.is_cancelled() => ("cancelled","Retained-output recovery was cancelled and its file worker has stopped. Verified files and the original experiment remain available.",Some(format!("{error:#}").chars().take(4000).collect()),Some("user_or_parent_stop")),
            Err(error) => ("failed","Retained-output recovery failed. The original experiment is unchanged; inspect the recorded error before retrying.",Some(format!("{error:#}").chars().take(4000).collect()),Some("verification_failed")),
        };
        recovery_event(&mut job,operation,status,"finished",message,error,reason);
        if let Ok(verified) = outcome {
            if let Some(event) = job.events.last_mut() {
                event.data["process_receipt_sha256"] = verified.result["process_receipt_sha256"].clone();
            }
        }
        self.database.put_lab_record(&job)?;
        Ok(())
    }

    /// A recovery cancellation is distinct from changing scientific job state.
    /// Returning None leaves ordinary pause/cancel handling to the caller.
    pub fn cancel_nr_recovery(&self, id: Uuid, expected_operation: Option<Uuid>) -> anyhow::Result<Option<LabJob>> {
        let _gate = self.gate.lock();
        let mut job = self.get(id)?;
        if let Some(expected) = expected_operation {
            ensure!(recovery_summary(&job)["operation_id"] == json!(expected),"The selected recovery operation changed; refresh its status before cancelling");
            if !recovery_pending(&job) { return Ok(Some(job)); }
        }
        if !recovery_pending(&job) { return Ok(None); }
        let token = self.execution_token(id).context("Recovery has no owned worker; inspect restart recovery status")?;
        token.cancel();
        if recovery_summary(&job)["status"] != "cancel_requested" {
            let summary = recovery_summary(&job);
            let operation = Uuid::parse_str(summary["operation_id"].as_str().context("Recovery operation ID is missing")?)?;
            recovery_event(&mut job,operation,"cancel_requested",summary["phase"].as_str().unwrap_or("verifying"),
                "Stopping retained-output recovery. The current bounded file read will finish before cancellation is reported; the experiment's saved result and state are unchanged.",None,Some("user_stop"));
            self.database.put_lab_record(&job)?;
        }
        Ok(Some(job))
    }

    #[cfg(test)]
    fn reconcile_nr_with(&self, id: Uuid, import: impl FnOnce(&LabJob, &Path) -> anyhow::Result<Value>) -> anyhow::Result<LabJob> {
        let (original, cancellation) = {
            let _gate = self.gate.lock();
            let job = self.get(id)?;
            ensure!(job.kind == "solver" && nr_engines::supports(job.input["engine"].as_str().unwrap_or_default()) && !job.active(), "Only an inactive NR attempt can reconcile retained outputs");
            let token = self.acquire(id)?;
            (job, token)
        };
        let _reservation = Reservation { service: self, id };
        self.reconcile_nr_reserved(original,&cancellation,import)
    }

    fn reconcile_nr_reserved(&self, original: LabJob, cancellation: &CancellationToken, import: impl FnOnce(&LabJob, &Path) -> anyhow::Result<Value>) -> anyhow::Result<LabJob> {
        let id = original.id;
        ensure!(!cancellation.is_cancelled(), "NR retained-output recovery was cancelled before import");
        let result = import(&original, &self.directory(id))?;
        ensure!(result["engine"] == original.input["engine"] && result["merger_validated"] == false, "Imported NR result differs from the selected engine or claims unvalidated merger fulfillment");
        let (result, numerical_validity) = merge_diagnostics(&original, &self.directory(id), result)?;
        let _gate = self.gate.lock();
        ensure!(!cancellation.is_cancelled(), "NR retained-output recovery was cancelled; copied files remain available");
        let mut job = self.get(id)?;
        ensure!(!job.active() && job.input == original.input && job.project_id == original.project_id && job.parent_id == original.parent_id && job.deadline_at == original.deadline_at, "NR job changed while retained output was read");
        ensure!(job.result.is_null() || job.result == result, "Existing NR result differs and will not be replaced");
        let receipt_pin = pin(&result["process_receipt_sha256"])?;
        if let Some(event) = job.events.iter().find(|event| event.kind == "nr_retained") {
            ensure!(event.data["process_receipt_sha256"] == receipt_pin && job.result == result, "Previously reconciled NR receipt differs");
            return Ok(job);
        }
        job.result = result.clone();
        if result["execution_completed"] == true && numerical_validity == Some(true) {
            let newly_completed = job.state != "completed";
            job.state = "completed".into();
            job.error = None;
            if newly_completed {
                job.event("completed", "Recovered a completed Einstein-equation gauge benchmark with its exact terminal receipt and verified analytic diagnostics.", json!({"merger_validated":false,"reconciled_without_execution":true}));
            }
        } else if numerical_validity == Some(false) {
            job.state = "failed".into();
        }
        job.event("nr_retained", if numerical_validity.is_none() {
            "Imported the stopped attempt's exact native files. Numerical validation remains pending; no solver was started and the original input and deadline are unchanged."
        } else {
            "Imported the stopped attempt's exact native files and preserved verified numerical diagnostics. No solver was started and the original input and deadline are unchanged."
        }, json!({"process_receipt_sha256":receipt_pin,"execution_completed":result["execution_completed"],"termination_reason":result["termination_reason"],"gauge_reference_validated":numerical_validity,"reconciled_without_execution":true}));
        job.normalize_completion();
        self.database.put_lab_record(&job)?;
        Ok(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl LaboratoryService {
        // Reuse the small retained-provenance fixture for agent dispatch tests;
        // all data is copied between fresh private test directories only.
        pub(crate) fn synthetic_puncture_evidence_fixture(&self, project: Uuid) -> LabJob {
            let (_temporary,source,job,_)=black_hole_diagnostic_fixture("completed",true);
            self.create(job.id,project,None,"solver","Synthetic puncture evidence; no engine executed",job.input.clone(),job.deadline_at).unwrap();
            fn copy(from:&Path,to:&Path){fs::create_dir_all(to).unwrap();for entry in fs::read_dir(from).unwrap(){let entry=entry.unwrap();let target=to.join(entry.file_name());if entry.file_type().unwrap().is_dir(){copy(&entry.path(),&target)}else{fs::copy(entry.path(),target).unwrap();}}}
            copy(&source.directory(job.id),&self.directory(job.id));
            self.update(job.id,|target|{target.state="completed".into();target.result=job.result.clone();}).unwrap();
            let request:Value=serde_json::from_slice(&fs::read(self.directory(job.id).join("nr-staged-request.json")).unwrap()).unwrap();
            self.event(job.id,"resource_plan","Synthetic retained admission, no engine executed",json!({"request":request})).unwrap();
            self.get(job.id).unwrap()
        }
    }

    #[tokio::test(flavor="current_thread")]
    async fn blocking_retention_keeps_async_timer_alive_and_owns_queue_until_writer_finishes() {
        let (started,ready)=tokio::sync::oneshot::channel();
        let (release,held)=std::sync::mpsc::channel();
        let worker=tokio::spawn(blocking_io(move||{
            let _=started.send(());held.recv_timeout(std::time::Duration::from_secs(3))?;Ok(())
        }));
        ready.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_millis(250),tokio::time::sleep(std::time::Duration::from_millis(20))).await.unwrap();
        worker.abort();assert!(worker.await.unwrap_err().is_cancelled());
        let (next_started,mut next_ready)=tokio::sync::oneshot::channel();
        let next=tokio::spawn(blocking_io(move||{let _=next_started.send(());Ok(())}));
        assert!(tokio::time::timeout(std::time::Duration::from_millis(30),&mut next_ready).await.is_err(),"Caller cancellation released a still-running writer's permit");
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1),next_ready).await.unwrap().unwrap();next.await.unwrap().unwrap();
    }

    struct Fixture {
        _temporary: tempfile::TempDir,
        root: PathBuf,
        directory: PathBuf,
        id: Uuid,
        manifest: Value,
        request: Value,
        staging: Value,
        receipt: Value,
    }

    impl Fixture {
        fn new() -> Self { Self::with_id(Uuid::new_v4()) }
        fn with_id(id: Uuid) -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("synthetic-runtime");
            let directory = temporary.path().join("retained");
            let work = root.join("jobs").join(id.to_string()).join("work");
            fs::create_dir_all(work.join("fields")).unwrap();
            fs::create_dir_all(work.join("history")).unwrap();
            fs::create_dir_all(&directory).unwrap();
            let manifest = json!({"schema":"phaseforge.nr-engine.v1","engine_id":ENGINE,
                "executable":{"path":"/root/pinned/synthetic-not-executed","sha256":"e".repeat(64)},
                "source":{"synthetic_fixture":true},"build":{"synthetic_fixture":true}});
            let request = json!({"schema":"phaseforge.nr-request.v1","engine_id":ENGINE,"job_id":id,
                "engine_manifest_sha256":nr::hash(&serde_json::to_vec(&manifest).unwrap()),
                "input":{"path":"input.athinput","sha256":"a".repeat(64)},"output_dir":"/root/synthetic/work",
                "cpu_threads":2,"memory_limit_bytes":256*MIB,"output_limit_bytes":128*1024,"deadline_at":null});
            let staged = serde_json::to_vec(&request).unwrap();
            fs::write(directory.join("nr-staged-request.json"), &staged).unwrap();
            let staging = json!({"schema":"phaseforge.nr-staging.v1","job_id":id,"executed":false,
                "input_sha256":request["input"]["sha256"],"request_sha256":nr::hash(&staged)});
            let mut receipt = request.clone();
            let mut files = Vec::new();
            let mut total = 0;
            for (path, bytes) in [("fields/run.bin", vec![0, 1, 127, 255]), ("history/table.csv", b"t,metric\n0,0.5\n".to_vec())] {
                fs::write(work.join(path), &bytes).unwrap();
                total += bytes.len();
                files.push(json!({"path":path,"bytes":bytes.len(),"sha256":nr::hash(&bytes)}));
            }
            for (key, value) in [("schema",json!("phaseforge.nr-process-receipt.v1")),("request_sha256",staging["request_sha256"].clone()),
                ("executable",manifest["executable"].clone()),("source",manifest["source"].clone()),("build",manifest["build"].clone()),
                ("process_group_drained",json!(true)),("termination_reason",json!("completed")),("exit_code",json!(0)),
                ("files",json!(files)),("retained_bytes",json!(total))] { receipt[key] = value; }
            let value = Self {_temporary:temporary,root,directory,id,manifest,request,staging,receipt};
            value.save_receipt();
            value
        }
        fn remote_job(&self) -> PathBuf { self.root.join("jobs").join(self.id.to_string()) }
        fn save_receipt(&self) { fs::write(self.remote_job().join("receipt.json"), serde_json::to_vec(&self.receipt).unwrap()).unwrap(); }
        fn retain(&self) -> anyhow::Result<Value> { retain_from_root(&self.root,self.id,&self.directory,&self.manifest,&self.request,&self.staging,128*1024) }
        fn repin(&mut self, selected_engine: &str, cap: u64) {
            self.manifest["engine_id"] = json!(selected_engine);
            self.manifest["source"]["athenak"] = json!("c5a0d7f9155a70149931bf0be5a4ffb673f2532a");
            self.request["engine_id"] = json!(selected_engine);
            self.request["output_limit_bytes"] = json!(cap);
            self.request["engine_manifest_sha256"] = json!(nr::hash(&serde_json::to_vec(&self.manifest).unwrap()));
            let raw = serde_json::to_vec(&self.request).unwrap();
            fs::write(self.directory.join("nr-staged-request.json"), &raw).unwrap();
            self.staging["request_sha256"] = json!(nr::hash(&raw));
            self.staging["input_sha256"] = self.request["input"]["sha256"].clone();
            for key in ["engine_id","engine_manifest_sha256","output_limit_bytes","input","deadline_at"] { self.receipt[key] = self.request[key].clone(); }
            self.receipt["request_sha256"] = self.staging["request_sha256"].clone();
            self.receipt["source"] = self.manifest["source"].clone();
            self.save_receipt();
        }
    }

    #[test]
    fn large_native_restart_import_is_streamed_and_published_idempotently() {
        let mut fixture = Fixture::new();
        fixture.repin(nr_engines::TWO_PUNCTURES_CPU,1024*MIB);
        let path = fixture.remote_job().join("work/restart.rst");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"Synthetic restart boundary fixture; not scientific data\n").unwrap();
        file.set_len(65*MIB+17).unwrap(); file.sync_all().unwrap(); drop(file);
        let mut digest = Sha256::new(); let mut input = File::open(&path).unwrap(); let mut buffer=[0u8;64*1024];
        loop { let n=input.read(&mut buffer).unwrap(); if n==0 {break;} digest.update(&buffer[..n]); }
        let count = fs::metadata(&path).unwrap().len(); let pin = format!("{:x}",digest.finalize());
        fixture.receipt["files"].as_array_mut().unwrap().push(json!({"path":"restart.rst","bytes":count,"sha256":pin}));
        fixture.receipt["retained_bytes"] = json!(fixture.receipt["retained_bytes"].as_u64().unwrap()+count); fixture.save_receipt();
        let result = retain_from_root(&fixture.root,fixture.id,&fixture.directory,&fixture.manifest,&fixture.request,&fixture.staging,1024*MIB).unwrap();
        let destination = fixture.directory.join("native/restart.rst");
        let modified = fs::metadata(&destination).unwrap().modified().unwrap();
        stream_pinned(&destination,count,&pin,None).unwrap();
        assert_eq!(result["engine"],nr_engines::TWO_PUNCTURES_CPU); assert_eq!(result["merger_validated"],false);
        assert_eq!(retain_from_root(&fixture.root,fixture.id,&fixture.directory,&fixture.manifest,&fixture.request,&fixture.staging,1024*MIB).unwrap(),result);
        assert_eq!(fs::metadata(destination).unwrap().modified().unwrap(),modified);
        assert!(fs::read_dir(fixture.directory.join("native")).unwrap().all(|row|!row.unwrap().file_name().to_string_lossy().starts_with(".nr-import-")));
    }

    #[test]
    fn engine_caps_and_single_file_limit_do_not_expand_gauge_admission() {
        for (selected_engine,cap,bytes) in [(ENGINE,64*MIB+1,4),(nr_engines::TWO_PUNCTURES_CPU,1024*MIB,256*MIB+1),
            (nr_engines::TWO_PUNCTURES_CPU,1024*MIB+1,4)] {
            let mut fixture = Fixture::new(); fixture.repin(selected_engine,cap);
            fixture.receipt["files"][0]["bytes"] = json!(bytes); fixture.save_receipt();
            assert!(retain_from_root(&fixture.root,fixture.id,&fixture.directory,&fixture.manifest,&fixture.request,&fixture.staging,cap).is_err());
            assert!(!fixture.directory.join("native").exists());
        }
        let mut fixture = Fixture::new();fixture.repin(nr_engines::TWO_PUNCTURES_CPU,1024*MIB);
        fixture.manifest["engine_id"] = json!(ENGINE);
        assert!(fixture.retain().is_err(),"A different registered engine must not be treated as equivalent");
    }

    #[test]
    fn failed_stream_pin_never_publishes_partial_final_file() {
        let fixture = Fixture::new();
        let source=fixture.remote_job().join("work/fields/run.bin");
        let destination=fixture.directory.join("stream/result.bin");
        assert!(immutable_stream(&source,&destination,4,&"f".repeat(64)).is_err());
        assert!(!destination.exists());
        assert!(fs::read_dir(destination.parent().unwrap()).unwrap().next().is_none());
    }

    #[test]
    fn exact_native_bytes_and_terminal_receipt_import_idempotently_without_runtime_request() {
        let fixture = Fixture::new();
        assert!(!fixture.remote_job().join("request.json").exists());
        let result = fixture.retain().unwrap();
        assert_eq!(result["execution_completed"],true);
        assert_eq!(result["merger_validated"],false);
        assert_eq!(result["numerical_validity"],"not_assessed_by_process_receipt");
        for row in fixture.receipt["files"].as_array().unwrap() {
            let path = row["path"].as_str().unwrap();
            assert_eq!(fs::read(fixture.directory.join("native").join(path)).unwrap(),fs::read(fixture.remote_job().join("work").join(path)).unwrap());
        }
        let raw = fs::read(fixture.directory.join("nr-process-receipt.json")).unwrap();
        assert_eq!(raw,fs::read(fixture.remote_job().join("receipt.json")).unwrap());
        assert_eq!(result["process_receipt_sha256"],nr::hash(&raw));
        let before = fs::metadata(fixture.directory.join("result.json")).unwrap().modified().unwrap();
        assert_eq!(fixture.retain().unwrap(),result);
        assert_eq!(fs::metadata(fixture.directory.join("result.json")).unwrap().modified().unwrap(),before);
    }

    #[test]
    fn stopped_or_nonzero_attempts_keep_exact_partial_data_without_certifying_completion() {
        for (reason,code) in [("pause",0),("deadline",-15),("parent_lost",-9),("completed",2)] {
            let mut fixture = Fixture::new();
            fixture.receipt["termination_reason"] = json!(reason);
            fixture.receipt["exit_code"] = json!(code);
            fixture.save_receipt();
            let result = fixture.retain().unwrap();
            assert_eq!(result["execution_completed"],false);
            assert_eq!(result["termination_reason"],reason);
            assert!(fixture.directory.join("native/history/table.csv").is_file());
        }
    }

    #[test]
    fn wrong_receipt_budget_input_executable_or_undrained_process_never_imports_outputs() {
        for (key,value) in [("job_id",json!(Uuid::new_v4())),("request_sha256",json!("b".repeat(64))),
            ("cpu_threads",json!(8)),("deadline_at",json!("2026-09-13T00:00:00Z")),
            ("memory_limit_bytes",json!(512*MIB)),("output_limit_bytes",json!(256*1024)),
            ("input",json!({"path":"input.athinput","sha256":"b".repeat(64)})),
            ("executable",json!({"path":"/root/other","sha256":"e".repeat(64)})),("process_group_drained",json!(false))] {
            let mut fixture = Fixture::new(); fixture.receipt[key] = value; fixture.save_receipt();
            assert!(fixture.retain().is_err(),"{key}");
            assert!(!fixture.directory.join("native").exists(),"{key}");
            assert!(!fixture.directory.join("result.json").exists(),"{key}");
        }
    }

    #[test]
    fn changed_staged_request_and_changed_retained_output_are_not_silently_replaced() {
        let fixture = Fixture::new();
        fs::write(fixture.directory.join("nr-staged-request.json"),b"{}").unwrap();
        assert!(fixture.retain().is_err());
        assert!(!fixture.directory.join("nr-process-receipt.json").exists());
        let fixture = Fixture::new();
        fs::write(fixture.remote_job().join("work/fields/run.bin"),[7,7,7,7]).unwrap();
        assert!(fixture.retain().is_err());
        assert!(fixture.directory.join("nr-process-receipt.json").is_file());
        assert!(!fixture.directory.join("result.json").exists());
    }

    #[test]
    fn prior_conflicting_local_file_is_preserved_and_missing_files_can_be_retried() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.directory.join("native/fields")).unwrap();
        let path = fixture.directory.join("native/fields/run.bin");
        fs::write(&path,b"prior user data").unwrap();
        assert!(fixture.retain().is_err());
        assert_eq!(fs::read(path).unwrap(),b"prior user data");
        let fixture = Fixture::new();
        let path = fixture.remote_job().join("work/history/table.csv");
        let saved = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(fixture.retain().is_err());
        let first = fs::read(fixture.directory.join("native/fields/run.bin")).unwrap();
        fs::write(path,saved).unwrap();
        fixture.retain().unwrap();
        assert_eq!(fs::read(fixture.directory.join("native/fields/run.bin")).unwrap(),first);
    }

    #[test]
    fn malformed_duplicate_case_and_windows_alias_inventory_fail_without_native_writes() {
        for path in ["../outside","fields\\run.bin","AUX.txt","fields/run.bin.","fields//run.bin","fields/run.bin:stream"] {
            let mut fixture = Fixture::new(); fixture.receipt["files"][0]["path"] = json!(path); fixture.save_receipt();
            assert!(fixture.retain().is_err(),"{path}");
            assert!(!fixture.directory.join("native").exists());
        }
        let mut fixture = Fixture::new();
        fixture.receipt["files"][1]["path"] = json!("FIELDS/RUN.BIN"); fixture.save_receipt();
        assert!(fixture.retain().is_err());
        assert!(!fixture.directory.join("native").exists());
    }

    #[test]
    fn over_budget_inventory_retains_terminal_facts_but_does_not_copy_or_complete() {
        let mut fixture = Fixture::new();
        fixture.receipt["files"][0]["bytes"] = json!(128*1024+1);
        fixture.save_receipt();
        assert!(fixture.retain().is_err());
        assert!(fixture.directory.join("nr-process-receipt.json").is_file());
        assert!(!fixture.directory.join("native").exists());
        assert!(!fixture.directory.join("result.json").exists());
    }

    #[test]
    fn hard_linked_source_and_destination_are_rejected() {
        let fixture = Fixture::new();
        fs::hard_link(fixture.remote_job().join("work/fields/run.bin"),fixture.root.join("hardlink.bin")).unwrap();
        assert!(fixture.retain().is_err());
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.directory.join("native/fields")).unwrap();
        fs::hard_link(fixture.remote_job().join("work/fields/run.bin"),fixture.directory.join("native/fields/run.bin")).unwrap();
        assert!(fixture.retain().is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_long_absolute_paths_keep_exact_retained_reads_and_streaming() {
        use std::os::windows::ffi::OsStrExt;
        let temporary = tempfile::tempdir().unwrap();
        let parent = temporary.path().join("long retained directory ".repeat(4).trim_end()).join("nested native output ".repeat(4).trim_end()).join("final retained component ".repeat(4).trim_end());
        fs::create_dir_all(&parent).unwrap();
        let source = parent.join("source.bin");
        let bytes = b"Synthetic long-path bytes, not scientific data";
        fs::write(&source,bytes).unwrap();
        assert!(source.as_os_str().encode_wide().count() > 260,"Fixture must exceed MAX_PATH");
        assert_eq!(bounded(&source,1024).unwrap(),bytes);
        let destination = parent.join("retained.bin");
        immutable_stream(&source,&destination,bytes.len() as u64,&nr::hash(bytes)).unwrap();
        assert_eq!(bounded(&destination,1024).unwrap(),bytes);
        eprintln!("EXERCISED actual Windows path beyond MAX_PATH with exact retained read and immutable streaming");
    }

    #[cfg(windows)]
    #[test]
    fn windows_short_alias_reads_and_streams_exact_bytes_without_allowing_other_handles_or_links() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
        fn short_path(path: &Path) -> PathBuf {
            let input = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
            let mut output = vec![0u16; 32768];
            let length = unsafe { GetShortPathNameW(input.as_ptr(),output.as_mut_ptr(),output.len() as u32) };
            assert!(length > 0 && (length as usize) < output.len(),"Cannot query fixture short spelling: {}",std::io::Error::last_os_error());
            PathBuf::from(String::from_utf16(&output[..length as usize]).unwrap())
        }
        let temporary = tempfile::Builder::new().prefix("phaseforge retained alias ").tempdir().unwrap();
        let source = temporary.path().join("native restart long name.bin");
        let bytes = b"Synthetic immutable alias fixture, not scientific data";
        fs::write(&source,bytes).unwrap();
        let alias = short_path(&source);
        if alias.to_string_lossy().eq_ignore_ascii_case(&source.to_string_lossy()) {
            eprintln!("SKIP actual Windows 8.3 alias coverage: this fixture filesystem does not supply a short spelling");
            return;
        }
        assert_eq!(bounded(&alias,1024).unwrap(),bytes);
        let destination = short_path(temporary.path()).join("retained output").join("native.bin");
        immutable_stream(&alias,&destination,bytes.len() as u64,&nr::hash(bytes)).unwrap();
        assert_eq!(bounded(&destination,1024).unwrap(),bytes);
        immutable_stream(&alias,&destination,bytes.len() as u64,&nr::hash(bytes)).unwrap();
        let different = temporary.path().join("different file.bin");
        fs::write(&different,bytes).unwrap();
        assert!(verify_handle(&File::open(&alias).unwrap(),&different,false).is_err(),"An unrelated handle must not become equivalent through long-name expansion");
        fs::hard_link(&source,temporary.path().join("extra source link.bin")).unwrap();
        assert!(bounded(&alias,1024).is_err(),"A real short alias must not bypass the hardlink guard");
        eprintln!("EXERCISED actual Windows 8.3 directory/file aliases, exact immutable streaming, mismatched-handle and hardlink rejection");
    }

    #[cfg(unix)]
    #[test]
    fn linked_source_base_and_destination_ancestor_are_rejected() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let work = fixture.remote_job().join("work");
        let elsewhere = fixture.root.join("elsewhere");
        fs::rename(&work,&elsewhere).unwrap(); symlink(&elsewhere,&work).unwrap();
        assert!(fixture.retain().is_err());
        let fixture = Fixture::new();
        let elsewhere = fixture.root.join("elsewhere"); fs::create_dir(&elsewhere).unwrap();
        symlink(&elsewhere,fixture.directory.join("native")).unwrap();
        assert!(fixture.retain().is_err());
        assert!(fs::read_dir(elsewhere).unwrap().next().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn windows_junction_source_base_and_destination_ancestor_are_rejected() {
        use std::os::windows::process::CommandExt;
        fn junction(link: &Path, target: &Path) {
            use base64::Engine as _;
            // Fixture-only native junction creation; both paths come from this
            // test's private TempDir. No application process is controlled.
            let command = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/WindowsPowerShell/v1.0/powershell.exe");
            let plain = |path: &Path| {
                let value = path.to_string_lossy();
                value.strip_prefix(r"\\?\").unwrap_or(&value).replace('\'',"''")
            };
            let script = format!("$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null",plain(link),plain(target));
            let encoded = base64::engine::general_purpose::STANDARD.encode(script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>());
            let output = std::process::Command::new(command).args(["-NoProfile","-NonInteractive","-EncodedCommand",&encoded])
                .env_clear().env("SystemRoot",std::env::var_os("SystemRoot").unwrap())
                .creation_flags(0x08000000).output().unwrap();
            assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        }
        let fixture = Fixture::new();
        let work = fixture.remote_job().join("work");
        let elsewhere = fixture.root.join("elsewhere");
        fs::rename(&work,&elsewhere).unwrap(); junction(&work,&elsewhere);
        assert!(fixture.retain().is_err());
        assert!(!fixture.directory.join("native").exists());
        fs::remove_dir(&work).unwrap();
        assert!(elsewhere.join("fields/run.bin").is_file());
        let fixture = Fixture::new();
        let elsewhere = fixture.root.join("elsewhere"); fs::create_dir(&elsewhere).unwrap();
        let link = fixture.directory.join("native"); junction(&link,&elsewhere);
        assert!(fixture.retain().is_err());
        assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
        fs::remove_dir(link).unwrap();
    }

    fn service_fixture(state: &str) -> (tempfile::TempDir,LaboratoryService,LabJob) {
        let directory = tempfile::tempdir().unwrap();
        let config = crate::config::AppConfig {data_directory:directory.path().to_owned(),..Default::default()};
        let database = crate::persistence::Database::open(&config.database_path()).unwrap();
        let project = crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("NR receipt fixture".into()),question:"synthetic retained files only".into()});
        database.put_project(&project).unwrap();
        let service = LaboratoryService::new(database,config).unwrap();
        let job = service.create(Uuid::new_v4(),project.id,None,"solver","synthetic NR attempt",json!({"engine":ENGINE,"parameters":{"nx":16}}),Some(chrono::Utc::now()-chrono::Duration::seconds(30))).unwrap();
        let job = service.update(job.id,|job|job.state=state.into()).unwrap();
        (directory,service,job)
    }

    fn retained_result(completed: bool) -> Value {
        json!({"engine":ENGINE,"execution_completed":completed,"process_receipt_sha256":"c".repeat(64),"termination_reason":if completed{"completed"}else{"deadline"},"exit_code":if completed{0}else{-15},"merger_validated":false})
    }

    async fn recovery_finished(service: &LaboratoryService, id: Uuid) -> LabJob {
        tokio::time::timeout(std::time::Duration::from_secs(3),async {
            loop {
                let job=service.get(id).unwrap();
                if !recovery_pending(&job)&&!service.executing(id) { return job; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("Synthetic recovery worker did not drain")
    }
    fn same_scientific_job(before: &LabJob, after: &LabJob) {
        assert_eq!(before.id,after.id);assert_eq!(before.project_id,after.project_id);assert_eq!(before.parent_id,after.parent_id);
        assert_eq!(before.input,after.input);assert_eq!(before.result,after.result);assert_eq!(before.state,after.state);
        assert_eq!(before.deadline_at,after.deadline_at);assert_eq!(before.completed_at,after.completed_at);
        assert_eq!(before.error,after.error);
    }

    #[tokio::test(flavor="current_thread")]
    async fn background_recovery_acknowledges_before_io_and_coalesces_inflight_duplicates() {
        let (_temporary,service,job)=service_fixture("completed");
        let original=service.reconcile_nr_with(job.id,|_,_|Ok(retained_result(true))).unwrap();
        let bytes=fs::read(service.directory(job.id).join("input.json")).unwrap();
        let (started,ready)=tokio::sync::oneshot::channel();
        let (release,held)=std::sync::mpsc::channel();
        let accepted=service.start_nr_recovery_with(job.id,move|_,_|{
            let _=started.send(());held.recv_timeout(std::time::Duration::from_secs(2))?;Ok(retained_result(true))
        }).unwrap();
        assert_eq!(recovery_summary(&accepted)["phase"],"queued");
        assert!(service.executing(job.id));same_scientific_job(&original,&accepted);
        ready.await.unwrap();
        // A second HTTP-style admission cannot invoke another reader while the
        // first blocking reader is still held, even though job.state=completed.
        let duplicate=service.start_nr_recovery_with(job.id,|_,_|panic!("Duplicate recovery reader ran")).unwrap();
        assert_eq!(recovery_summary(&duplicate)["operation_id"],recovery_summary(&accepted)["operation_id"]);
        tokio::time::timeout(std::time::Duration::from_millis(250),tokio::time::sleep(std::time::Duration::from_millis(15))).await.unwrap();
        release.send(()).unwrap();
        let finished=recovery_finished(&service,job.id).await;
        assert_eq!(recovery_summary(&finished)["status"],"completed");same_scientific_job(&original,&finished);
        assert_eq!(recovery_summary(&finished)["started_at"],recovery_summary(&accepted)["started_at"]);
        assert_eq!(finished.events.iter().filter(|event|event.kind=="nr_retained").count(),1);
        assert_eq!(fs::read(service.directory(job.id).join("input.json")).unwrap(),bytes);
        assert_eq!(finished.summary()["recovery"],recovery_summary(&finished));
    }

    #[tokio::test(flavor="current_thread")]
    async fn new_explicit_recovery_after_completion_reverifies_and_records_tamper_failure() {
        let (_temporary,service,job)=service_fixture("completed");
        let original=service.reconcile_nr_with(job.id,|_,_|Ok(retained_result(true))).unwrap();
        let artifact=service.directory(job.id).join("synthetic-pinned-artifact.json");
        let bytes=b"{\"synthetic\":true,\"measurement\":1}";fs::write(&artifact,bytes).unwrap();
        let expected=nr::hash(bytes);
        let reader=move|_:&LabJob,directory:&Path| {
            ensure!(nr::hash(&bounded(&directory.join("synthetic-pinned-artifact.json"),1024)?)==expected,"Synthetic changed-artifact hash: exact saved bytes differ");
            Ok(retained_result(true))
        };
        service.start_nr_recovery_with(job.id,reader.clone()).unwrap();
        let first=recovery_finished(&service,job.id).await;
        fs::write(&artifact,b"{\"synthetic\":true,\"measurement\":2}").unwrap();
        service.start_nr_recovery_with(job.id,reader).unwrap();
        let second=recovery_finished(&service,job.id).await;
        assert_ne!(recovery_summary(&first)["operation_id"],recovery_summary(&second)["operation_id"]);
        assert_eq!(recovery_summary(&second)["status"],"failed");
        assert!(recovery_summary(&second)["error"].as_str().unwrap().contains("changed-artifact hash"));
        same_scientific_job(&original,&second);
        assert_eq!(second.events.iter().filter(|event|event.kind=="nr_retained").count(),1);
    }

    #[tokio::test(flavor="current_thread")]
    async fn queued_recovery_cancel_does_not_read_or_cancel_original_completed_job() {
        let (_temporary,service,job)=service_fixture("completed");
        let original=service.reconcile_nr_with(job.id,|_,_|Ok(retained_result(true))).unwrap();
        let queue=NR_IO.acquire().await.unwrap();
        let accepted=service.start_nr_recovery_with(job.id,|_,_|panic!("Cancelled queued recovery performed I/O")).unwrap();
        let cancelled=service.cancel_nr_recovery(job.id,None).unwrap().unwrap();
        assert_eq!(recovery_summary(&cancelled)["status"],"cancel_requested");same_scientific_job(&original,&cancelled);
        let duplicate=service.start_nr_recovery_with(job.id,|_,_|panic!("Cancelled duplicate reader ran")).unwrap();
        assert_eq!(recovery_summary(&duplicate)["operation_id"],recovery_summary(&accepted)["operation_id"]);
        // Cancellation drains independently of the occupied global I/O queue.
        let final_job=recovery_finished(&service,job.id).await;
        assert_eq!(recovery_summary(&final_job)["status"],"cancelled");same_scientific_job(&original,&final_job);
        drop(queue);
    }

    #[tokio::test(flavor="current_thread")]
    async fn running_recovery_cancel_remains_reserved_until_reader_drains() {
        let (_temporary,service,job)=service_fixture("completed");
        let original=service.reconcile_nr_with(job.id,|_,_|Ok(retained_result(true))).unwrap();
        let (started,ready)=tokio::sync::oneshot::channel();let (release,held)=std::sync::mpsc::channel();
        service.start_nr_recovery_with(job.id,move|_,_|{let _=started.send(());held.recv_timeout(std::time::Duration::from_secs(2))?;Ok(retained_result(true))}).unwrap();
        ready.await.unwrap();
        let requested=service.cancel_nr_recovery(job.id,None).unwrap().unwrap();
        assert!(service.executing(job.id));assert_eq!(recovery_summary(&requested)["status"],"cancel_requested");
        same_scientific_job(&original,&requested);
        release.send(()).unwrap();
        let finished=recovery_finished(&service,job.id).await;
        assert_eq!(recovery_summary(&finished)["status"],"cancelled");same_scientific_job(&original,&finished);
    }

    #[tokio::test(flavor="current_thread")]
    async fn stale_cancel_cannot_stop_a_new_recovery_or_change_a_finished_experiment() {
        let (_temporary,service,job)=service_fixture("completed");
        let original=service.reconcile_nr_with(job.id,|_,_|Ok(retained_result(true))).unwrap();
        service.start_nr_recovery_with(job.id,|_,_|Ok(retained_result(true))).unwrap();
        let first=recovery_finished(&service,job.id).await;
        let old_operation=Uuid::parse_str(recovery_summary(&first)["operation_id"].as_str().unwrap()).unwrap();
        let late=service.cancel_nr_recovery(job.id,Some(old_operation)).unwrap().unwrap();
        assert_eq!(serde_json::to_value(&first).unwrap(),serde_json::to_value(late).unwrap());
        let queue=NR_IO.acquire().await.unwrap();
        let second=service.start_nr_recovery_with(job.id,|_,_|panic!("New queued operation should be cancelled before reading")).unwrap();
        assert!(service.cancel_nr_recovery(job.id,Some(old_operation)).is_err());
        assert!(!service.execution_token(job.id).unwrap().is_cancelled());
        let current_operation=Uuid::parse_str(recovery_summary(&second)["operation_id"].as_str().unwrap()).unwrap();
        service.cancel_nr_recovery(job.id,Some(current_operation)).unwrap();
        let finished=recovery_finished(&service,job.id).await;
        assert_eq!(recovery_summary(&finished)["status"],"cancelled");same_scientific_job(&original,&finished);drop(queue);
    }

    #[test]
    fn restart_marks_recovery_cancelled_without_reexecution_or_scientific_state_changes() {
        let (_temporary,service,job)=service_fixture("completed");
        let original=service.reconcile_nr_with(job.id,|_,_|Ok(retained_result(true))).unwrap();
        let operation=Uuid::new_v4();
        service.update(job.id,|job|recovery_event(job,operation,"started","verifying","Synthetic interrupted recovery",None,None)).unwrap();
        let restarted=LaboratoryService::new(service.database.clone(),service.config.clone()).unwrap();
        let after=restarted.get(job.id).unwrap();
        same_scientific_job(&original,&after);
        assert_eq!(recovery_summary(&after)["status"],"cancelled");
        assert_eq!(recovery_summary(&after)["reason"],"backend_restart");assert!(!restarted.executing(job.id));
        let again=LaboratoryService::new(service.database.clone(),service.config.clone()).unwrap().get(job.id).unwrap();
        assert_eq!(serde_json::to_value(after).unwrap(),serde_json::to_value(again).unwrap());
    }

    #[tokio::test(flavor="current_thread")]
    async fn recovery_worker_panic_is_durable_failure_and_releases_reservation() {
        let (_temporary,service,job)=service_fixture("paused");
        service.start_nr_recovery_with(job.id,|_,_|panic!("Synthetic worker panic; no scientific execution")).unwrap();
        let finished=recovery_finished(&service,job.id).await;
        assert_eq!(recovery_summary(&finished)["status"],"failed");same_scientific_job(&job,&finished);
        assert!(recovery_summary(&finished)["error"].as_str().unwrap().contains("worker failed"));
    }

    #[test]
    fn recovery_cancellation_removes_only_unpublished_partial_copy_and_resets_thread_context() {
        let fixture=Fixture::new();
        let source=fixture.remote_job().join("work/fields/run.bin");
        let destination=fixture.directory.join("cancelled-copy.bin");
        let bytes=fs::read(&source).unwrap();
        {
            let token=CancellationToken::new();let _context=RecoveryCancellation::enter(token.clone());
            token.cancel();
            assert!(immutable_stream(&source,&destination,bytes.len() as u64,&nr::hash(&bytes)).is_err());
            assert!(!destination.exists());
            assert!(!fs::read_dir(&fixture.directory).unwrap().any(|entry|entry.unwrap().file_name().to_string_lossy().starts_with(".nr-import-")));
        }
        assert_eq!(fs::read(&source).unwrap(),bytes);
        immutable_stream(&source,&destination,bytes.len() as u64,&nr::hash(&bytes)).unwrap();
        assert_eq!(fs::read(destination).unwrap(),bytes,"A reused blocking thread inherited a cancelled operation's context");
    }

    fn diagnostic_fixture(service: &LaboratoryService, id: Uuid, passed: bool) -> Value {
        let directory = service.directory(id).join("diagnostics");
        fs::create_dir_all(directory.join("sources")).unwrap();
        fs::create_dir_all(directory.join("slices")).unwrap();
        let receipt = serde_json::to_vec(&json!({"schema":"phaseforge.nr-process-receipt.v1","job_id":id,"synthetic_fixture":true})).unwrap();
        let mut result = retained_result(true);
        result["process_receipt_sha256"] = json!(nr::hash(&receipt));
        let analytic = json!({"schema":"phaseforge.nr-gauge-analytic.v1","passed":passed,"status":if passed{"passed"}else{"failed"},"fixture_only":true});
        let mut artifacts = Vec::new();
        for (name,raw) in [("analytic.json",serde_json::to_vec(&analytic).unwrap()),("sources/execution-receipt.json",receipt),
            ("slices/index.json",b"{\"fixture_only\":true}".to_vec()),("measurements.json",b"{\"fixture_only\":true}".to_vec())] {
            fs::write(directory.join(name),&raw).unwrap();
            artifacts.push(json!({"path":name,"bytes":raw.len(),"sha256":nr::hash(&raw)}));
        }
        let diagnostic = json!({"schema":"phaseforge.nr-gauge-result.v1","engine":ENGINE,
            "scientific_scope":"Synthetic retention test; no numerical benchmark was run",
            "execution":{"receipt_type":"phaseforge.nr-process-receipt.v1","job_id":id,"success":true,"exit_code":0,"termination_reason":"completed","process_group_drained":true},
            "analytic":{"status":analytic["status"],"passed":passed,"report":artifacts[0]},
            "fulfillment":{"gauge_reference_validated":passed,"black_hole_collision_validated":false,"physical_radiation":false},
            "sources":{"execution_receipt":artifacts[1]},"slice_index":artifacts[2],"measurements":artifacts[3],
            "frame_count":1,"initial_time":0.0,"final_time":0.1,"artifacts":artifacts});
        let raw = serde_json::to_vec(&diagnostic).unwrap();
        fs::write(directory.join("result.json"),&raw).unwrap();
        let summary = json!({"path":"diagnostics/result.json","sha256":nr::hash(&raw),"gauge_reference_validated":passed,
            "analytic":diagnostic["analytic"],"slice_index":diagnostic["slice_index"],"frame_count":diagnostic["frame_count"],
            "initial_time":diagnostic["initial_time"],"final_time":diagnostic["final_time"],"scope":diagnostic["scientific_scope"]});
        let mut saved = result.clone(); saved["diagnostics"] = summary;
        service.update(id,|job|job.result=saved).unwrap();
        result
    }

    fn black_hole_diagnostic_fixture(state: &str, process_completed: bool) -> (tempfile::TempDir,LaboratoryService,LabJob,Value) {
        let (temporary,service,original) = service_fixture(state);
        let job=service.update(original.id,|job|job.input=json!({"engine":nr_engines::TWO_PUNCTURES_CPU,"parameters":{"recipe":"head_on_diagnostic_v1"}})).unwrap();
        let mut fixture=Fixture::with_id(job.id);fixture.directory=service.directory(job.id);
        let input=b"# Synthetic contract fixture; no engine executed\n<time>\ntlim = 1\n";
        fixture.request["input"]["sha256"] = json!(nr::hash(input));
        fixture.request["deadline_at"] = json!(job.deadline_at);
        fixture.repin(nr_engines::TWO_PUNCTURES_CPU,128*1024);
        if !process_completed {
            fixture.receipt["termination_reason"] = json!("deadline");fixture.receipt["exit_code"] = json!(-15);fixture.save_receipt();
        }
        let result=fixture.retain().unwrap();
        for (name,raw) in [("nr-engine.json",serde_json::to_vec(&fixture.manifest).unwrap()),
            ("nr-staging.log",serde_json::to_vec(&fixture.staging).unwrap()),("input.athinput",input.to_vec())] {
            fs::write(fixture.directory.join(name),raw).unwrap();
        }
        let directory=fixture.directory.join("diagnostics");fs::create_dir_all(&directory).unwrap();
        let unknown=json!({"status":"unknown","gamma":null,"speed_over_c":null,"calibration":null});
        let times=if process_completed{json!([0.0,1.0])}else{json!([0.0,0.5])};
        let index=json!({"schema":"phaseforge.nr-amr-frame-inventory.v1","representation":"nr_amr_native","paired_times":times,"interpolation":"none","fixture_only":true});
        let measurements=json!({"schema":"phaseforge.nr-black-hole-measurements.v1","physical_boost":unknown,"fixture_only":true});
        let mut artifacts=Vec::new();
        let mut add=|name:&str,raw:Vec<u8>| {
            let path=directory.join(name);fs::create_dir_all(path.parent().unwrap()).unwrap();fs::write(&path,&raw).unwrap();
            artifacts.push(json!({"path":name,"bytes":raw.len(),"sha256":nr::hash(&raw)}));
        };
        add("sources/execution-receipt.json",fs::read(fixture.remote_job().join("receipt.json")).unwrap());
        add("sources/engine.json",serde_json::to_vec(&fixture.manifest).unwrap());
        add("sources/request.json",serde_json::to_vec(&fixture.request).unwrap());
        add("sources/input.athinput",input.to_vec());
        for row in fixture.receipt["files"].as_array().unwrap() {
            let name=row["path"].as_str().unwrap();add(&format!("native/{name}"),fs::read(fixture.remote_job().join("work").join(name)).unwrap());
        }
        add("frames/index.json",serde_json::to_vec(&index).unwrap());
        add("measurements.json",serde_json::to_vec(&measurements).unwrap());
        let descriptor=|name:&str|artifacts.iter().find(|row|row["path"]==name).unwrap().clone();
        let diagnostic=json!({"schema":"phaseforge.nr-black-hole-result.v1","job_id":job.id,"engine_identity":fixture.manifest,
            "scope":"Synthetic source-pin fixture; no scientific engine executed",
            "execution":{"termination_reason":result["termination_reason"],"exit_code":result["exit_code"],"deadline_at":job.deadline_at,
                "process_group_drained":true,"process_completed":process_completed,"time_target":1.0,
                "last_paired_native_time":if process_completed{1.0}else{0.5},"retained_time_target_met":process_completed,
                "complete":process_completed,"status":if process_completed{"complete_execution"}else{"partial_execution"},"no_final_snapshot_fabricated":true},
            "frame_inventory":descriptor("frames/index.json"),"measurements":descriptor("measurements.json"),
            "paired_times":times,"paired_state_count":2,"physical_boost":unknown,
            "fulfillment":{"requested_0_999c_collision":false,"merger_validated":false,"horizon_properties_validated":false,"convergence_validated":false},
            "sources":{"receipt_sha256":result["process_receipt_sha256"],"manifest_sha256":fixture.request["engine_manifest_sha256"],
                "input_sha256":fixture.request["input"]["sha256"],"request_sha256":fixture.staging["request_sha256"],
                "postprocessor_sha256":"f".repeat(64),"decoder_sha256":"d".repeat(64),"source_commit":fixture.manifest["source"]["athenak"],"cpu_cuda_equivalence_tested":false},
            "artifacts":artifacts});
        let raw=serde_json::to_vec(&diagnostic).unwrap();fs::write(directory.join("result.json"),&raw).unwrap();
        let summary=json!({"path":"diagnostics/result.json","sha256":nr::hash(&raw),"execution":diagnostic["execution"],
            "frame_inventory":diagnostic["frame_inventory"],"measurements":diagnostic["measurements"],"paired_times":diagnostic["paired_times"],
            "paired_state_count":diagnostic["paired_state_count"],"physical_boost":diagnostic["physical_boost"],"fulfillment":diagnostic["fulfillment"],"scope":diagnostic["scope"]});
        let mut saved=result.clone();saved["diagnostics"]=summary;
        let job=service.update(job.id,|job|job.result=saved).unwrap();
        (temporary,service,job,result)
    }

    #[test]
    fn black_hole_execution_and_measurements_never_upgrade_unvalidated_recovery() {
        for state in ["failed","timed_out","paused","completed"] {
            for completed in [false,true] {
                let (_temporary,service,original,execution)=black_hole_diagnostic_fixture(state,completed);
                let first=service.reconcile_nr_with(original.id,|_,_|Ok(execution.clone())).unwrap();
                let repeated=service.reconcile_nr_with(original.id,|_,_|Ok(execution.clone())).unwrap();
                assert_eq!(first.state,state);assert_eq!(first.deadline_at,original.deadline_at);assert_eq!(first.input,original.input);
                assert_eq!(first.result,original.result);assert_eq!(first.completed_at,original.completed_at);
                assert_eq!(serde_json::to_value(first).unwrap(),serde_json::to_value(repeated).unwrap());
                assert!(!service.executing(original.id));
            }
        }
    }

    #[test]
    fn completed_black_hole_readonly_verifier_never_imports_or_mutates_job_or_files() {
        let (_temporary, service, original, _) = black_hole_diagnostic_fixture("completed", true);
        let request: Value = serde_json::from_slice(&fs::read(service.directory(original.id).join("nr-staged-request.json")).unwrap()).unwrap();
        service.event(original.id,"resource_plan","Synthetic retained admission, no engine executed",json!({"request":request})).unwrap();
        let job = service.get(original.id).unwrap();
        fn inventory(directory:&Path)->std::collections::BTreeMap<PathBuf,String> {
            fn visit(root:&Path,path:&Path,rows:&mut std::collections::BTreeMap<PathBuf,String>){
                for entry in fs::read_dir(path).unwrap(){let path=entry.unwrap().path();if path.is_dir(){visit(root,&path,rows)}else{rows.insert(path.strip_prefix(root).unwrap().to_owned(),nr::hash(&fs::read(&path).unwrap()));}}
            }
            let mut rows=std::collections::BTreeMap::new();visit(directory,directory,&mut rows);rows
        }
        let before = inventory(&service.directory(job.id));
        let diagnostic = service.verified_black_hole_diagnostics(&job).unwrap();
        assert_eq!(diagnostic["execution"]["complete"],true);
        assert_eq!(diagnostic["fulfillment"]["merger_validated"],false);
        assert_eq!(inventory(&service.directory(job.id)),before);
        assert_eq!(serde_json::to_value(service.get(job.id).unwrap()).unwrap(),serde_json::to_value(&job).unwrap());
        assert!(!service.executing(job.id));
        let mut stale=job.clone();stale.project_id=Uuid::new_v4();assert!(service.verified_black_hole_diagnostics(&stale).is_err());
        let mut stale=job.clone();stale.result["process_receipt_sha256"]=json!("0".repeat(64));assert!(service.verified_black_hole_diagnostics(&stale).is_err());
    }

    #[test]
    fn readonly_diagnostic_evidence_requires_original_admission_and_completed_state() {
        let (_temporary, service, job, _) = black_hole_diagnostic_fixture("completed",true);
        assert!(service.verified_black_hole_diagnostics(&job).is_err());
        let request:Value=serde_json::from_slice(&fs::read(service.directory(job.id).join("nr-staged-request.json")).unwrap()).unwrap();
        service.event(job.id,"resource_plan","Synthetic admission",json!({"request":request})).unwrap();
        service.update(job.id,|j|j.state="failed".into()).unwrap();
        assert!(service.verified_black_hole_diagnostics(&service.get(job.id).unwrap()).is_err());
    }

    #[test]
    fn retained_monitor_uses_frozen_historical_sources_and_rejects_changed_receipts() {
        let (_temporary,service,original,execution)=black_hole_diagnostic_fixture("completed",true);
        let root=service.directory(original.id);fs::create_dir_all(root.join("nr-monitor-tools")).unwrap();fs::create_dir_all(root.join("nr-monitor")).unwrap();
        let mut source_pins=json!({});
        for name in ["nr_live_guard.py","nr_black_hole_result.py","athenak_decode.py"] {
            let raw=format!("# synthetic historical {name}; never executed\n");fs::write(root.join("nr-monitor-tools").join(name),&raw).unwrap();source_pins[name]=json!(nr::hash(raw.as_bytes()));
        }
        let latest=json!({"ordinal":0,"time":0.0,"cycle":0,"accuracy_validated":false,"finite":true,"positive_physical_spatial_metric":true,
            "growth_guard_passed":true,"metric":execution["native_files"][0],"constraints":execution["native_files"][1]});
        let mut row=latest.clone();row["previous_entry_sha256"]=Value::Null;let mut observations=serde_json::to_vec(&row).unwrap();observations.push(b'\n');
        fs::write(root.join("nr-monitor/observations.jsonl"),&observations).unwrap();
        let thresholds=json!({"expected_blocks":624,"static_mesh":true,"excise_chi":0.0625,"growth_factor":100.0,"initial_rms_floor":1e-10,"poll_interval_seconds":1.0});
        let receipt=json!({"schema":"phaseforge.nr-live-guard.v1","status":"stopped","exit_code":0,"reason":"caller_stopped","observed_pair_count":1,
            "last_time":0.0,"last_cycle":0,"latest_observation":latest,"accuracy_validated":false,"guard_thresholds":thresholds,"source_sha256":source_pins,
            "observations":{"path":"observations.jsonl","sha256":nr::hash(&observations),"bytes":observations.len(),"last_entry_sha256":nr::hash(&observations)}});
        let raw=serde_json::to_vec(&receipt).unwrap();fs::write(root.join("nr-monitor/receipt.json"),&raw).unwrap();
        let summary=json!({"path":"nr-monitor/receipt.json","sha256":nr::hash(&raw),"passed":true,"status":"stopped","reason":"caller_stopped",
            "observed_pair_count":1,"last_time":0.0,"last_cycle":0,"latest_observation":latest,"guard_thresholds":thresholds,"accuracy_validated":false});
        let job=service.update(original.id,|job|job.result["live_monitor"]=summary.clone()).unwrap();
        let recovered=service.reconcile_nr_with(job.id,|_,_|Ok(execution.clone())).unwrap();assert_eq!(recovered.result,job.result);
        let mut result=execution.clone();merge_live_monitor(&job,&root,&mut result).unwrap();assert_eq!(result["live_monitor"],summary);
        fs::write(root.join("nr-monitor-tools/nr_black_hole_result.py"),b"changed historical source").unwrap();
        assert!(merge_live_monitor(&job,&root,&mut execution.clone()).is_err());
        fs::write(root.join("nr-monitor-tools/nr_black_hole_result.py"),b"# synthetic historical nr_black_hole_result.py; never executed\n").unwrap();
        fs::write(root.join("nr-monitor/observations.jsonl"),b"changed observations\n").unwrap();
        assert!(merge_live_monitor(&job,&root,&mut execution.clone()).is_err());
        assert_eq!(service.get(job.id).unwrap().result,job.result);
    }

    #[test]
    fn black_hole_changed_native_measurements_or_source_identity_cannot_replace_saved_result() {
        for name in ["native/fields/run.bin","measurements.json","sources/execution-receipt.json","sources/engine.json","sources/request.json","sources/input.athinput","frames/index.json"] {
            let (_temporary,service,original,execution)=black_hole_diagnostic_fixture("timed_out",false);
            fs::write(service.directory(original.id).join("diagnostics").join(name),b"changed retained fixture").unwrap();
            assert!(service.reconcile_nr_with(original.id,|_,_|Ok(execution.clone())).is_err(),"{name}");
            assert_eq!(service.get(original.id).unwrap().result,original.result);assert!(!service.executing(original.id));
        }
        let (_temporary,service,original,mut execution)=black_hole_diagnostic_fixture("paused",true);
        execution["engine"]=json!(ENGINE);
        assert!(service.reconcile_nr_with(original.id,|_,_|Ok(execution)).is_err());
        assert_eq!(service.get(original.id).unwrap().state,"paused");
    }

    #[test]
    fn rehashed_black_hole_summary_cannot_assert_boost_or_merger_or_other_execution() {
        for (path,value) in [("/fulfillment/merger_validated",json!(true)),("/physical_boost/speed_over_c",json!(0.999)),
            ("/job_id",json!(Uuid::new_v4())),("/sources/request_sha256",json!("0".repeat(64))),
            ("/execution/retained_time_target_met",json!(true))] {
            let (_temporary,service,original,execution)=black_hole_diagnostic_fixture("timed_out",false);
            let path_on_disk=service.directory(original.id).join("diagnostics/result.json");
            let mut diagnostic:Value=serde_json::from_slice(&fs::read(&path_on_disk).unwrap()).unwrap();
            *diagnostic.pointer_mut(path).unwrap()=value;
            let raw=serde_json::to_vec(&diagnostic).unwrap();fs::write(&path_on_disk,&raw).unwrap();
            service.update(original.id,|job|job.result["diagnostics"]["sha256"]=json!(nr::hash(&raw))).unwrap();
            assert!(service.reconcile_nr_with(original.id,|_,_|Ok(execution.clone())).is_err(),"{path}");
            assert_eq!(service.get(original.id).unwrap().state,"timed_out");assert!(!service.executing(original.id));
        }
    }

    #[test]
    fn reconcile_preserves_deadline_and_input_and_appends_one_event_without_execution() {
        for complete in [false,true] {
            let (_directory,service,original) = service_fixture("timed_out");
            let input = fs::read(service.directory(original.id).join("input.json")).unwrap();
            let first = service.reconcile_nr_with(original.id,|snapshot,_| {assert_eq!(snapshot.deadline_at,original.deadline_at);Ok(retained_result(complete))}).unwrap();
            let repeated = service.reconcile_nr_with(original.id,|_,_|Ok(retained_result(complete))).unwrap();
            assert_eq!(first.input,original.input); assert_eq!(first.deadline_at,original.deadline_at);
            assert_eq!(first.state,"timed_out","Process exit alone must not promote numerical fulfillment");
            assert_eq!(serde_json::to_value(&first).unwrap(),serde_json::to_value(&repeated).unwrap());
            assert_eq!(first.events.iter().filter(|event|event.kind=="nr_retained").count(),1);
            assert_eq!(fs::read(service.directory(original.id).join("input.json")).unwrap(),input);
            assert!(!service.executing(original.id));
        }
    }

    #[test]
    fn failed_reconciliation_releases_reservation_and_cannot_touch_active_or_changed_job() {
        let (_directory,service,original) = service_fixture("paused");
        assert!(service.reconcile_nr_with(original.id,|_,_|anyhow::bail!("synthetic unavailable receipt")).is_err());
        assert!(!service.executing(original.id));
        assert_eq!(serde_json::to_value(service.get(original.id).unwrap()).unwrap(),serde_json::to_value(&original).unwrap());
        let token = service.acquire(original.id).unwrap();
        assert!(service.reconcile_nr_with(original.id,|_,_|panic!("must not read an executing job")).is_err());
        assert!(!token.is_cancelled()); service.release(original.id);
        service.update(original.id,|job|job.state="running".into()).unwrap();
        assert!(service.reconcile_nr_with(original.id,|_,_|panic!("must not read an active job")).is_err());
        service.update(original.id,|job|job.state="paused".into()).unwrap();
        assert!(service.reconcile_nr_with(original.id,|_,_| {
            service.update(original.id,|job|job.deadline_at=None)?;
            Ok(retained_result(true))
        }).is_err());
        assert!(!service.executing(original.id));
        assert_eq!(service.get(original.id).unwrap().state,"paused");
        assert!(service.get(original.id).unwrap().result.is_null());
        assert!(service.reconcile_nr_with(original.id,|_,_| {
            service.stop(original.id,"cancelled")?;
            Ok(retained_result(true))
        }).is_err());
        assert!(!service.executing(original.id));
        assert!(service.get(original.id).unwrap().result.is_null());
    }

    #[test]
    fn reconciliation_does_not_retimestamp_an_already_completed_result() {
        let (_directory,service,original) = service_fixture("completed");
        let reconciled = service.reconcile_nr_with(original.id,|_,_|Ok(retained_result(true))).unwrap();
        assert_eq!(reconciled.completed_at,original.completed_at);
        assert_eq!(reconciled.events.iter().filter(|event|event.kind=="completed").count(),original.events.iter().filter(|event|event.kind=="completed").count());
        assert_eq!(reconciled.events.iter().filter(|event|event.kind=="nr_retained").count(),1);
    }

    #[test]
    fn only_verified_analytic_success_can_complete_and_analytic_failure_stays_failed() {
        for passed in [false,true] {
            let (_directory,service,original) = service_fixture("failed");
            let execution = diagnostic_fixture(&service,original.id,passed);
            let diagnostic_before = service.get(original.id).unwrap().result["diagnostics"].clone();
            let first = service.reconcile_nr_with(original.id,|_,_|Ok(execution.clone())).unwrap();
            assert_eq!(first.state,if passed{"completed"}else{"failed"});
            assert_eq!(first.result["diagnostics"],diagnostic_before);
            assert_eq!(first.input,original.input); assert_eq!(first.deadline_at,original.deadline_at);
            let repeated = service.reconcile_nr_with(original.id,|_,_|Ok(execution.clone())).unwrap();
            assert_eq!(serde_json::to_value(&first).unwrap(),serde_json::to_value(repeated).unwrap());
        }
    }

    #[test]
    fn modified_diagnostic_report_source_or_slice_cannot_promote_or_replace_saved_result() {
        for name in ["result.json","analytic.json","sources/execution-receipt.json","slices/index.json"] {
            let (_directory,service,original) = service_fixture("failed");
            let execution = diagnostic_fixture(&service,original.id,true);
            let before = serde_json::to_value(service.get(original.id).unwrap()).unwrap();
            fs::write(service.directory(original.id).join("diagnostics").join(name),b"changed retained bytes").unwrap();
            assert!(service.reconcile_nr_with(original.id,|_,_|Ok(execution.clone())).is_err(),"{name}");
            assert_eq!(serde_json::to_value(service.get(original.id).unwrap()).unwrap(),before,"{name}");
            assert!(!service.executing(original.id));
        }
    }
}
