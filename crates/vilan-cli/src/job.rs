//! Child-process lifetime for the spawned `node` programs
//! (`windows-support.md` §6).
//!
//! On unix a `run --watch` round's child is free of orphans by construction:
//! the child shares this process's group, so the terminal's `Ctrl-C` reaches
//! the whole tree, and `Child::kill` on the direct child is all a restart round
//! needs to hand back the port (anything the program forked is in the same
//! group and dies with the session). Nothing here adds unix behavior — the
//! wrapper is a transparent newtype and the pins say so.
//!
//! Windows has no such group. `Child::kill` is `TerminateProcess` on the direct
//! `node.exe` **only**, so a dev server that forked a worker keeps the port and
//! the next round fails to bind; and if the CLI itself dies (crash, taskkill,
//! console close) every descendant is simply orphaned. The Windows answer is a
//! **Job object** per spawned child, created with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`:
//!
//! * a process assigned to a job passes its membership to everything it
//!   spawns, so `TerminateJobObject` takes the whole tree — that is
//!   [`ManagedChild::kill`];
//! * the limit flag means the kernel terminates the job when its **last handle
//!   closes**, so the CLI's own death — for any reason, including one that runs
//!   no Rust code — reaps the tree.
//!
//! Known, accepted gap: `std::process::Command` cannot spawn `CREATE_SUSPENDED`,
//! so a child could in principle fork between `CreateProcess` and
//! `AssignProcessToJobObject`; such a grandchild escapes the job. The window is
//! the few microseconds before `node` has read its script.

use std::process::{Child, ExitStatus};

/// A spawned child, plus — on Windows only — the Job object that owns its
/// process tree. On unix this is exactly a [`Child`] (pinned by
/// `the_unix_wrapper_adds_no_state`).
pub struct ManagedChild {
    child: Child,
    /// `None` when the job could not be created or assigned: the child is still
    /// perfectly usable, it just falls back to today's direct-kill behavior
    /// rather than failing a watch round over a missing kernel object.
    #[cfg(windows)]
    job: Option<windows::Job>,
}

impl ManagedChild {
    /// Takes ownership of a freshly spawned child, putting it under a Job
    /// object on Windows. Call this immediately after `Command::spawn`.
    pub fn adopt(child: Child) -> ManagedChild {
        #[cfg(windows)]
        {
            let job = windows::Job::create_and_assign(&child);
            ManagedChild { child, job }
        }
        #[cfg(not(windows))]
        {
            ManagedChild { child }
        }
    }

    /// Stops the child. On unix, exactly `Child::kill` (the process group covers
    /// the rest). On Windows, `TerminateJobObject` — the child *and everything
    /// it spawned*.
    pub fn kill(&mut self) -> std::io::Result<()> {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            return job.terminate();
        }
        self.child.kill()
    }

    /// Reaps the direct child. (On Windows the job's other members are already
    /// terminated by `kill`; they were never this process's children, so there
    /// is nothing to reap for them.)
    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait()
    }
}

#[cfg(windows)]
mod windows {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };

    /// An owned Job-object handle. Dropping it closes the handle, which — with
    /// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` set and no other handle open —
    /// terminates every process still in the job.
    pub struct Job(HANDLE);

    // A kernel handle is just a value; this type never shares it, so moving a
    // `ManagedChild` (which the watch loop does, round to round) is safe.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        /// Creates an anonymous kill-on-close job and assigns `child` to it.
        /// `None` on any failure — see [`super::ManagedChild::job`].
        pub fn create_and_assign(child: &Child) -> Option<Job> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                // Owned from here on, so every early return closes the handle.
                let job = Job(handle);
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&raw const limits).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if set == 0 {
                    return None;
                }
                if AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) == 0 {
                    return None;
                }
                Some(job)
            }
        }

        /// Terminates every process in the job — the child's whole tree.
        pub fn terminate(&self) -> std::io::Result<()> {
            // Exit code 1: the child was stopped, not successful. The watch loop
            // discards it (it only waits to reap), and nothing else reads it.
            if unsafe { TerminateJobObject(self.0, 1) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }
    }

    #[cfg(test)]
    impl Job {
        /// How many processes are still alive in the job — the kernel's own
        /// count, which is what makes "the tree died" checkable rather than
        /// inferred. Test-only; `u32::MAX` marks a failed query so a broken
        /// probe can never read as "everything is dead".
        pub fn active_processes(&self) -> u32 {
            use windows_sys::Win32::System::JobObjects::{
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
                QueryInformationJobObject,
            };
            unsafe {
                let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
                let mut returned = 0u32;
                let queried = QueryInformationJobObject(
                    self.0,
                    JobObjectBasicAccountingInformation,
                    (&raw mut info).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    &raw mut returned,
                );
                if queried == 0 {
                    return u32::MAX;
                }
                info.ActiveProcesses
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(all(windows, test))]
impl ManagedChild {
    /// The job's live-process count (0 when there is no job at all).
    fn active_processes(&self) -> u32 {
        self.job.as_ref().map_or(0, |job| job.active_processes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn the_unix_wrapper_adds_no_state() {
        // The unix arm is a *type-level* no-op: no job field, no extra bytes,
        // so `ManagedChild` is `Child` with two forwarding methods. If a future
        // change grows the unix struct, this fails and the "unix behavior is
        // unchanged" claim in the module docs has to be re-argued.
        assert_eq!(size_of::<ManagedChild>(), size_of::<std::process::Child>());
    }

    #[cfg(unix)]
    #[test]
    fn kill_then_wait_reaps_the_child_on_unix() {
        // The forwarding path a restart round takes: `kill` stops the process
        // and `wait` reports the termination — exactly `Child::kill`/`wait`
        // before the wrapper existed.
        let child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let mut managed = ManagedChild::adopt(child);
        managed.kill().expect("kill");
        let status = managed.wait().expect("wait");
        assert!(
            !status.success(),
            "a killed child does not exit successfully"
        );
    }

    /// Runs on the Windows CI leg only (`windows-support.md` §8): the Job
    /// object is the whole point of this module and cannot be exercised from
    /// unix. `cmd.exe /C ping` gives a two-deep tree — `cmd.exe` is the
    /// child, `ping.exe` its own child — so "the job kill takes the tree"
    /// is a real claim and not just "kill killed the child". `ping -n 30` is
    /// the long-lived grandchild rather than `timeout /T 30` because
    /// `timeout.exe` refuses a non-console stdin ("Input redirection is not
    /// supported") and exits immediately — caught by this test's first CI run.
    #[cfg(windows)]
    #[test]
    fn a_job_kill_takes_the_whole_child_tree() {
        use std::time::{Duration, Instant};

        let child = std::process::Command::new("cmd.exe")
            .args(["/C", "ping", "-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .spawn()
            .expect("spawn cmd.exe");
        let mut managed = ManagedChild::adopt(child);
        assert!(
            managed.job.is_some(),
            "the spawned child must be assigned to a job"
        );

        // Let `cmd.exe` get as far as starting `timeout.exe`, so the tree is
        // genuinely two deep when the kill lands.
        let deadline = Instant::now() + Duration::from_secs(5);
        while managed.active_processes() < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            managed.active_processes() >= 2,
            "cmd.exe should have spawned timeout.exe into the same job"
        );

        managed.kill().expect("terminate the job");
        let status = managed.wait().expect("wait");
        assert!(
            !status.success(),
            "a killed child does not exit successfully"
        );
        // The kernel counts every process still in the job — zero means the
        // grandchild went too, which `Child::kill` alone would never achieve.
        assert_eq!(
            managed.active_processes(),
            0,
            "TerminateJobObject must leave no process in the job"
        );
    }
}
