//! Windows LPAC boundary for generated code; distinct from trusted shipped workers.
//! The caller supplies a dedicated managed runtime and a dedicated experiment directory.
use serde::Serialize;
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct IsolationSpec {
    pub id: Uuid,
    pub runtime_directory: PathBuf,
    pub working_directory: PathBuf,
    pub script: PathBuf,
    pub arguments: Vec<String>,
    pub memory_mb: usize,
    pub process_limit: u32,
    pub time_limit: Option<Duration>,
}

#[derive(Debug, Serialize)]
pub struct IsolationOutcome {
    pub exit_code: u32,
    pub process_id: u32,
    pub elapsed_ms: u128,
    pub boundary: &'static str,
}

#[cfg(windows)]
pub use windows::IsolatedProcess;

#[cfg(not(windows))]
pub struct IsolatedProcess;
#[cfg(not(windows))]
impl IsolatedProcess {
    pub fn spawn(_: IsolationSpec) -> anyhow::Result<Self> {
        anyhow::bail!("Generated-code isolation requires the validated Windows LPAC boundary; unrestricted fallback is disabled")
    }
    pub async fn wait(&mut self, _: &CancellationToken) -> anyhow::Result<IsolationOutcome> {
        anyhow::bail!("Windows isolation is unavailable")
    }
    pub fn stop_and_reap(&mut self) -> anyhow::Result<()> {
        anyhow::bail!("Windows isolation is unavailable")
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use anyhow::{ensure, Context};
    use std::{
        collections::BTreeMap,
        ffi::OsStr,
        fs,
        mem::{size_of, size_of_val, zeroed},
        os::windows::ffi::OsStrExt,
        path::Path,
        ptr::{null, null_mut},
        time::Instant,
    };
    use windows_sys::Win32::{
        Foundation::*,
        Security::{Authorization::*, Isolation::*, *},
        Storage::FileSystem::{
            DELETE, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        },
        System::{JobObjects::*, Threading::*},
    };

    static ACL_GATE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    fn wide(value: impl AsRef<OsStr>) -> anyhow::Result<Vec<u16>> {
        let mut value: Vec<u16> = value.as_ref().encode_wide().collect();
        ensure!(!value.contains(&0), "Embedded NUL is forbidden");
        value.push(0);
        Ok(value)
    }
    fn win(ok: i32, action: &str) -> anyhow::Result<()> {
        ensure!(ok != 0, "{action}: {}", std::io::Error::last_os_error());
        Ok(())
    }
    fn canonical_directory(path: &Path) -> anyhow::Result<PathBuf> {
        ensure!(
            path.is_absolute() && path.is_dir(),
            "An existing absolute managed directory is required"
        );
        ensure!(
            !fs::symlink_metadata(path)?.file_type().is_symlink(),
            "Managed directories cannot be reparse links"
        );
        let path = path.canonicalize()?;
        ensure!(
            !path.to_string_lossy().starts_with(r"\\?\UNC\"),
            "Network directories cannot host isolated experiments"
        );
        Ok(path)
    }
    fn reject_links(root: &Path) -> anyhow::Result<()> {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "Managed runtime/work input contains a reparse link"
            );
            if metadata.is_dir() {
                reject_links(&entry.path())?;
            }
        }
        Ok(())
    }
    fn verify_runtime(root: &Path) -> anyhow::Result<()> {
        // The compiled outer pins the inner receipt and every source/native file.
        // An absent/corrupt v2 outer never falls through to a legacy self-described manifest.
        crate::laboratory::runtime::verify(
            root,
            crate::laboratory::runtime::RuntimeKind::Generated,
            &CancellationToken::new(),
        )?;
        Ok(())
    }
    // CreateProcessW passes this directly to Python's CRT; no shell is involved.
    fn quote(value: &str) -> String {
        let mut output = String::from("\"");
        let mut slashes = 0;
        for character in value.chars() {
            if character == '\\' {
                slashes += 1;
                continue;
            }
            if character == '"' {
                output.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
            } else {
                output.extend(std::iter::repeat_n('\\', slashes));
            }
            slashes = 0;
            output.push(character);
        }
        output.extend(std::iter::repeat_n('\\', slashes * 2));
        output.push('"');
        output
    }

    struct Handle(isize);
    impl Handle {
        fn raw(&self) -> HANDLE {
            self.0 as HANDLE
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe {
                    CloseHandle(self.raw());
                }
            }
        }
    }

    struct Profile {
        name: Vec<u16>,
        sid: isize,
        grants: Vec<PathBuf>,
    }
    impl Profile {
        fn create(id: Uuid) -> anyhow::Result<Self> {
            let name = wide(format!("PhaseForge.Experiment.{}", id.simple()))?;
            let mut sid = null_mut();
            let hr = unsafe {
                CreateAppContainerProfile(
                    name.as_ptr(),
                    name.as_ptr(),
                    name.as_ptr(),
                    null(),
                    0,
                    &mut sid,
                )
            };
            // A stale identity must be reconciled explicitly, never shared with an active process.
            ensure!(
                hr >= 0,
                "Could not create a fresh AppContainer profile: HRESULT 0x{:08x}",
                hr as u32
            );
            Ok(Self {
                name,
                sid: sid as isize,
                grants: vec![],
            })
        }
        fn grant(&mut self, path: &Path, write: bool) -> anyhow::Result<()> {
            unsafe {
                change_acl(
                    path,
                    self.sid as PSID,
                    if write {
                        FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE
                    } else {
                        FILE_GENERIC_READ | FILE_GENERIC_EXECUTE
                    },
                    GRANT_ACCESS,
                )?;
            }
            self.grants.push(path.into());
            Ok(())
        }
    }
    impl Drop for Profile {
        fn drop(&mut self) {
            for path in &self.grants {
                let _ = unsafe { change_acl(path, self.sid as PSID, 0, REVOKE_ACCESS) };
            }
            unsafe {
                DeleteAppContainerProfile(self.name.as_ptr());
                FreeSid(self.sid as PSID);
            }
        }
    }
    unsafe fn change_acl(
        path: &Path,
        sid: PSID,
        permissions: u32,
        mode: ACCESS_MODE,
    ) -> anyhow::Result<()> {
        let _guard = ACL_GATE.lock();
        let path = wide(path)?;
        let mut descriptor = null_mut();
        let mut old_acl = null_mut();
        let code = GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut old_acl,
            null_mut(),
            &mut descriptor,
        );
        ensure!(code == 0, "Cannot read managed directory ACL: {code}");
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: permissions,
            grfAccessMode: mode,
            grfInheritance: OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: sid as *mut u16,
            },
        };
        let mut acl = null_mut();
        let code = SetEntriesInAclW(1, &entry, old_acl, &mut acl);
        if code != 0 {
            LocalFree(descriptor);
            anyhow::bail!("Cannot prepare managed directory ACL: {code}");
        }
        let code = SetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            acl,
            null(),
        );
        LocalFree(acl as _);
        LocalFree(descriptor);
        ensure!(code == 0, "Cannot apply managed directory ACL: {code}");
        Ok(())
    }

    unsafe fn verify_file_grant(path: &Path, sid: PSID, required: u32) -> anyhow::Result<()> {
        let name = wide(path)?;
        let mut descriptor = null_mut();
        let mut acl = null_mut();
        let code = GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut acl,
            null_mut(),
            &mut descriptor,
        );
        ensure!(code == 0, "Cannot verify runtime file ACL: {code}");
        let trustee = TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid as _,
        };
        let mut allowed = 0;
        let code = GetEffectiveRightsFromAclW(acl, &trustee, &mut allowed);
        LocalFree(descriptor);
        ensure!(
            code == 0 && allowed & required == required,
            "Package lacks required runtime file rights: {} (code {code}, mask {allowed:x})",
            path.display()
        );
        Ok(())
    }

    struct Attributes {
        buffer: Vec<usize>,
    }
    struct Capability(isize);
    impl Capability {
        fn registry_read() -> anyhow::Result<Self> {
            unsafe {
                let mut groups = null_mut();
                let mut group_count = 0;
                let mut sids = null_mut();
                let mut sid_count = 0;
                win(
                    DeriveCapabilitySidsFromName(
                        wide("registryRead")?.as_ptr(),
                        &mut groups,
                        &mut group_count,
                        &mut sids,
                        &mut sid_count,
                    ),
                    "Derive runtime registry-read capability",
                )?;
                for index in 0..group_count {
                    LocalFree(*groups.add(index as usize));
                }
                LocalFree(groups as _);
                if sid_count != 1 {
                    for index in 0..sid_count {
                        LocalFree(*sids.add(index as usize));
                    }
                    LocalFree(sids as _);
                    anyhow::bail!("Unexpected registry-read capability count");
                }
                let sid = *sids;
                LocalFree(sids as _);
                Ok(Self(sid as isize))
            }
        }
    }
    impl Drop for Capability {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0 as _);
            }
        }
    }
    impl Attributes {
        fn new() -> anyhow::Result<Self> {
            let mut size = 0;
            unsafe {
                InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
            }
            ensure!(size > 0, "Process attributes unavailable");
            let mut this = Self {
                buffer: vec![0; size.div_ceil(size_of::<usize>())],
            };
            win(
                unsafe { InitializeProcThreadAttributeList(this.pointer(), 2, 0, &mut size) },
                "Initialize process attributes",
            )?;
            Ok(this)
        }
        fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
            self.buffer.as_mut_ptr() as _
        }
    }
    impl Drop for Attributes {
        fn drop(&mut self) {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }

    // Some Windows builds expose the SDK's class 46 but reject that query.
    // The underlying LPAC token security attribute is independently queryable.
    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *const u16,
    }
    #[repr(C)]
    struct TokenAttribute {
        name: UnicodeString,
        value_type: u16,
        reserved: u16,
        flags: u32,
        count: u32,
        values: *const u64,
    }
    #[repr(C)]
    struct TokenAttributes {
        version: u16,
        reserved: u16,
        count: u32,
        attributes: *const TokenAttribute,
    }
    unsafe fn lpac_flag(token: HANDLE) -> anyhow::Result<u32> {
        let mut value = 0u32;
        let mut returned = 0;
        if GetTokenInformation(
            token,
            TokenIsLessPrivilegedAppContainer,
            &mut value as *mut _ as _,
            4,
            &mut returned,
        ) != 0
        {
            return Ok(value);
        }
        let mut required = 0;
        GetTokenInformation(token, TokenSecurityAttributes, null_mut(), 0, &mut required);
        ensure!(
            required as usize >= size_of::<TokenAttributes>() && required <= 65536,
            "LPAC security attributes are unavailable"
        );
        let mut storage = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
        win(
            GetTokenInformation(
                token,
                TokenSecurityAttributes,
                storage.as_mut_ptr() as _,
                required,
                &mut required,
            ),
            "Query LPAC security attribute",
        )?;
        let start = storage.as_ptr() as usize;
        let end = start + required as usize;
        let contained = |pointer: usize, bytes: usize| {
            pointer >= start && pointer.checked_add(bytes).is_some_and(|at| at <= end)
        };
        let header = &*(storage.as_ptr() as *const TokenAttributes);
        ensure!(
            header.version == 1
                && header.count <= 512
                && contained(
                    header.attributes as usize,
                    header.count as usize * size_of::<TokenAttribute>()
                ),
            "Invalid token attribute layout"
        );
        for attribute in std::slice::from_raw_parts(header.attributes, header.count as usize) {
            ensure!(
                attribute.name.length % 2 == 0
                    && contained(
                        attribute.name.buffer as usize,
                        attribute.name.length as usize
                    ),
                "Invalid token attribute name"
            );
            let name = String::from_utf16(std::slice::from_raw_parts(
                attribute.name.buffer,
                attribute.name.length as usize / 2,
            ))?;
            if name == "WIN://NOALLAPPPKG" {
                ensure!(
                    [2, 6].contains(&attribute.value_type)
                        && attribute.count == 1
                        && contained(attribute.values as usize, 8),
                    "Invalid LPAC security attribute value"
                );
                return Ok(u32::from(*attribute.values != 0));
            }
        }
        Ok(0)
    }

    pub struct IsolatedProcess {
        process: Handle,
        job: Handle,
        profile: Profile,
        pid: u32,
        started: Instant,
        time_limit: Option<Duration>,
    }
    impl IsolatedProcess {
        pub fn spawn(spec: IsolationSpec) -> anyhow::Result<Self> {
            ensure!(
                (128..=8192).contains(&spec.memory_mb),
                "Memory limit must be 128–8192 MiB"
            );
            ensure!(
                (1..=16).contains(&spec.process_limit),
                "Process limit must be 1–16"
            );
            ensure!(
                spec.time_limit
                    .is_none_or(|limit| limit >= Duration::from_secs(1)
                        && limit <= Duration::from_secs(86400)),
                "Time limit must be 1 second to 24 hours"
            );
            let runtime = canonical_directory(&spec.runtime_directory)?;
            let working = canonical_directory(&spec.working_directory)?;
            ensure!(
                !runtime.starts_with(&working) && !working.starts_with(&runtime),
                "Runtime and working directory must be separate"
            );
            reject_links(&runtime)?;
            reject_links(&working)?;
            ensure!(
                runtime.join("phaseforge-isolation-runtime.json").is_file(),
                "Provision a pinned dedicated embedded runtime before isolation"
            );
            verify_runtime(&runtime)?;
            let program = runtime.join("python.exe");
            ensure!(program.is_file(), "Managed embedded Python is missing");
            let script = spec
                .script
                .canonicalize()
                .context("Experiment script is missing")?;
            ensure!(
                script.starts_with(&working) && script.is_file(),
                "Experiment script must belong to its working directory"
            );
            let temporary = working.join("tmp");
            fs::create_dir_all(&temporary)?;
            let mut profile = Profile::create(spec.id)?;
            profile.grant(&runtime, false)?;
            profile.grant(&working, true)?;
            unsafe {
                verify_file_grant(
                    &program,
                    profile.sid as PSID,
                    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                )?;
                verify_file_grant(
                    &runtime.join("python313.dll"),
                    profile.sid as PSID,
                    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                )?;
            }

            let mut environment = BTreeMap::new();
            let system_root =
                std::env::var_os("SystemRoot").context("SystemRoot is unavailable")?;
            environment.insert(
                "SystemRoot".to_string(),
                system_root.to_string_lossy().into_owned(),
            );
            environment.insert(
                "WINDIR".to_string(),
                system_root.to_string_lossy().into_owned(),
            );
            for key in [
                "TEMP",
                "TMP",
                "HOME",
                "USERPROFILE",
                "LOCALAPPDATA",
                "APPDATA",
            ] {
                environment.insert(key.into(), temporary.to_string_lossy().into_owned());
            }
            environment.insert(
                "PATH".into(),
                format!(
                    "{};{}\\System32",
                    runtime.display(),
                    system_root.to_string_lossy()
                ),
            );
            for key in [
                "OMP_NUM_THREADS",
                "OPENBLAS_NUM_THREADS",
                "MKL_NUM_THREADS",
                "PYTHONNOUSERSITE",
            ] {
                environment.insert(key.into(), "1".into());
            }
            let mut block = Vec::new();
            for (key, value) in environment {
                block.extend(wide(format!("{key}={value}"))?);
            }
            block.push(0);
            let bootstrap = "import runpy,sys;sys.stdout=open('stdout.log','w',buffering=1,encoding='utf-8');sys.stderr=open('stderr.log','w',buffering=1,encoding='utf-8');sys.argv=sys.argv[1:];runpy.run_path(sys.argv[0],run_name='__main__')";
            let mut arguments = vec![
                program.to_string_lossy().into_owned(),
                "-I".into(),
                "-B".into(),
                "-c".into(),
                bootstrap.into(),
                script.to_string_lossy().into_owned(),
            ];
            arguments.extend(spec.arguments);
            let mut command = wide(
                arguments
                    .iter()
                    .map(|arg| quote(arg))
                    .collect::<Vec<_>>()
                    .join(" "),
            )?;
            ensure!(
                command.len() < 32767,
                "Experiment command exceeds the Windows command limit"
            );
            let mut attributes = Attributes::new()?;
            let registry_read = Capability::registry_read()?;
            let mut capability_list = [SID_AND_ATTRIBUTES {
                Sid: registry_read.0 as PSID,
                Attributes: 4,
            }]; // SE_GROUP_ENABLED
            let capabilities = SECURITY_CAPABILITIES {
                AppContainerSid: profile.sid as PSID,
                Capabilities: capability_list.as_mut_ptr(),
                CapabilityCount: 1,
                Reserved: 0,
            };
            let policy: u32 = 1; // PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT
            unsafe {
                win(
                    UpdateProcThreadAttribute(
                        attributes.pointer(),
                        0,
                        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                        &capabilities as *const _ as _,
                        size_of::<SECURITY_CAPABILITIES>(),
                        null_mut(),
                        null(),
                    ),
                    "Set AppContainer capabilities",
                )?;
                win(
                    UpdateProcThreadAttribute(
                        attributes.pointer(),
                        0,
                        PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY as usize,
                        &policy as *const _ as _,
                        size_of::<u32>(),
                        null_mut(),
                        null(),
                    ),
                    "Require less-privileged AppContainer",
                )?;
                let mut startup: STARTUPINFOEXW = zeroed();
                startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
                startup.lpAttributeList = attributes.pointer();
                let mut info: PROCESS_INFORMATION = zeroed();
                win(
                    CreateProcessW(
                        wide(&program)?.as_ptr(),
                        command.as_mut_ptr(),
                        null(),
                        null(),
                        0,
                        CREATE_SUSPENDED
                            | CREATE_NO_WINDOW
                            | CREATE_UNICODE_ENVIRONMENT
                            | EXTENDED_STARTUPINFO_PRESENT,
                        block.as_ptr() as _,
                        wide(&working)?.as_ptr(),
                        &startup.StartupInfo,
                        &mut info,
                    ),
                    "Create isolated process",
                )?;
                let process = Handle(info.hProcess as isize);
                let thread = Handle(info.hThread as isize);
                let job = Handle(CreateJobObjectW(null(), null()) as isize);
                let configure = (|| -> anyhow::Result<()> {
                    ensure!(job.0 != 0, "Cannot create experiment job object");
                    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
                    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
                        | JOB_OBJECT_LIMIT_JOB_MEMORY
                        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                        | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
                    limits.ProcessMemoryLimit = spec.memory_mb * 1024 * 1024;
                    limits.JobMemoryLimit = limits.ProcessMemoryLimit;
                    limits.BasicLimitInformation.ActiveProcessLimit = spec.process_limit;
                    win(
                        SetInformationJobObject(
                            job.raw(),
                            JobObjectExtendedLimitInformation,
                            &limits as *const _ as _,
                            size_of_val(&limits) as u32,
                        ),
                        "Set experiment job limits",
                    )?;
                    win(
                        AssignProcessToJobObject(job.raw(), process.raw()),
                        "Assign suspended experiment to job",
                    )?;
                    let mut token = null_mut();
                    win(
                        OpenProcessToken(process.raw(), TOKEN_QUERY, &mut token),
                        "Inspect experiment token",
                    )?;
                    let token = Handle(token as isize);
                    for class in [TokenIsAppContainer] {
                        let mut value: u32 = 0;
                        let mut returned = 0;
                        win(
                            GetTokenInformation(
                                token.raw(),
                                class,
                                &mut value as *mut _ as _,
                                4,
                                &mut returned,
                            ),
                            "Verify isolated token",
                        )?;
                        ensure!(
                            value == 1,
                            "Windows did not create the required LPAC token; execution refused"
                        );
                    }
                    ensure!(
                        lpac_flag(token.raw())? == 1,
                        "Windows did not set the LPAC security attribute; execution refused"
                    );
                    ensure!(
                        ResumeThread(thread.raw()) != u32::MAX,
                        "Cannot resume isolated experiment"
                    );
                    Ok(())
                })();
                if let Err(error) = configure {
                    TerminateProcess(process.raw(), 1);
                    WaitForSingleObject(process.raw(), 3000);
                    return Err(error);
                }
                Ok(Self {
                    process,
                    job,
                    profile,
                    pid: info.dwProcessId,
                    started: Instant::now(),
                    time_limit: spec.time_limit,
                })
            }
        }
        pub fn id(&self) -> u32 {
            self.pid
        }
        /// Generated output is inspected only after the whole job reports zero processes.
        pub fn stop_and_reap(&mut self) -> anyhow::Result<()> {
            unsafe {
                TerminateJobObject(self.job.raw(), 1);
            }
            let started = Instant::now();
            loop {
                let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
                win(
                    unsafe {
                        QueryInformationJobObject(
                            self.job.raw(),
                            JobObjectBasicAccountingInformation,
                            &mut accounting as *mut _ as *mut _,
                            size_of_val(&accounting) as u32,
                            null_mut(),
                        )
                    },
                    "Observe isolated process tree termination",
                )?;
                if accounting.ActiveProcesses == 0 {
                    return Ok(());
                }
                ensure!(started.elapsed() < Duration::from_secs(3), "Isolated process tree has not finished stopping; generated outputs remain unread");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        pub async fn wait(
            &mut self,
            token: &CancellationToken,
        ) -> anyhow::Result<IsolationOutcome> {
            loop {
                if token.is_cancelled()
                    || self
                        .time_limit
                        .is_some_and(|limit| self.started.elapsed() >= limit)
                {
                    self.terminate();
                    anyhow::bail!(
                        "Isolated experiment {}",
                        if token.is_cancelled() {
                            "cancelled"
                        } else {
                            "time limit expired"
                        }
                    );
                }
                let status = unsafe { WaitForSingleObject(self.process.raw(), 0) };
                ensure!(
                    status != WAIT_FAILED,
                    "Cannot observe isolated process status"
                );
                if status == WAIT_OBJECT_0 {
                    let mut code = 0;
                    win(
                        unsafe { GetExitCodeProcess(self.process.raw(), &mut code) },
                        "Read isolated exit code",
                    )?;
                    // No detached child survives a completed root process.
                    unsafe {
                        TerminateJobObject(self.job.raw(), code);
                    }
                    return Ok(IsolationOutcome {
                        exit_code: code,
                        process_id: self.pid,
                        elapsed_ms: self.started.elapsed().as_millis(),
                        boundary: "windows_lpac_no_network",
                    });
                }
                tokio::select! { _ = token.cancelled() => {}, _ = tokio::time::sleep(Duration::from_millis(25)) => {} }
            }
        }
        fn terminate(&mut self) {
            unsafe {
                TerminateJobObject(self.job.raw(), 1);
                WaitForSingleObject(self.process.raw(), 3000);
            }
        }
    }
    impl Drop for IsolatedProcess {
        fn drop(&mut self) {
            self.terminate();
            let _ = &self.profile;
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::{json, Value};
        fn probe_spec(
            script: &str,
            memory_mb: usize,
            process_limit: u32,
            seconds: u64,
        ) -> Option<IsolationSpec> {
            let Some(root) = std::env::var_os("PHASEFORGE_ISOLATION_TEST_ROOT").map(PathBuf::from)
            else {
                eprintln!("SKIP: PHASEFORGE_ISOLATION_TEST_ROOT is not configured");
                return None;
            };
            let work = root.join(format!("supervision-{}", Uuid::new_v4()));
            fs::create_dir_all(&work).unwrap();
            fs::write(work.join("fixture.py"), script).unwrap();
            Some(IsolationSpec {
                id: Uuid::new_v4(),
                runtime_directory: root.join("runtime"),
                working_directory: work.clone(),
                script: work.join("fixture.py"),
                arguments: vec![],
                memory_mb,
                process_limit,
                time_limit: Some(Duration::from_secs(seconds)),
            })
        }
        async fn read_receipt(path: &Path) -> Value {
            let until = Instant::now() + Duration::from_secs(8);
            loop {
                if let Ok(bytes) = fs::read(path) {
                    if let Ok(value) = serde_json::from_slice(&bytes) {
                        return value;
                    }
                }
                assert!(
                    Instant::now() < until,
                    "Fixture did not produce {}",
                    path.display()
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        #[tokio::test]
        async fn lpac_process_tree_stops_on_cancel_deadline_drop_and_root_exit() {
            for mode in ["cancel", "deadline", "drop", "root_exit"] {
                let script=format!("import json,pathlib,subprocess,sys,time\nchild=subprocess.Popen([sys.executable,'-I','-c','import time;time.sleep(120)'])\npathlib.Path('tree.json').write_text(json.dumps({{'parent':__import__('os').getpid(),'child':child.pid}}))\ntime.sleep({})\n",if mode=="root_exit"{"0.2"}else{"120"});
                let Some(spec) =
                    probe_spec(&script, 256, 4, if mode == "deadline" { 2 } else { 20 })
                else {
                    return;
                };
                let root = spec.working_directory.clone();
                let mut process = IsolatedProcess::spawn(spec).unwrap();
                let receipt = read_receipt(&root.join("tree.json")).await;
                let handles: Vec<_> = ["parent", "child"]
                    .into_iter()
                    .map(|key| {
                        let raw = unsafe {
                            OpenProcess(0x00100000, 0, receipt[key].as_u64().unwrap() as u32)
                        };
                        assert!(!raw.is_null());
                        let handle = Handle(raw as isize);
                        assert_eq!(
                            unsafe { WaitForSingleObject(handle.raw(), 0) },
                            WAIT_TIMEOUT
                        );
                        handle
                    })
                    .collect();
                if mode == "drop" {
                    drop(process);
                } else {
                    let token = CancellationToken::new();
                    if mode == "cancel" {
                        token.cancel();
                    }
                    let result = tokio::time::timeout(Duration::from_secs(5), process.wait(&token))
                        .await
                        .unwrap();
                    if mode == "root_exit" {
                        assert_eq!(result.unwrap().exit_code, 0);
                    } else {
                        assert!(result.is_err());
                    }
                    drop(process);
                }
                for handle in handles {
                    assert_eq!(
                        unsafe { WaitForSingleObject(handle.raw(), 3000) },
                        WAIT_OBJECT_0,
                        "{mode} left a running process"
                    );
                }
            }
        }
        #[tokio::test]
        async fn lpac_enforces_per_process_and_aggregate_memory_caps() {
            for (megabytes, expected) in [(16, 0), (512, 37)] {
                let script=format!("import sys\ntry:\n memory=bytearray({megabytes}*1024*1024)\nexcept MemoryError:\n sys.exit(37)\n");
                let Some(spec) = probe_spec(&script, 128, 4, 15) else {
                    return;
                };
                let mut process = IsolatedProcess::spawn(spec).unwrap();
                assert_eq!(
                    process
                        .wait(&CancellationToken::new())
                        .await
                        .unwrap()
                        .exit_code,
                    expected
                );
            }
            let script="import subprocess,sys\nheld=bytearray(112*1024*1024)\nchild=subprocess.Popen([sys.executable,'-I','-c','import sys\\ntry:\\n a=bytearray(112*1024*1024)\\nexcept MemoryError:\\n sys.exit(37)'])\nsys.exit(child.wait())\n";
            let Some(spec) = probe_spec(script, 192, 4, 15) else {
                return;
            };
            let mut process = IsolatedProcess::spawn(spec).unwrap();
            assert_eq!(process.wait(&CancellationToken::new()).await.unwrap().exit_code,37,"Combined allocation must hit the job cap while each process is below its individual cap");
        }
        #[tokio::test]
        async fn lpac_enforces_child_count_without_disabling_allowed_children() {
            let script="import subprocess,sys\ntry:\n child=subprocess.Popen([sys.executable,'-I','-c','pass'])\nexcept OSError:\n sys.exit(38)\nsys.exit(child.wait())\n";
            for (limit, expected) in [(1, 38), (2, 0)] {
                let Some(spec) = probe_spec(script, 256, limit, 15) else {
                    return;
                };
                let mut process = IsolatedProcess::spawn(spec).unwrap();
                assert_eq!(
                    process
                        .wait(&CancellationToken::new())
                        .await
                        .unwrap()
                        .exit_code,
                    expected
                );
            }
        }
        #[tokio::test]
        async fn actual_lpac_numerics_and_negative_canaries() {
            let Some(root) = std::env::var_os("PHASEFORGE_ISOLATION_TEST_ROOT").map(PathBuf::from)
            else {
                eprintln!(
                    "SKIP: PHASEFORGE_ISOLATION_TEST_ROOT must name the dedicated probe root"
                );
                return;
            };
            let runtime = root.join("runtime");
            let work = root.join(format!("probe-{}", Uuid::new_v4()));
            fs::create_dir_all(&work).unwrap();
            let outside = root.join(format!("outside-{}.txt", Uuid::new_v4()));
            fs::write(&outside, b"Synthetic secret canary only").unwrap();
            let write_target = outside.with_extension("forbidden");
            let runtime_target = runtime.join("forbidden-write.txt");
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            std::net::TcpStream::connect(("127.0.0.1", port)).unwrap(); // Host positive control.
            fs::write(
                work.join("probe.py"),
                include_str!("../../../tools/isolation_probe.py"),
            )
            .unwrap();
            fs::write(work.join("input.json"), serde_json::to_vec(&json!({"read_canary":outside,"write_canary":write_target,"runtime_canary":runtime_target,"listening_port":port})).unwrap()).unwrap();
            let mut child = IsolatedProcess::spawn(IsolationSpec {
                id: Uuid::new_v4(),
                runtime_directory: runtime,
                working_directory: work.clone(),
                script: work.join("probe.py"),
                arguments: vec![
                    "--input".into(),
                    work.join("input.json").to_string_lossy().into_owned(),
                ],
                memory_mb: 512,
                process_limit: 4,
                time_limit: Some(Duration::from_secs(30)),
            })
            .unwrap();
            let outcome = child.wait(&CancellationToken::new()).await.unwrap();
            drop(child);
            let error = fs::read_to_string(work.join("stderr.log")).unwrap_or_default();
            assert_eq!(
                outcome.exit_code,
                0,
                "LPAC exited {:08x}: {}. Artifacts: {}",
                outcome.exit_code,
                error,
                work.display()
            );
            let report: Value =
                serde_json::from_slice(&fs::read(work.join("isolation-report.json")).unwrap())
                    .unwrap();
            assert_eq!(
                report["token"],
                json!({"appcontainer":1,"less_privileged":1})
            );
            assert!(report["linear_residual"].as_f64().unwrap() < 1e-12);
            for field in [
                "outside_read",
                "outside_hardlink",
                "outside_write",
                "runtime_write",
                "loopback_network",
                "external_network",
            ] {
                assert_eq!(report[field]["denied"], true, "{field}: {}", report[field]);
            }
            assert!(!write_target.exists() && !runtime_target.exists());
            assert_eq!(fs::read(&outside).unwrap(), b"Synthetic secret canary only");
            use sha2::{Digest, Sha256};
            assert_eq!(
                report["retained_sha256"],
                format!(
                    "{:x}",
                    Sha256::digest(fs::read(work.join("calculation.npz")).unwrap())
                )
            );
            println!(
                "LPAC acceptance report: {}",
                work.join("isolation-report.json").display()
            );
        }
    }
}
