//! Offline, exact-byte managed Python runtimes. Dependencies are not a sandbox.
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, BTreeSet}, fs::{self, File, OpenOptions}, io::{Read, Write}, path::{Component, Path, PathBuf}};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const OUTER: &str = "phaseforge-runtime-seed.json";
const INNER: &str = "phaseforge-isolation-runtime.json";
const RESERVE_BYTES: u64 = 64 * 1024 * 1024;

pub(super) const SCIENCE_SMOKE: &str = r#"
import sys,json,pathlib,csv,numpy,openmm,PIL
def local_origin(value): return pathlib.Path(str(pathlib.Path(value).resolve()).removeprefix('\\\\?\\'))
root=local_origin(sys.executable).parent
modules={m.__name__:str(local_origin(m.__file__)) for m in (json,pathlib,csv,numpy,openmm,PIL)}
assert sys.version_info[:3]==(3,13,15)
assert (numpy.__version__,openmm.__version__,PIL.__version__)==('2.4.6','8.5.2','12.3.0')
assert all(pathlib.Path(p).is_relative_to(root) for p in modules.values()),modules
assert all(local_origin(p).is_relative_to(root) for p in sys.path),sys.path
assert sys.flags.isolated and sys.flags.dont_write_bytecode and sys.flags.optimize==0
assert numpy.linalg.norm([3.,4.])==5.
system=openmm.System();system.addParticle(39.948)
force=openmm.CustomExternalForce('0.5*k*x*x');force.addGlobalParameter('k',1.0);force.addParticle(0,[]);system.addForce(force)
integrator=openmm.VerletIntegrator(.001)
context=openmm.Context(system,integrator,openmm.Platform.getPlatformByName('Reference'))
context.setPositions([[1.,0.,0.]]);context.setVelocities([[0.,0.,0.]])
before=context.getState(getEnergy=True).getPotentialEnergy()._value
integrator.step(2);state=context.getState(getPositions=True,getEnergy=True)
x=float(state.getPositions(asNumpy=True)._value[0,0])
assert before==.5 and 0.999<x<1.0 and state.getPotentialEnergy()._value<before
print(json.dumps({'python':sys.version,'sys_path':sys.path,'module_origins':modules,'runtime':str(root),'openmm':openmm.__version__,'numpy':numpy.__version__,'pillow':PIL.__version__,'reference_steps':2,'position_x_nm':x,'energy_before_kj_mol':before,'isolated':sys.flags.isolated,'dont_write_bytecode':sys.flags.dont_write_bytecode,'optimize':sys.flags.optimize}))
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RuntimeKind { Science, Generated }
impl RuntimeKind {
    pub(super) fn name(self) -> &'static str { match self { Self::Science => "science-v2", Self::Generated => "python-numpy-v2" } }
    fn bytes(self) -> &'static [u8] { match self {
        Self::Science => include_bytes!("../../../tools/runtime-seeds/science-v2.manifest.json"),
        Self::Generated => include_bytes!("../../../tools/runtime-seeds/python-numpy-v2.manifest.json"),
    } }
    pub(super) fn directory(self, data: &Path) -> PathBuf {
        let root = data.join("environments").join(self.name());
        if self == Self::Generated { root.join("runtime") } else { root }
    }
}

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32, schema: String, kind: String, python: String, numpy: String,
    sources: Vec<serde_json::Value>, files: BTreeMap<String, String>, file_bytes: BTreeMap<String, u64>,
    native_files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct VerifiedRuntime {
    pub directory: PathBuf, pub python: PathBuf, pub kind: &'static str,
    pub manifest_sha256: String, pub file_count: usize, pub content_bytes: u64,
    pub python_version: &'static str, pub numpy_version: &'static str,
    pub source_count: usize,
}

fn sha(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn hash_text(value: &str) -> bool { value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }

fn relative(value: &str) -> anyhow::Result<&Path> {
    ensure!(!value.is_empty() && !value.contains('\\') && !value.contains(':') && value.len() <= 1024, "Invalid runtime inventory path");
    ensure!(value.split('/').all(|p| !p.is_empty() && p != "." && p != ".." && !p.ends_with(['.', ' '])), "Unsafe runtime inventory path");
    for part in value.split('/') {
        let stem=part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(!matches!(stem.as_str(),"CON"|"PRN"|"AUX"|"NUL"|"CONIN$"|"CONOUT$") && !(stem.len()==4 && (stem.starts_with("COM")||stem.starts_with("LPT")) && matches!(stem.as_bytes()[3],b'1'..=b'9')), "Windows runtime device aliases are forbidden");
    }
    let path = Path::new(value);
    ensure!(path.components().all(|part| matches!(part, Component::Normal(_))), "Runtime paths must be relative");
    Ok(path)
}

fn ordinary_metadata(path: &Path) -> anyhow::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).with_context(|| format!("Missing managed runtime member: {}", path.display()))?;
    ensure!(!metadata.file_type().is_symlink() && (metadata.is_dir() || metadata.is_file()), "Runtime links and special files are forbidden: {}", path.display());
    #[cfg(windows)] { use std::os::windows::fs::MetadataExt; ensure!(metadata.file_attributes() & 0x400 == 0, "Runtime reparse points are forbidden: {}", path.display()); }
    Ok(metadata)
}

fn ordinary_chain(path: &Path) -> anyhow::Result<()> {
    ensure!(path.is_absolute(), "Runtime paths must be absolute");
    let mut cursor = PathBuf::new();
    for component in path.components() {
        ensure!(!matches!(component, Component::ParentDir | Component::CurDir), "Runtime paths cannot traverse parent directories");
        cursor.push(component.as_os_str());
        if matches!(component, Component::Normal(_)) { ordinary_metadata(&cursor)?; }
    }
    #[cfg(windows)] {
        ensure!(matches!(path.components().next(),Some(Component::Prefix(prefix)) if matches!(prefix.kind(),std::path::Prefix::Disk(_)|std::path::Prefix::VerbatimDisk(_))), "Only local drive runtime directories are supported; network/device paths are forbidden");
    }
    Ok(())
}

fn open_plain(path: &Path) -> anyhow::Result<File> {
    ensure!(ordinary_metadata(path)?.is_file(), "Runtime member is not a file: {}", path.display());
    let file = File::open(path)?;
    #[cfg(windows)] {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, GetFinalPathNameByHandleW, BY_HANDLE_FILE_INFORMATION};
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        ensure!(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } != 0, "Could not inspect runtime file identity");
        ensure!(information.nNumberOfLinks == 1 && information.dwFileAttributes & 0x400 == 0, "Runtime file hardlinks/reparse points are forbidden");
        let mut final_name = vec![0_u16; 32768];
        let count = unsafe { GetFinalPathNameByHandleW(file.as_raw_handle(), final_name.as_mut_ptr(), final_name.len() as u32, 0) };
        ensure!(count > 0 && (count as usize) < final_name.len(), "Could not inspect final runtime handle path");
        let final_name = String::from_utf16(&final_name[..count as usize])?;
        let normalize = |value: &str| value.strip_prefix(r"\\?\").unwrap_or(value).replace('/', r"\").to_lowercase();
        ensure!(normalize(&final_name) == normalize(&path.to_string_lossy()), "Runtime file handle resolved through an unexpected ancestor or link");
    }
    #[cfg(unix)] { use std::os::unix::fs::MetadataExt; ensure!(file.metadata()?.nlink() == 1, "Runtime hardlinks are forbidden"); }
    Ok(file)
}

fn read_bounded(path: &Path, limit: u64) -> anyhow::Result<Vec<u8>> {
    let mut file = open_plain(path)?;
    ensure!(file.metadata()?.len() <= limit, "Runtime metadata exceeded its bound");
    let mut bytes = Vec::new();
    Read::by_ref(&mut file).take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "Runtime metadata changed while being read");
    Ok(bytes)
}

fn parse(kind: RuntimeKind, bytes: &[u8]) -> anyhow::Result<Manifest> {
    let manifest: Manifest = serde_json::from_slice(bytes)?;
    ensure!(manifest.schema_version == 1 && manifest.schema == "phaseforge.runtime-seed.v1" && manifest.kind == kind.name()
            && manifest.python == "3.13.15" && manifest.numpy == "2.4.6", "Unsupported compiled runtime contract");
    ensure!(!manifest.files.is_empty() && manifest.files.len() <= 10000 && manifest.files.keys().eq(manifest.file_bytes.keys()), "Invalid compiled runtime inventory");
    ensure!(manifest.files.contains_key("python.exe") && manifest.files.contains_key("python313._pth")
            && manifest.files.contains_key("Lib/os.py") && !manifest.files.contains_key(OUTER), "Missing interpreter/source-stdlib contract");
    let expected_sources: &[&str] = if kind == RuntimeKind::Science { &[
        "d1f04d990aee1253d8569e8e5104e30fa9f5fa830899f14843448872d936a2cf", "1e66a7945a48390ee4c2a4268a0e4185884059a13c4aab6d148aa208deea4a76", "c4fc99836233ea196540b17ab0983aff60ed07941751930f5f4d05bc3b3b7359",
        "289c870e28c946b03992215f1f01f0b7d870f84a6f4c0a57e103bd8d62712b46", "1cca606cd25738df4ed873d5ad46bbdb3d83b5cbca291f6b4ff13a4df6b0bbe8"] } else { &[
        "d1f04d990aee1253d8569e8e5104e30fa9f5fa830899f14843448872d936a2cf", "1e66a7945a48390ee4c2a4268a0e4185884059a13c4aab6d148aa208deea4a76", "c4fc99836233ea196540b17ab0983aff60ed07941751930f5f4d05bc3b3b7359"] };
    let source_hashes: BTreeSet<_> = manifest.sources.iter().map(|source| source["sha256"].as_str().unwrap_or("")).collect();
    ensure!(manifest.sources.len() == expected_sources.len() && source_hashes == expected_sources.iter().copied().collect(), "Compiled runtime upstream source pins differ");
    let mut folded = BTreeSet::new();
    for (name, hash) in &manifest.files {
        relative(name)?;
        ensure!(folded.insert(name.to_ascii_lowercase()) && hash_text(hash) && manifest.file_bytes[name] <= 128 * 1024 * 1024, "Invalid or duplicate runtime member");
        ensure!(!name.ends_with(".pyc") && !name.ends_with(".pyo") && !name.contains("/__pycache__/"), "Pinned runtime must retain source modules without bytecode caches");
    }
    let expected_native: BTreeSet<_> = manifest.files.keys().filter(|name| Path::new(name).extension().and_then(|extension| extension.to_str()).is_some_and(|extension| ["dll","pyd","exe"].contains(&extension.to_ascii_lowercase().as_str()))).collect();
    ensure!(expected_native == manifest.native_files.keys().collect(), "Native inventory must include every executable and native library");
    for (name, hash) in &manifest.native_files { ensure!(manifest.files.get(name) == Some(hash), "Native member lacks its full inventory pin"); }
    Ok(manifest)
}

fn verify_inner(root: &Path, outer: &Manifest) -> anyhow::Result<()> {
    let inner: serde_json::Value = serde_json::from_slice(&read_bounded(&root.join(INNER), 2 * 1024 * 1024)?)?;
    ensure!(inner["schema_version"] == 1 && inner["python"] == "3.13.15" && inner["numpy"] == "2.4.6"
            && inner["sources"] == serde_json::json!(outer.sources), "Generated runtime inner source/version receipt differs");
    let files = inner["files"].as_object().context("Inner runtime inventory missing")?;
    ensure!(files.len() + 1 == outer.files.len(), "Inner/outer runtime inventory count differs");
    for (name, hash) in &outer.files {
        if name == INNER { continue; }
        ensure!(files.get(name).and_then(|value| value.as_str()) == Some(hash), "Inner/outer runtime content pin differs: {name}");
    }
    ensure!(!files.contains_key(INNER) && !files.contains_key(OUTER), "Runtime manifests cannot circularly hash themselves");
    Ok(())
}

fn verify_contract(root: &Path, kind: RuntimeKind, compiled: &[u8], token: &CancellationToken) -> anyhow::Result<VerifiedRuntime> {
    ordinary_chain(root)?;
    ensure!(read_bounded(&root.join(OUTER), 2 * 1024 * 1024)? == compiled, "Bundled runtime manifest differs from this application's compiled contract; restore the matching package, no host/network fallback is allowed");
    let manifest = parse(kind, compiled)?;
    let expected_dirs: BTreeSet<PathBuf> = manifest.files.keys().flat_map(|name| Path::new(name).ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf)).collect();
    let mut observed = BTreeSet::new();
    fn walk(root: &Path, directory: &Path, manifest: &Manifest, dirs: &BTreeSet<PathBuf>, observed: &mut BTreeSet<String>, token: &CancellationToken) -> anyhow::Result<()> {
        for entry in fs::read_dir(directory)? {
            ensure!(!token.is_cancelled(), "Runtime verification cancelled");
            let path = entry?.path();
            let metadata = ordinary_metadata(&path)?;
            let rel = path.strip_prefix(root)?;
            if metadata.is_dir() {
                ensure!(dirs.contains(rel), "Unexpected runtime directory: {}", rel.display());
                walk(root, &path, manifest, dirs, observed, token)?;
            } else {
                let name = rel.to_str().context("Non-UTF8 runtime member")?.replace('\\', "/");
                if name == OUTER { continue; }
                let expected = manifest.files.get(&name).context("Unexpected unpinned runtime file")?;
                ensure!(metadata.len() == manifest.file_bytes[&name], "Runtime member size differs: {name}");
                let mut file = open_plain(&path)?;
                let mut digest = Sha256::new();
                let mut total = 0_u64;
                let mut buffer = [0_u8; 65536];
                loop {
                    ensure!(!token.is_cancelled(), "Runtime verification cancelled");
                    let count = file.read(&mut buffer)?;
                    if count == 0 { break; }
                    total += count as u64;
                    ensure!(total <= manifest.file_bytes[&name], "Runtime file grew while verifying");
                    digest.update(&buffer[..count]);
                }
                ensure!(total == manifest.file_bytes[&name] && format!("{:x}", digest.finalize()) == *expected, "Runtime file hash differs: {name}");
                observed.insert(name);
            }
        }
        Ok(())
    }
    walk(root, root, &manifest, &expected_dirs, &mut observed, token)?;
    ensure!(observed.len() == manifest.files.len(), "A pinned runtime file is missing");
    if kind == RuntimeKind::Generated { verify_inner(root, &manifest)?; }
    Ok(VerifiedRuntime { directory: root.to_path_buf(), python: root.join("python.exe"), kind: kind.name(),
        manifest_sha256: sha(compiled), file_count: observed.len(), content_bytes: manifest.file_bytes.values().sum(),
        python_version: "3.13.15", numpy_version: "2.4.6", source_count: manifest.sources.len() })
}

pub(super) fn verify(root: &Path, kind: RuntimeKind, token: &CancellationToken) -> anyhow::Result<VerifiedRuntime> {
    verify_contract(root, kind, kind.bytes(), token)
}

fn source_root() -> anyhow::Result<PathBuf> {
    if cfg!(debug_assertions) {
        if let Some(path) = std::env::var_os("PHASEFORGE_RUNTIME_SEED_ROOT") {
            let path = PathBuf::from(path);
            ensure!(path.is_absolute(), "PHASEFORGE_RUNTIME_SEED_ROOT must be absolute");
            return Ok(path);
        }
    }
    let exe = std::env::current_exe()?;
    Ok(exe.parent().context("Executable directory missing")?.join("runtime-seeds"))
}

fn available_bytes(path: &Path) -> anyhow::Result<u64> {
    #[cfg(windows)] {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0;
        ensure!(unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, std::ptr::null_mut(), std::ptr::null_mut()) } != 0, "Cannot determine runtime storage availability");
        return Ok(available);
    }
    #[cfg(not(windows))] { let _ = path; anyhow::bail!("Bundled v2 Python runtimes require Windows") }
}

fn provision_from(source: &Path, destination: &Path, kind: RuntimeKind, token: &CancellationToken) -> anyhow::Result<VerifiedRuntime> {
    if destination.try_exists()? { return verify(destination, kind, token); }
    let verified = verify(source, kind, token).with_context(|| format!("Missing or damaged bundled {} runtime; reinstall matching application resources. No host Python or network fallback is permitted", kind.name()))?;
    let manifest = parse(kind, kind.bytes())?;
    let parent = destination.parent().context("Managed runtime parent missing")?;
    create_ordinary_directories(parent)?;
    ensure!(available_bytes(parent)? >= verified.content_bytes + RESERVE_BYTES, "Insufficient free space for the verified runtime plus its declared 64MiB reserve");
    let stage = parent.join(format!(".{}-pending-{}", kind.name(), Uuid::new_v4()));
    fs::create_dir(&stage)?;
    // Failed/cancelled staging remains separate for diagnosis. It is never selected as a runtime.
    for (name, expected) in &manifest.files {
        ensure!(!token.is_cancelled(), "Runtime copying cancelled; partial staging retained");
        let relative = relative(name)?;
        let target = stage.join(relative);
        if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
        let mut source_file = open_plain(&source.join(relative))?;
        let mut target_file = OpenOptions::new().write(true).create_new(true).open(&target)?;
        let mut digest = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; 65536];
        loop {
            ensure!(!token.is_cancelled(), "Runtime copying cancelled; partial staging retained");
            let count = source_file.read(&mut buffer)?;
            if count == 0 { break; }
            total += count as u64;
            ensure!(total <= manifest.file_bytes[name], "Runtime source grew during copy");
            digest.update(&buffer[..count]);
            target_file.write_all(&buffer[..count])?;
        }
        target_file.sync_all()?;
        ensure!(total == manifest.file_bytes[name] && format!("{:x}", digest.finalize()) == *expected, "Runtime source changed during copy: {name}");
    }
    let mut outer = OpenOptions::new().write(true).create_new(true).open(stage.join(OUTER))?;
    outer.write_all(kind.bytes())?;
    outer.sync_all()?;
    drop(outer);
    verify(&stage, kind, token)?;
    verify(source, kind, token)?;
    ensure!(!token.is_cancelled(), "Runtime publication cancelled; staging retained");
    match fs::rename(&stage, destination) {
        Ok(()) => verify(destination, kind, token),
        Err(error) if destination.try_exists()? => verify(destination, kind, token).with_context(|| format!("Concurrent runtime publication conflict: {error}")),
        Err(error) => Err(error).context("Could not atomically publish verified runtime; staging preserved"),
    }
}

fn create_ordinary_directories(path: &Path) -> anyhow::Result<()> {
    ensure!(path.is_absolute(), "Managed runtime destination must be absolute");
    if path.try_exists()? { ordinary_chain(path)?; ensure!(ordinary_metadata(path)?.is_dir(), "Managed runtime parent is not a directory"); return Ok(()); }
    let parent = path.parent().context("Managed runtime destination has no existing ancestor")?;
    create_ordinary_directories(parent)?;
    match fs::create_dir(path) { Ok(()) => (), Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (), Err(error) => return Err(error.into()) }
    ordinary_chain(path)?;
    ensure!(ordinary_metadata(path)?.is_dir(), "Managed runtime parent changed to a file");
    Ok(())
}

pub(super) fn provision(data: &Path, kind: RuntimeKind, token: &CancellationToken) -> anyhow::Result<VerifiedRuntime> {
    ensure!(cfg!(windows), "Bundled v2 runtimes require Windows; no unrestricted fallback is permitted");
    provision_from(&source_root()?.join(kind.name()), &kind.directory(data), kind, token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn fixture(root: &Path) -> Vec<u8> {
        let mut contract: Value = serde_json::from_slice(RuntimeKind::Science.bytes()).unwrap();
        let mut files = serde_json::Map::new(); let mut sizes = serde_json::Map::new();
        for (name, data) in [("python.exe", b"fixture interpreter".as_slice()), ("python313._pth", b".\nLib\n".as_slice()), ("Lib/os.py", b"# fixture source\n".as_slice())] {
            let path = root.join(name); fs::create_dir_all(path.parent().unwrap()).unwrap(); fs::write(path, data).unwrap();
            files.insert(name.into(), json!(sha(data))); sizes.insert(name.into(), json!(data.len()));
        }
        contract["files"] = json!(files); contract["file_bytes"] = json!(sizes); contract["native_files"] = json!({"python.exe":files["python.exe"]});
        let bytes = serde_json::to_vec_pretty(&contract).unwrap(); fs::write(root.join(OUTER), &bytes).unwrap(); bytes
    }
    #[test]
    fn frozen_runtime_contract_refuses_missing_extra_modified_and_linked_members() {
        let dir = tempfile::tempdir().unwrap();
        // A newly owned fixture may inherit an 8.3 spelling from Windows TEMP.
        // Keep inspected input paths canonical without relaxing the handle guard.
        let parent = fs::canonicalize(dir.path()).unwrap();
        let root = &parent.join("runtime"); fs::create_dir(root).unwrap(); let bytes = fixture(root); let token = CancellationToken::new();
        assert_eq!(verify_contract(root, RuntimeKind::Science, &bytes, &token).unwrap().file_count, 3);
        fs::write(root.join("surprise.py"), b"untracked").unwrap(); assert!(verify_contract(root, RuntimeKind::Science, &bytes, &token).is_err()); fs::remove_file(root.join("surprise.py")).unwrap();
        fs::write(root.join("Lib/os.py"), b"# fixture SOURCe\n").unwrap(); assert!(verify_contract(root, RuntimeKind::Science, &bytes, &token).is_err());
        let bytes = fixture(root); fs::remove_file(root.join("python.exe")).unwrap(); assert!(verify_contract(root, RuntimeKind::Science, &bytes, &token).is_err());
        let bytes = fixture(root); fs::write(root.join(OUTER), b"{}").unwrap(); assert!(verify_contract(root, RuntimeKind::Science, &bytes, &token).is_err());
        let bytes = fixture(root); fs::hard_link(root.join("Lib/os.py"), dir.path().join("outside-alias")).unwrap(); assert!(verify_contract(root, RuntimeKind::Science, &bytes, &token).is_err());
        fs::remove_file(dir.path().join("outside-alias")).unwrap(); let stopped = CancellationToken::new(); stopped.cancel(); assert!(verify_contract(root, RuntimeKind::Science, &bytes, &stopped).is_err());
    }
    #[test]
    fn compiled_source_pins_and_paths_are_strict() {
        for kind in [RuntimeKind::Science, RuntimeKind::Generated] { parse(kind, kind.bytes()).unwrap(); }
        for bad in ["../escape", "a\\b", "C:/outside", "file:stream", "a/./b", "a//b", "a./b", "a /b", "NUL.txt", "a/COM1"] { assert!(relative(bad).is_err(), "{bad}"); }
        let mut altered: Value = serde_json::from_slice(RuntimeKind::Science.bytes()).unwrap(); altered["sources"][0]["sha256"] = json!("0".repeat(64));
        assert!(parse(RuntimeKind::Science, &serde_json::to_vec(&altered).unwrap()).is_err());
    }
    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "Actual offline copies and LPAC processes; requires explicit acceptance output and seed directories"]
    async fn copied_v2_runtimes_execute_source_stdlib_science_and_lpac_negative_checks() {
        use crate::laboratory::{isolation::{IsolatedProcess, IsolationSpec}, process};
        use std::time::Duration;
        let output = PathBuf::from(std::env::var_os("PHASEFORGE_RUNTIME_ACCEPTANCE_ROOT").expect("explicit acceptance directory required"));
        assert!(output.is_absolute() && !output.exists(), "Use a fresh absolute evidence directory"); fs::create_dir_all(&output).unwrap();
        let seeds = source_root().unwrap(); let token = CancellationToken::new(); let data = output.join("data");
        fs::create_dir_all(data.join("environments/science-v1")).unwrap(); fs::write(data.join("environments/science-v1/prior-evidence"), b"preserve v1").unwrap();
        let science = provision_from(&seeds.join("science-v2"), &RuntimeKind::Science.directory(&data), RuntimeKind::Science, &token).unwrap();
        let generated = provision_from(&seeds.join("python-numpy-v2"), &RuntimeKind::Generated.directory(&data), RuntimeKind::Generated, &token).unwrap();
        let trusted_work = output.join("trusted-work"); fs::create_dir(&trusted_work).unwrap();
        let mut command = process::clean_command(&science.python, &trusted_work);
        command.args(["-I", "-B", "-c", SCIENCE_SMOKE]).stdout(File::create(trusted_work.join("stdout.json")).unwrap()).stderr(File::create(trusted_work.join("stderr.log")).unwrap());
        let status = process::OwnedProcess::spawn(&mut command, 1024).unwrap().wait(&token).await.unwrap();
        assert!(status.success(), "{}", fs::read_to_string(trusted_work.join("stderr.log")).unwrap());
        let science_receipt: Value = serde_json::from_slice(&fs::read(trusted_work.join("stdout.json")).unwrap()).unwrap();
        let work = output.join("lpac-work"); fs::create_dir(&work).unwrap();
        let outside = output.join("synthetic-secret.txt"); fs::write(&outside, b"synthetic canary, no credentials").unwrap();
        let outside_write = output.join("forbidden-write.txt"); let runtime_write = generated.directory.join("forbidden-write.txt");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); let port = listener.local_addr().unwrap().port();
        std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let origin_script = r#"
import json,pathlib,csv,numpy,sys
def local_origin(value): return pathlib.Path(str(pathlib.Path(value).resolve()).removeprefix('\\\\?\\'))
root=local_origin(sys.executable).parent
origins={m.__name__:str(local_origin(m.__file__)) for m in (json,pathlib,csv,numpy)}
assert all(pathlib.Path(p).is_relative_to(root) for p in origins.values()),origins
assert all(local_origin(p).is_relative_to(root) for p in sys.path),sys.path
assert sys.flags.isolated and sys.flags.dont_write_bytecode and sys.flags.optimize==0
pathlib.Path('module-origins.json').write_text(json.dumps({'runtime':str(root),'sys_path':sys.path,'module_origins':origins,'isolated':sys.flags.isolated,'dont_write_bytecode':sys.flags.dont_write_bytecode,'optimize':sys.flags.optimize}))
"#;
        fs::write(work.join("probe.py"), format!("{}\n{}", include_str!("../../../tools/isolation_probe.py"), origin_script)).unwrap();
        fs::write(work.join("input.json"), serde_json::to_vec(&json!({"read_canary":outside,"write_canary":outside_write,"runtime_canary":runtime_write,"listening_port":port})).unwrap()).unwrap();
        let spec = IsolationSpec { id: Uuid::new_v4(), runtime_directory:generated.directory.clone(), working_directory:work.clone(), script:work.join("probe.py"), arguments:vec!["--input".into(),work.join("input.json").to_string_lossy().into_owned()], memory_mb:512,process_limit:4,time_limit:Some(Duration::from_secs(30)) };
        let mut child = IsolatedProcess::spawn(spec.clone()).unwrap(); let outcome = child.wait(&token).await.unwrap(); drop(child);
        assert_eq!(outcome.exit_code, 0, "{}", fs::read_to_string(work.join("stderr.log")).unwrap());
        let negative: Value = serde_json::from_slice(&fs::read(work.join("isolation-report.json")).unwrap()).unwrap();
        for field in ["outside_read","outside_hardlink","outside_write","runtime_write","loopback_network","external_network"] { assert_eq!(negative[field]["denied"], true, "{field}"); }
        assert_eq!(negative["token"], json!({"appcontainer":1,"less_privileged":1})); assert!(negative["linear_residual"].as_f64().unwrap() < 1e-12);
        assert_eq!(negative["retained_sha256"], sha(&fs::read(work.join("calculation.npz")).unwrap()));
        assert_eq!(fs::read(&outside).unwrap(), b"synthetic canary, no credentials"); assert!(!outside_write.exists() && !runtime_write.exists());
        let lpac_origins: Value = serde_json::from_slice(&fs::read(work.join("module-origins.json")).unwrap()).unwrap();
        verify(&science.directory, RuntimeKind::Science, &token).unwrap(); verify(&generated.directory, RuntimeKind::Generated, &token).unwrap();
        assert_eq!(fs::read(data.join("environments/science-v1/prior-evidence")).unwrap(), b"preserve v1");
        let outer = generated.directory.join(OUTER); let saved_outer = fs::read(&outer).unwrap(); fs::write(&outer, b"{}").unwrap();
        assert!(IsolatedProcess::spawn(spec).is_err()); assert_eq!(fs::read(&outer).unwrap(), b"{}");
        fs::write(&outer, saved_outer).unwrap();
        let missing_destination = output.join("missing-runtime"); assert!(provision_from(&output.join("missing-seed"), &missing_destination, RuntimeKind::Science, &token).is_err()); assert!(!missing_destination.exists());
        let report = json!({"passed":true,"science":science,"generated":generated,"trusted_science":science_receipt,"lpac":negative,"lpac_origins":lpac_origins,"runtime_unchanged_after_execution":true,"changed_outer_launch_refused":true,"missing_seed_no_fallback":true,"v1_preserved":true,"offline_copy":true});
        fs::write(output.join("report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap(); println!("Runtime v2 acceptance: {}", output.join("report.json").display());
    }
    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "Actual three solver service entries; run after the explicit copied-runtime acceptance"]
    async fn copied_v2_runtime_runs_all_three_real_solver_service_entries() {
        use crate::{config::AppConfig, persistence::Database, laboratory::LaboratoryService};
        let root=PathBuf::from(std::env::var_os("PHASEFORGE_RUNTIME_ACCEPTANCE_ROOT").expect("explicit completed runtime acceptance required"));
        assert!(root.join("report.json").is_file());
        let config=AppConfig{data_directory:root.join("data"),..Default::default()};
        let database=Database::open(&config.database_path()).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("Copied source runtime integration".into()),question:"Exercise actual trusted service entry points".into()});database.put_project(&project).unwrap();
        let service=LaboratoryService::new(database,config).unwrap();
        let mut reports=vec![];
        for (engine,parameters) in [
            ("openmm_argon",json!({"atom_count":32,"temperature_kelvin":120,"density_g_cm3":0.8,"seed":314159,"platform":"CPU","steps":4,"sample_interval":2,"chunk_frames":4})),
            ("diffusion_2d",json!({"nx":16,"ny":16,"steps":4,"record_interval":2,"dt_s":0.005,"diffusivity_um2_s":0.2})),
            ("newtonian_nbody",json!({"body_ids":["left","right"],"masses":[0.5,0.5],"positions":[[-0.5,0.,0.],[0.5,0.,0.]],"velocities":[[0.,-0.5,0.],[0.,0.5,0.]],"timestep":0.001,"steps":4,"sample_interval":2,"chunk_frames":50,"min_separation":0.05,"boundary":"isolated","unit_system":"scaled_G1"}))] {
            let job=service.create(Uuid::new_v4(),project.id,None,"solver","Bundled v2 service smoke",json!({"engine":engine,"parameters":parameters}),None).unwrap();
            service.execute_solver(job.id,&CancellationToken::new()).await.unwrap_or_else(|error|panic!("{engine}: {error:#}; saved {}",service.directory(job.id).display()));
            let saved=service.get(job.id).unwrap(); assert_eq!(saved.state,"completed");
            let manifest=service.read_json(job.id,"manifest.json").unwrap();
            let runtime_events:Vec<_>=saved.events.iter().filter(|event|event.kind=="runtime_verified").collect();assert!(!runtime_events.is_empty());
            assert!(service.directory(job.id).join("requirements-science.txt").is_file());
            reports.push(json!({"engine":engine,"job_id":job.id,"directory":service.directory(job.id),"state":saved.state,"result":saved.result,"manifest":manifest,"runtime_verified":runtime_events}));
        }
        verify(&RuntimeKind::Science.directory(&root.join("data")),RuntimeKind::Science,&CancellationToken::new()).unwrap();
        fs::write(root.join("service-entry-report.json"),serde_json::to_vec_pretty(&json!({"passed":true,"solvers":reports})).unwrap()).unwrap();
        println!("Runtime v2 solver service entries: {}",root.join("service-entry-report.json").display());
    }
}
