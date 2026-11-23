//! Every shell runs inside a Windows Job Object.
//!
//! On Windows, Ctrl+C can end a wrapper (`pnpm.cmd`, `npm.cmd`…) while the real
//! child (`node`) keeps running attached to the console: PowerShell is back at
//! its prompt but output keeps arriving. Orphans keep a dead parent PID, so a
//! parent/child walk misses them; job membership does not. The job is also
//! KILL_ON_JOB_CLOSE, so closing a pane ends everything it started.

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
    PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
};

pub struct Job(HANDLE);

impl Job {
    /// Put `pid` (the shell) in a new job. Children it spawns join automatically.
    pub fn for_process(pid: u32) -> Option<Self> {
        // SAFETY: plain Win32 calls; every handle is checked and closed (Drop for the job).
        unsafe {
            let job = Job(CreateJobObjectW(std::ptr::null(), std::ptr::null()));
            if job.0.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            );
            let proc = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if proc.is_null() {
                return None;
            }
            let ok = AssignProcessToJobObject(job.0, proc) != 0;
            CloseHandle(proc);
            ok.then_some(job)
        }
    }

    /// PIDs currently in the job.
    fn pids(&self) -> Vec<u32> {
        const MAX: usize = 4096;
        // Header (two u32) + ULONG_PTR array; usize-aligned buffer.
        let mut buf = vec![0usize; 2 + MAX];
        // SAFETY: buffer is large and aligned enough for the variable-length struct.
        unsafe {
            let ok = QueryInformationJobObject(
                self.0,
                JobObjectBasicProcessIdList,
                buf.as_mut_ptr() as *mut _,
                (buf.len() * std::mem::size_of::<usize>()) as u32,
                std::ptr::null_mut(),
            );
            if ok == 0 {
                return vec![];
            }
            let list = &*(buf.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST);
            let n = (list.NumberOfProcessIdsInList as usize).min(MAX);
            std::slice::from_raw_parts(list.ProcessIdList.as_ptr(), n)
                .iter()
                .map(|&p| p as u32)
                .collect()
        }
    }

    /// Terminate everything in the job except `keep` (the shell). Returns how many.
    pub fn kill_except(&self, keep: u32) -> usize {
        self.pids()
            .into_iter()
            .filter(|&pid| pid != keep && !is_console_host(pid))
            .filter(|&pid| {
                // SAFETY: handle opened for termination only and always closed.
                unsafe {
                    let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
                    if h.is_null() {
                        return false;
                    }
                    let ok = TerminateProcess(h, 1) != 0;
                    CloseHandle(h);
                    ok
                }
            })
            .count()
    }
}

/// conhost / OpenConsole own a console; killing them would crash its clients.
fn is_console_host(pid: u32) -> bool {
    exe_name(pid).is_some_and(|n| {
        n.eq_ignore_ascii_case("conhost.exe") || n.eq_ignore_ascii_case("OpenConsole.exe")
    })
}

fn exe_name(pid: u32) -> Option<String> {
    // SAFETY: query-only handle, closed before returning; buffer length is passed in.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(h);
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        ok.then(|| path.rsplit('\\').next().unwrap_or_default().to_string())
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: we own the handle. KILL_ON_JOB_CLOSE ends the pane's processes.
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_tracks_orphans_and_kills_them() {
        // Children inherit the job: the ping started by cmd is listed and killable
        // without walking parent PIDs (which break once a parent dies).
        let mut shell = std::process::Command::new("cmd")
            .args(["/c", "ping -n 30 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        let job = Job::for_process(shell.id()).expect("job");
        let ping = || {
            job.pids()
                .into_iter()
                .any(|p| exe_name(p).is_some_and(|n| n.eq_ignore_ascii_case("ping.exe")))
        };
        let start = std::time::Instant::now();
        while !ping() && start.elapsed().as_secs() < 5 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(job.pids().contains(&shell.id()));
        assert!(ping(), "child ping should be in the job");
        assert!(job.kill_except(shell.id()) >= 1);
        // With its child gone cmd exits normally (its console was not touched).
        assert!(shell.wait().unwrap().code().is_some());
        assert!(!ping());
    }

    #[test]
    fn job_finds_orphan_whose_parent_died() {
        // pnpm.cmd case: the wrapper exits, node keeps running with a dead parent.
        if std::process::Command::new("node")
            .arg("-v")
            .output()
            .is_err()
        {
            return; // node not installed
        }
        let mut shell = std::process::Command::new("cmd")
            .args([
                "/c",
                "start /b node -e \"process.on('SIGINT',()=>{});setInterval(()=>{},1000)\"",
            ])
            .spawn()
            .unwrap();
        let job = Job::for_process(shell.id()).expect("job");
        shell.wait().unwrap(); // the wrapper is gone…
        let node = || {
            job.pids()
                .into_iter()
                .any(|p| exe_name(p).is_some_and(|n| n.eq_ignore_ascii_case("node.exe")))
        };
        let start = std::time::Instant::now();
        while !node() && start.elapsed().as_secs() < 5 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(node(), "…but its orphaned node child is still in the job");
        job.kill_except(shell.id());
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!node(), "orphan terminated");
    }
}
