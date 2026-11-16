//! Windows-only Job Object launcher for explicitly approved official Forge tools.
//!
//! This is NOT an OS sandbox: a normal user process still has that user's file
//! and network permissions. It does prevent a processor from running before
//! job assignment and kills the entire contained process tree on cancellation.
#![cfg(windows)]
use std::{
    ffi::c_void,
    io,
    mem::size_of,
    ops::{Deref, DerefMut},
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Child, Command},
    ptr,
};

type Handle = *mut c_void;
const CREATE_SUSPENDED: u32 = 0x00000004;
const CREATE_NO_WINDOW: u32 = 0x08000000;
const TH32CS_SNAPTHREAD: u32 = 0x00000004;
const THREAD_SUSPEND_RESUME: u32 = 0x0002;
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
const JOB_OBJECT_LIMIT_ACTIVE_PROCESS: u32 = 0x00000008;
const JOB_OBJECT_LIMIT_PROCESS_MEMORY: u32 = 0x00000100;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x00002000;

#[repr(C)]
#[derive(Default)]
struct BasicLimit {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}
#[repr(C)]
#[derive(Default)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}
#[repr(C)]
#[derive(Default)]
struct ExtendedLimit {
    basic: BasicLimit,
    io_info: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}
#[repr(C)]
#[derive(Default)]
struct ThreadEntry {
    size: u32,
    usage: u32,
    thread_id: u32,
    owner_pid: u32,
    base_priority: i32,
    delta_priority: i32,
    flags: u32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, kind: i32, info: *const c_void, len: u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
    fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
    fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
    fn OpenThread(access: u32, inherit: i32, thread_id: u32) -> Handle;
    fn ResumeThread(thread: Handle) -> u32;
}

struct OwnedHandle(Handle);
impl OwnedHandle {
    fn new(value: Handle) -> io::Result<Self> {
        if value.is_null() || (value as isize) == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub(crate) struct JobChild {
    child: Child,
    // Dropping this handle kills every remaining member of the job.
    _job: OwnedHandle,
}
impl Deref for JobChild {
    type Target = Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}
impl DerefMut for JobChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

fn resume_single_primary_thread(pid: u32) -> io::Result<()> {
    let snapshot = OwnedHandle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })?;
    let mut entry = ThreadEntry {
        size: size_of::<ThreadEntry>() as u32,
        ..ThreadEntry::default()
    };
    let mut thread_id = None;
    let mut valid = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
    while valid {
        if entry.owner_pid == pid {
            if thread_id.replace(entry.thread_id).is_some() {
                return Err(io::Error::other("suspended processor has multiple threads"));
            }
        }
        valid = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
    }
    let tid = thread_id.ok_or_else(|| io::Error::other("suspended primary thread absent"))?;
    let thread = OwnedHandle::new(unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, tid) })?;
    if unsafe { ResumeThread(thread.0) } == u32::MAX {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Windows CreateProcessW(CREATE_SUSPENDED) → Job Object → ResumeThread.
/// No Java instruction runs until process memory/descendant limits are active.
pub(crate) fn spawn(mut command: Command) -> io::Result<JobChild> {
    let job = OwnedHandle::new(unsafe { CreateJobObjectW(ptr::null_mut(), ptr::null()) })?;
    let mut limits = ExtendedLimit::default();
    limits.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
    limits.basic.active_process_limit = 16;
    limits.process_memory_limit = 3usize * 1024 * 1024 * 1024;
    if unsafe {
        SetInformationJobObject(
            job.0,
            JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            (&limits as *const ExtendedLimit).cast(),
            size_of::<ExtendedLimit>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    command.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
    let mut child = command.spawn()?;
    let assigned = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as Handle) };
    let result = if assigned == 0 {
        Err(io::Error::last_os_error())
    } else {
        resume_single_primary_thread(child.id())
    };
    if let Err(error) = result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(JobChild { child, _job: job })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn benign_process_runs_under_job_object() {
        // CI contract test: never downloads or runs Java/Forge.
        let mut command = Command::new(std::env::var("ComSpec").unwrap_or_else(|_| {
            r"C:\Windows\System32\cmd.exe".to_owned()
        }));
        command.args(["/d", "/c", "exit", "0"]);
        let mut child = spawn(command).expect("suspended child assigned to Job Object");
        assert!(child.wait().unwrap().success());
    }
}
