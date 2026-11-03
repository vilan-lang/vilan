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
//! A plain `vilan run` (not `--watch`) is the one wait that is not a restart
//! round's: [`run_to_end`] spawns the program, waits for it, and on unix
//! forwards a `SIGTERM`, `SIGHUP` or `SIGINT` the CLI is sent to it (E335) - a
//! supervisor, an editor task or a harness that stops `vilan run` stops the
//! program with it, and the program's own death by a signal is what the run
//! then reports (128 + the signal, as a shell does). A `--watch` round's child
//! is registered too (E341), in the other mode: the signal is forwarded to it,
//! the session waits up to [`WATCH_CHILD_GRACE`] for it to end (and kills it
//! past that), and then ends as an interrupted session always has - exit 130.
//! Before, a supervisor's `SIGTERM` ended the watcher and left the round's
//! program holding its port. Restarts keep their own path (`ManagedChild::kill`
//! on the previous round's child).
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
    /// is nothing to reap for them.) A child the signal pipeline holds is taken
    /// off it FIRST: once reaped its pid may be reused, and a late forwarded
    /// signal must not find it.
    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        #[cfg(unix)]
        forward::forget(self.child.id() as libc::pid_t);
        self.child.wait()
    }
}

/// How long an ending `--watch` session waits for its round's child after
/// forwarding it the signal that ends the session (E341), before it kills the
/// child: long enough for a server's graceful stop (F45) to drain, short enough
/// that a supervisor's stop is not left waiting on a program that ignores it.
#[cfg(unix)]
pub const WATCH_CHILD_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Spawns a `--watch` round's child and registers it with the signal pipeline
/// in [`forward::Mode::Exit`] (E341): a `SIGTERM`, `SIGHUP` or `SIGINT` that
/// ends the session reaches the program too. `spawn` runs under the registry's
/// lock, as [`run_to_end`]'s does, so a signal that lands while the child is
/// being started is handled once it exists rather than by the bare hook.
#[cfg(unix)]
pub fn spawn_watched(
    spawn: impl FnOnce() -> std::io::Result<ManagedChild>,
) -> std::io::Result<ManagedChild> {
    let mut registered = forward::registry();
    let child = spawn()?;
    *registered = Some((child.child.id() as libc::pid_t, forward::Mode::Exit));
    Ok(child)
}

/// The non-unix form: the Job object [`ManagedChild::adopt`] assigns takes the
/// round's tree when the session dies.
#[cfg(not(unix))]
pub fn spawn_watched(
    spawn: impl FnOnce() -> std::io::Result<ManagedChild>,
) -> std::io::Result<ManagedChild> {
    spawn()
}

/// Spawns the program a plain `vilan run` executes, waits for it to end and
/// answers its status. `spawn` runs under the forwarding registry's lock
/// (unix), so a signal that arrives while the child is being started is
/// forwarded to it once it exists rather than answered by the interrupt hook
/// that would leave it behind.
#[cfg(unix)]
pub fn run_to_end(
    spawn: impl FnOnce() -> std::io::Result<ManagedChild>,
) -> std::io::Result<ExitStatus> {
    let mut child = {
        let mut registered = forward::registry();
        let child = spawn()?;
        *registered = Some((child.child.id() as libc::pid_t, forward::Mode::Wait));
        child
    };
    forward::wait_unreaped(child.child.id() as libc::pid_t);
    // Off the registry BEFORE the reaping `wait`: once the child is reaped its
    // pid may be reused, and a late forwarded signal must not find it.
    *forward::registry() = None;
    child.wait()
}

/// The non-unix form: the Job object [`ManagedChild::adopt`] assigns is the
/// forwarding (the CLI's death takes the tree).
#[cfg(not(unix))]
pub fn run_to_end(
    spawn: impl FnOnce() -> std::io::Result<ManagedChild>,
) -> std::io::Result<ExitStatus> {
    spawn()?.wait()
}

/// What a signal-ended child reports as an exit code: 128 + the signal's
/// number, as a shell does. `None` for a normal exit (or off unix).
#[cfg(unix)]
pub fn signal_exit_code(status: &ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map(|signal| 128 + signal)
}

#[cfg(not(unix))]
pub fn signal_exit_code(_status: &ExitStatus) -> Option<i32> {
    None
}

/// The unix signal pipeline (E335): `SIGINT`, `SIGTERM` and `SIGHUP` are caught
/// by a handler that only writes the signal's number to a pipe (the one thing a
/// handler may do), and a thread reads the pipe and either forwards the signal
/// to the registered child - a plain run's, or a watch round's (E341) - or,
/// with none registered (a compile, a watch session between rounds), hands it
/// to the interrupt hook, which removes the temp script and exits 130, as it
/// did when `ctrlc` installed the handler (`ctrlc` hands its closure no signal
/// number, which forwarding needs).
#[cfg(unix)]
mod forward {
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::{Mutex, MutexGuard, Once, PoisonError};
    use std::time::{Duration, Instant};

    /// What a signal does with the registered child.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Mode {
        /// A plain `vilan run`: forward it and keep waiting - the program's own
        /// end is the run's.
        Wait,
        /// A `--watch` round's child: forward it, wait up to
        /// [`super::WATCH_CHILD_GRACE`] for the child to end (killing it past
        /// that), then hand the signal to the interrupt hook - the session ends.
        Exit,
    }

    /// The child the CLI is waiting on, and in which mode (`None` outside one).
    static CHILD: Mutex<Option<(libc::pid_t, Mode)>> = Mutex::new(None);

    /// The pipe's write end the handler writes to (-1 before [`install`]).
    static PIPE_WRITE: AtomicI32 = AtomicI32::new(-1);

    pub fn registry() -> MutexGuard<'static, Option<(libc::pid_t, Mode)>> {
        CHILD.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes `pid` off the registry if it is the child registered there.
    pub fn forget(pid: libc::pid_t) {
        let mut registered = registry();
        if registered.is_some_and(|(held, _)| held == pid) {
            *registered = None;
        }
    }

    /// Whether `pid` has exited, without reaping it and without blocking.
    fn exited_unreaped(pid: libc::pid_t) -> bool {
        // SAFETY: `siginfo_t` is plain data (zeroed is a valid value) and the
        // pointer is to a live local of that type; `si_pid` reads the field
        // `waitid` fills (zero when no child changed state).
        unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            let waited = libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
            );
            // An error (the child is not ours any more) ends the wait too.
            waited != 0 || info.si_pid() != 0
        }
    }

    /// Waits for a forwarded-to watch child to end, up to the grace period,
    /// killing it past that, and reaps it - so the session that exits next
    /// leaves no process behind, not even one for init to collect. Called with
    /// the registry locked, so nothing else reaps the pid first; the round's
    /// `ManagedChild` is never waited on again, because the session exits.
    fn end_watched(pid: libc::pid_t) {
        let deadline = Instant::now() + super::WATCH_CHILD_GRACE;
        while !exited_unreaped(pid) {
            if Instant::now() >= deadline {
                // SAFETY: a plain `kill` on the unreaped child.
                let _ = unsafe { libc::kill(pid, libc::SIGKILL) };
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        loop {
            // SAFETY: `waitpid` on our own child, with a null status pointer.
            let reaped = unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
            if reaped >= 0
                || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return;
            }
        }
    }

    /// Blocks until the child has exited WITHOUT reaping it (`WNOWAIT`), so the
    /// pid stays this process's own until the caller has left the registry.
    /// Any failure returns at once; the caller's `wait` is the authority.
    pub fn wait_unreaped(pid: libc::pid_t) {
        loop {
            // SAFETY: `siginfo_t` is plain data (zeroed is a valid value) and
            // the pointer is to a live local of that type.
            let waited = unsafe {
                let mut info: libc::siginfo_t = std::mem::zeroed();
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOWAIT,
                )
            };
            if waited == 0 {
                return;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return;
            }
        }
    }

    /// The handler: one two-byte message, the signal's number and whether the
    /// kernel raised it (a terminal's `Ctrl-C`, which the whole foreground
    /// group - the child included - has already received).
    extern "C" fn on_signal(
        signal: libc::c_int,
        info: *mut libc::siginfo_t,
        _context: *mut libc::c_void,
    ) {
        // SAFETY: the kernel passes a valid `siginfo_t` for an `SA_SIGINFO`
        // handler; `write` is async-signal-safe and `errno` is restored.
        unsafe {
            let saved = *errno_location();
            #[cfg(any(target_os = "linux", target_os = "android"))]
            let from_kernel = !info.is_null() && (*info).si_code == libc::SI_KERNEL;
            #[cfg(not(any(target_os = "linux", target_os = "android")))]
            let from_kernel = {
                let _ = info;
                false
            };
            let message = [signal as u8, from_kernel as u8];
            let fd = PIPE_WRITE.load(Ordering::Relaxed);
            if fd >= 0 {
                libc::write(fd, message.as_ptr().cast(), message.len());
            }
            *errno_location() = saved;
        }
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe fn errno_location() -> *mut libc::c_int {
        unsafe { libc::__errno_location() }
    }

    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    unsafe fn errno_location() -> *mut libc::c_int {
        unsafe { libc::__error() }
    }

    /// Installs the handlers once; `unclaimed(signal)` runs on the reader
    /// thread for a signal that arrives with no child to forward it to. Best
    /// effort, as the `ctrlc` install it replaces was: a run is not worth
    /// failing over a handler the OS refused.
    pub fn install(unclaimed: fn(i32)) {
        static INSTALLED: Once = Once::new();
        INSTALLED.call_once(|| {
            let mut fds = [0 as libc::c_int; 2];
            // SAFETY: `fds` is a two-int array, as `pipe` wants.
            if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
                return;
            }
            for fd in fds {
                // SAFETY: the two descriptors were just created and are ours.
                unsafe {
                    libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                }
            }
            // SAFETY: as above; the handler must never block.
            unsafe {
                let flags = libc::fcntl(fds[1], libc::F_GETFL);
                libc::fcntl(fds[1], libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
            PIPE_WRITE.store(fds[1], Ordering::Relaxed);
            let read_end = fds[0];
            let spawned = std::thread::Builder::new()
                .name("vilan-signals".to_string())
                .spawn(move || reader(read_end, unclaimed));
            if spawned.is_err() {
                return;
            }
            for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
                // SAFETY: a zeroed `sigaction` is a valid starting value; the
                // handler is an `extern "C"` function of the `SA_SIGINFO`
                // shape.
                unsafe {
                    let mut action: libc::sigaction = std::mem::zeroed();
                    action.sa_sigaction = on_signal as *const () as usize;
                    action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
                    libc::sigemptyset(&mut action.sa_mask);
                    libc::sigaction(signal, &action, std::ptr::null_mut());
                }
            }
        });
    }

    /// The reader thread: forwards each signal to the registered child, or
    /// hands it to `unclaimed` when there is none.
    fn reader(read_end: libc::c_int, unclaimed: fn(i32)) {
        let mut message = [0u8; 2];
        loop {
            // SAFETY: reads into a live two-byte buffer.
            let read = unsafe { libc::read(read_end, message.as_mut_ptr().cast(), 2) };
            if read == 2 {
                let signal = i32::from(message[0]);
                let from_kernel = message[1] != 0;
                // The registry stays locked across the `kill`: the run clears
                // it before it reaps the child, so the pid is still ours here.
                let guard = registry();
                // A terminal's `Ctrl-C` reached the child with the rest of the
                // foreground group: delivering it again would run a program's
                // SIGINT handler twice.
                let delivered = signal == libc::SIGINT && from_kernel;
                match *guard {
                    Some((_, Mode::Wait)) if delivered => {}
                    Some((pid, Mode::Wait)) => {
                        // SAFETY: a plain `kill` on the unreaped child.
                        let _ = unsafe { libc::kill(pid, signal) };
                    }
                    // E341: a watch session ends with its round's program. The
                    // registry stays locked to the end, so no round can start
                    // another child while the session goes.
                    Some((pid, Mode::Exit)) => {
                        if !delivered {
                            // SAFETY: a plain `kill` on the unreaped child.
                            let _ = unsafe { libc::kill(pid, signal) };
                        }
                        end_watched(pid);
                        unclaimed(signal);
                    }
                    None => {
                        drop(guard);
                        unclaimed(signal);
                    }
                }
            } else if read < 0
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
            {
                continue;
            } else {
                return;
            }
        }
    }
}

#[cfg(unix)]
pub use forward::install as install_signal_forwarding;

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

    // SAFETY: a kernel handle is just a value; this type never shares it, so
    // moving a `ManagedChild` (which the watch loop does, round to round) is
    // safe, and every operation on the handle is kernel-side thread-safe.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        /// Creates an anonymous kill-on-close job and assigns `child` to it.
        /// `None` on any failure — see [`super::ManagedChild::job`].
        pub fn create_and_assign(child: &Child) -> Option<Job> {
            // SAFETY: plain WinAPI FFI. The created handle is null-checked and
            // owned by `Job` before any early return; `zeroed()` is a valid
            // value for the all-integer `#[repr(C)]` limits struct; the raw
            // child handle stays valid under the `&Child` borrow.
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
            // SAFETY: the handle is non-null by construction (checked at create).
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
            // SAFETY: the out-parameters are live locals of the exact sizes the
            // call is told, and the handle is non-null by construction.
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
            // SAFETY: the owned handle is non-null by construction and closed
            // exactly once — nothing else stores or closes it.
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
