//! Supervision for trusted, shipped scientific workers. This is not an arbitrary-code sandbox.
use std::{path::Path, process::{Child, Command, Stdio}, time::Duration};
use anyhow::Context;
use tokio_util::sync::CancellationToken;

pub fn clean_command(program: &Path, working: &Path) -> Command {
    let mut command = Command::new(program);
    command.current_dir(working).env_clear().stdin(Stdio::null());
    // No inherited provider credentials, Python module path or desktop launch token.
    for name in ["SystemRoot", "WINDIR", "TEMP", "TMP", "COMSPEC", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(value) = std::env::var_os(name) { command.env(name, value); }
    }
    if let Some(parent) = program.parent() {
        let mut paths = vec![parent.to_path_buf()];
        if let Some(root) = std::env::var_os("SystemRoot") { paths.push(std::path::PathBuf::from(root).join("System32")); }
        if let Ok(path) = std::env::join_paths(paths) { command.env("PATH", path); }
    }
    command.env("PYTHONNOUSERSITE", "1").env("OPENMM_CPU_THREADS", "4").env("OMP_NUM_THREADS", "4");
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
    command
}

pub struct OwnedProcess { child: Child, #[cfg(windows)] boundary: isize }
impl OwnedProcess {
    pub fn spawn(command: &mut Command, memory_mb: usize) -> anyhow::Result<Self> {
        // Assign the still-suspended process to its job before any worker instruction runs.
        #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000004);}
        let mut child = command.spawn().context("Could not launch scientific worker")?;
        #[cfg(windows)] {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() { let _ = child.kill(); anyhow::bail!("Could not create Windows process supervision boundary"); }
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
                limits.ProcessMemoryLimit = memory_mb.saturating_mul(1024 * 1024);
                if SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits as *const _ as *const _, std::mem::size_of_val(&limits) as u32) == 0 || AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
                    let error = std::io::Error::last_os_error();
                    let _ = child.kill(); CloseHandle(job); return Err(error.into());
                }
                if let Err(error)=resume_worker(child.id()){
                    let _=child.kill();let _=child.wait();CloseHandle(job);return Err(error);
                }
                Ok(Self {child, boundary: job as isize})
            }
        }
        #[cfg(not(windows))] { let _ = memory_mb; Ok(Self {child}) }
    }
    pub async fn wait(&mut self, token: &CancellationToken) -> anyhow::Result<std::process::ExitStatus> {
        self.wait_inner(token,None).await
    }
    pub async fn wait_cooperative(&mut self, token:&CancellationToken, signal:&Path)->anyhow::Result<std::process::ExitStatus>{
        self.wait_inner(token,Some(signal)).await
    }
    async fn wait_inner(&mut self,token:&CancellationToken,signal:Option<&Path>)->anyhow::Result<std::process::ExitStatus>{
        loop {
            if token.is_cancelled() {
                if let Some(signal)=signal {
                    let _=std::fs::write(signal,b"checkpoint and stop");
                    let until=std::time::Instant::now()+Duration::from_secs(3);
                    while std::time::Instant::now()<until {
                        if self.child.try_wait()?.is_some(){anyhow::bail!("Worker stopped; retained outputs and available recovery checkpoints remain in the job");}
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
                self.stop(); anyhow::bail!("Worker cancelled; last complete checkpoint and artifacts retained");
            }
            if let Some(status) = self.child.try_wait()? { return Ok(status); }
            tokio::select! { _ = token.cancelled() => {}, _ = tokio::time::sleep(Duration::from_millis(150)) => {} }
        }
    }
    pub fn id(&self) -> u32 { self.child.id() }
    fn stop(&mut self) {
        #[cfg(windows)] unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.boundary as *mut _, 1); }
        let _ = self.child.kill(); let _ = self.child.wait();
    }
}
#[cfg(windows)]
unsafe fn resume_worker(pid:u32)->anyhow::Result<()> {
    use windows_sys::Win32::{Foundation::{CloseHandle,INVALID_HANDLE_VALUE},System::{Diagnostics::ToolHelp::*,Threading::*}};
    let snapshot=CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD,0);
    if snapshot==INVALID_HANDLE_VALUE{return Err(std::io::Error::last_os_error().into());}
    let mut entry:THREADENTRY32=std::mem::zeroed();entry.dwSize=std::mem::size_of::<THREADENTRY32>() as u32;
    let mut next=Thread32First(snapshot,&mut entry);let mut resumed=false;
    while next!=0 {
        if entry.th32OwnerProcessID==pid {
            let thread=OpenThread(THREAD_SUSPEND_RESUME,0,entry.th32ThreadID);
            if !thread.is_null(){resumed=ResumeThread(thread)!=u32::MAX;CloseHandle(thread);}
            break;
        }
        next=Thread32Next(snapshot,&mut entry);
    }
    CloseHandle(snapshot);
    anyhow::ensure!(resumed,"Could not resume the supervised Windows worker");Ok(())
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() { self.stop(); }
        #[cfg(windows)] unsafe { windows_sys::Win32::Foundation::CloseHandle(self.boundary as *mut _); }
    }
}
