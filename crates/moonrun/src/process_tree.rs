// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Supervision and guest process identity for concurrent Runs.

mod guest;
mod moon;
#[cfg(windows)]
pub(crate) use guest::ProcessHandle;
pub(crate) use guest::{ProcessContext, ProcessWait};

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use thiserror::Error;

use crate::engine::SUPPORTS_RUN_TERMINATION;
use crate::{Engine, Module, RunOptions, RunOutcome, RunTermination, RunTerminator};
use crate::{WorkingDirectory, termination_channel};

/// A process identity in moonrun's virtual namespace, never an OS PID.
///
/// Values occupy the upper half of positive `i32`s and are not reused yet.
/// This range is only a convention: safety comes from table lookup, never from
/// assuming the OS cannot allocate the same integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtualPid(i32);

// Allocate across trees so a handle from another tree cannot alias a local Run.
static NEXT_PID: AtomicU32 = AtomicU32::new(1 << 30);

impl VirtualPid {
    pub fn get(self) -> i32 {
        self.0
    }

    fn allocate(next: &AtomicU32) -> Result<Self, ProcessError> {
        next.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            (value <= i32::MAX as u32).then(|| value + 1)
        })
        .map(|value| Self(value as i32))
        .map_err(|_| ProcessError::PidExhausted)
    }
}

impl std::fmt::Display for VirtualPid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState {
    /// Admitted for execution, including startup and ordinary Run teardown.
    Running,
    /// Forced termination was accepted; teardown may still be blocked.
    Terminating,
    /// The Run has returned or unwound, but has not been reaped by `wait`.
    Exited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: VirtualPid,
    /// `None` denotes a root or a child adopted by the embedding supervisor.
    pub parent: Option<VirtualPid>,
    pub state: ProcessState,
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("the selected engine backend does not support virtual process termination")]
    UnsupportedBackend,
    #[error("virtual process IDs are exhausted")]
    PidExhausted,
    #[error("virtual process stack size must be nonzero and fit in bytes with a host reserve")]
    InvalidStackSize,
    #[error("unknown virtual process {0}")]
    NotFound(VirtualPid),
    #[error("virtual parent {0} is no longer running")]
    ParentNotRunning(VirtualPid),
    #[error("another caller is already waiting for virtual process {0}")]
    AlreadyWaiting(VirtualPid),
    #[error("the virtual process tree owns the Run termination receiver")]
    TerminationAlreadyConfigured,
    #[error("Moon handoff owns the program arguments; pass them through the Moon command")]
    MoonArgumentsAlreadyConfigured,
    #[error("failed to prepare or start a virtual process")]
    Spawn(#[from] std::io::Error),
    #[error("virtual process Run failed")]
    RunFailed(#[source] anyhow::Error),
    #[error("virtual process {0} panicked")]
    Panicked(VirtualPid),
}

struct Entry {
    info: ProcessInfo,
    terminator: RunTerminator,
    signals: crate::SignalSender,
    completion: Arc<ProcessCompletion>,
    waiting: bool,
}

#[derive(Debug, Default)]
struct ProcessCompletion {
    // Wait/cancel may lock this independently of the table. Never acquire the
    // table or a termination lock while holding it.
    result: Mutex<Option<Result<RunOutcome, ProcessError>>>,
    changed: Condvar,
    // Windows handles retain the exit status even after a supervisor takes
    // the owned Run result and removes the process from the tree.
    #[cfg(windows)]
    exit_code: std::sync::OnceLock<i32>,
    #[cfg(target_os = "macos")]
    // Observations belong to the waiting Run. The tree must not keep its
    // notification resources alive after that Run closes its poll.
    notifications: Mutex<Vec<std::sync::Weak<std::task::Waker>>>,
}

impl ProcessCompletion {
    fn publish(&self, result: Result<RunOutcome, ProcessError>) {
        let mut slot = self.result.lock().unwrap();
        #[cfg(windows)]
        self.exit_code.set(guest::exit_status(&result)).unwrap();
        *slot = Some(result);
        #[cfg(target_os = "macos")]
        let notifications: Vec<_> = std::mem::take(&mut *self.notifications.lock().unwrap())
            .into_iter()
            .filter_map(|notification| notification.upgrade())
            .collect();
        drop(slot);
        self.changed.notify_all();
        // Listeners choose their transport. Never call them with completion
        // locks held; collecting them under the result lock prevents lost wakes.
        #[cfg(target_os = "macos")]
        for notification in notifications {
            notification.wake_by_ref();
        }
    }

    // Reserve a separate reaper before starting the Run: thread creation may
    // fail, and joining after Run exit can still block in native TLS cleanup.
    // Even is_finished() can be true before those destructors have returned.
    // Dropping the sender without a thread cancels this unused reservation.
    fn start_reaper(
        self: &Arc<Self>,
        pid: VirtualPid,
    ) -> std::io::Result<mpsc::Sender<JoinHandle<anyhow::Result<RunOutcome>>>> {
        let (sender, receiver) = mpsc::channel::<JoinHandle<anyhow::Result<RunOutcome>>>();
        let completion = Arc::clone(self);
        thread::Builder::new()
            .name(format!("moonrun-reaper-{pid}"))
            .spawn(move || {
                let Ok(thread) = receiver.recv() else {
                    return;
                };
                let result = match thread.join() {
                    Ok(result) => result.map_err(ProcessError::RunFailed),
                    Err(_) => Err(ProcessError::Panicked(pid)),
                };
                completion.publish(result);
            })?;
        Ok(sender)
    }
}

type Table = Arc<Mutex<BTreeMap<VirtualPid, Entry>>>;

// A Windows spawn must pin completion before another supervisor can reap its
// PID. Publishing the guest HANDLE later must not depend on a new PID lookup.
pub(crate) struct SpawnedProcess {
    pub(crate) pid: VirtualPid,
    #[cfg(windows)]
    completion: Arc<ProcessCompletion>,
}

/// Opt-in host-side lifecycle management for Runs on the supplied Engine.
///
/// Each Run gets a thread, an owned environment (policy provisioning takes
/// precedence), and a fixed cwd. Parent links do not imply configuration
/// inheritance. Guests see their virtual PID and can signal or wait for their
/// children through the existing async process API, including recursive Moon
/// invocations. Other native spawning is not yet supported in this mode;
/// ordinary Engine Runs retain ambient process access.
/// Native stacks retain at least the creator's size; Run startup checks the
/// guest budget against the executing thread's remaining native stack.
///
/// The supervisor can wait for any member. Exiting parents leave children
/// running under the supervisor. Dropping the tree neither stops nor joins
/// Runs: callers must explicitly terminate and wait when required.
#[derive(Clone)]
pub struct VirtualProcessTree {
    engine: Engine,
    table: Table,
    thread_stack_size: usize,
    guest_stack_budget: usize,
}

impl VirtualProcessTree {
    /// Requires external Run termination support, currently available on V8.
    pub fn new(engine: Engine) -> Result<Self, ProcessError> {
        if !SUPPORTS_RUN_TERMINATION {
            return Err(ProcessError::UnsupportedBackend);
        }
        // Preserve the creator's actual native stack size, enlarging it when
        // the guest budget requires more room. Each supervised Run checks its
        // executing thread's remaining stack separately, including host reserve.
        // The pinned V8's default guest limit fits in 1 MiB.
        let guest_stack_budget = engine
            .config
            .stack_size
            .unwrap_or(1024)
            .checked_mul(1024)
            .filter(|bytes| *bytes != 0)
            .ok_or(ProcessError::InvalidStackSize)?;
        let thread_stack_size = guest_stack_budget
            .checked_add(2 * 1024 * 1024)
            .ok_or(ProcessError::InvalidStackSize)?;
        let (low, high) = crate::native_stack::bounds()?;
        let native_stack_size = high
            .checked_sub(low)
            .ok_or_else(|| std::io::Error::other("invalid native thread stack bounds"))?;
        Ok(Self {
            engine,
            table: Arc::new(Mutex::new(BTreeMap::new())),
            thread_stack_size: thread_stack_size.max(native_stack_size),
            guest_stack_budget,
        })
    }

    /// Start a Run with an optional live parent from this tree.
    ///
    /// Absent explicit provisioning, capture the host environment and cwd at
    /// spawn. Stdio follows `options`. Host setup errors after thread creation
    /// are returned by `wait`, just like other Run failures.
    pub fn spawn(
        &self,
        parent: Option<VirtualPid>,
        module: &Module,
        options: RunOptions,
    ) -> Result<VirtualPid, ProcessError> {
        self.spawn_with_signal_receiver(parent, module, options, crate::signal_channel().1)
    }

    /// Start a Run using an external signal channel, for example a CLI's
    /// process-signal adapter. The tree retains a sender for guest control.
    pub fn spawn_with_signal_receiver(
        &self,
        parent: Option<VirtualPid>,
        module: &Module,
        options: RunOptions,
        signals: crate::SignalReceiver,
    ) -> Result<VirtualPid, ProcessError> {
        let module = module.clone();
        self.spawn_run(parent, options, signals, move |engine, options, signals| {
            engine.run_with_signal_receiver(&module, options, signals)
        })
        .map(|spawned| spawned.pid)
    }

    /// Ask a host-selected Moon or moonx executable to prepare Wasm, then run it
    /// under the PID allocated here. `args` are Moon's arguments, including
    /// `run` for Moon; pass program arguments there rather than in `options`.
    ///
    /// Build/download helpers retain host access. Termination during preparation
    /// is retained until the helper exits and prevents subsequent Wasm execution.
    pub fn spawn_moon(
        &self,
        parent: Option<VirtualPid>,
        executable: &Path,
        args: &[OsString],
        options: RunOptions,
    ) -> Result<VirtualPid, ProcessError> {
        if !options.args.is_empty() || options.test_args.is_some() {
            return Err(ProcessError::MoonArgumentsAlreadyConfigured);
        }
        let executable = executable.to_owned();
        let args = args.to_vec();
        self.spawn_run(
            parent,
            options,
            crate::signal_channel().1,
            move |engine, options, signals| {
                moon::run(
                    engine,
                    &executable,
                    &args,
                    options,
                    signals,
                    #[cfg(windows)]
                    false,
                )
            },
        )
        .map(|spawned| spawned.pid)
    }

    fn spawn_run(
        &self,
        parent: Option<VirtualPid>,
        mut options: RunOptions,
        signal_receiver: crate::SignalReceiver,
        run: impl FnOnce(&Engine, RunOptions, crate::SignalReceiver) -> anyhow::Result<RunOutcome>
        + Send
        + 'static,
    ) -> Result<SpawnedProcess, ProcessError> {
        if options.termination_receiver.is_some() {
            return Err(ProcessError::TerminationAlreadyConfigured);
        }
        options.native_stack_budget = Some(self.guest_stack_budget);
        options
            .default_environment
            .get_or_insert_with(|| std::env::vars_os().collect());
        if matches!(options.working_directory, WorkingDirectory::Ambient) {
            options.working_directory = WorkingDirectory::Fixed(std::env::current_dir()?);
        }
        let mut table = self.table.lock().unwrap();
        if let Some(parent) = parent {
            let entry = table.get(&parent).ok_or(ProcessError::NotFound(parent))?;
            if entry.info.state != ProcessState::Running {
                return Err(ProcessError::ParentNotRunning(parent));
            }
        }
        let pid = VirtualPid::allocate(&NEXT_PID)?;
        options.process_context = Some(ProcessContext {
            pid,
            tree: self.clone(),
        });
        let (terminator, receiver) = termination_channel();
        let signals = signal_receiver.sender();
        options.termination_receiver = Some(receiver);
        let engine = self.engine.clone();
        let shared_table = Arc::clone(&self.table);
        let completion = Arc::new(ProcessCompletion::default());
        let reaper = completion.start_reaper(pid)?;
        // Keep admission locked through publication. Even an immediate return
        // or panic must find its entry, and a parent cannot exit in between.
        let thread = thread::Builder::new()
            .name(format!("moonrun-{pid}"))
            .stack_size(self.thread_stack_size)
            .spawn(move || {
                let _exit = ProcessExitGuard {
                    table: shared_table,
                    pid,
                };
                run(&engine, options, signal_receiver)
            })?;
        let spawned = SpawnedProcess {
            pid,
            #[cfg(windows)]
            completion: Arc::clone(&completion),
        };
        table.insert(
            pid,
            Entry {
                info: ProcessInfo {
                    pid,
                    parent,
                    state: ProcessState::Running,
                },
                terminator,
                signals,
                completion,
                waiting: false,
            },
        );
        reaper
            .send(thread)
            .expect("process reaper must accept its Run");
        Ok(spawned)
    }

    /// Request forced termination of one member, without affecting descendants.
    /// Acceptance is not completion; synchronous host I/O may delay `wait`.
    /// Unknown IDs are errors and are never forwarded to the operating system.
    pub fn terminate(
        &self,
        pid: VirtualPid,
        outcome: RunTermination,
    ) -> Result<bool, ProcessError> {
        let mut table = self.table.lock().unwrap();
        let entry = table.get_mut(&pid).ok_or(ProcessError::NotFound(pid))?;
        // Lock order is table then termination. Run completion acquires the
        // table only after Engine::run has closed the termination channel.
        let accepted = entry.terminator.terminate(outcome);
        if accepted {
            entry.info.state = ProcessState::Terminating;
        }
        Ok(accepted)
    }

    /// Join and reap exactly once, including Run failures and thread panics.
    /// No table lock is held while waiting for Run and thread teardown.
    pub fn wait(&self, pid: VirtualPid) -> Result<RunOutcome, ProcessError> {
        let claim = WaitClaim::new(Arc::clone(&self.table), pid)?;
        let mut result = claim.completion.result.lock().unwrap();
        while result.is_none() {
            result = claim.completion.changed.wait(result).unwrap();
        }
        let outcome = result.take().unwrap();
        drop(result);
        self.table.lock().unwrap().remove(&pid);
        drop(claim);
        outcome
    }

    /// A coherent snapshot, including exited members awaiting reaping.
    pub fn snapshot(&self) -> Vec<ProcessInfo> {
        self.table
            .lock()
            .unwrap()
            .values()
            .map(|entry| entry.info)
            .collect()
    }
}

// Mark exit on panic too, after Engine::run's stack has unwound. The reaper
// publishes completion separately, after native thread cleanup and join.
// The thread retains the table even if the embedding owner drops its tree.
struct ProcessExitGuard {
    table: Table,
    pid: VirtualPid,
}

impl Drop for ProcessExitGuard {
    fn drop(&mut self) {
        let mut table = self.table.lock().unwrap();
        table.get_mut(&self.pid).unwrap().info.state = ProcessState::Exited;
        for entry in table.values_mut() {
            if entry.info.parent == Some(self.pid) {
                entry.info.parent = None;
            }
        }
    }
}

// A completed guest Job retains its claim until the guest reads the result.
// Dropping/cancelling an unconsumed Job releases it without losing the exit.
struct WaitClaim {
    table: Table,
    pid: VirtualPid,
    completion: Arc<ProcessCompletion>,
}

impl WaitClaim {
    fn new(table: Table, pid: VirtualPid) -> Result<Self, ProcessError> {
        let completion = {
            let mut entries = table.lock().unwrap();
            let entry = entries.get_mut(&pid).ok_or(ProcessError::NotFound(pid))?;
            if entry.waiting {
                return Err(ProcessError::AlreadyWaiting(pid));
            }
            entry.waiting = true;
            Arc::clone(&entry.completion)
        };
        Ok(Self {
            table,
            pid,
            completion,
        })
    }
}

impl Drop for WaitClaim {
    fn drop(&mut self) {
        if let Some(entry) = self.table.lock().unwrap().get_mut(&self.pid) {
            entry.waiting = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_exhaustion_never_wraps_or_reuses_an_id() {
        let next = AtomicU32::new(i32::MAX as u32);
        assert_eq!(VirtualPid::allocate(&next).unwrap().get(), i32::MAX);
        for _ in 0..2 {
            assert!(matches!(
                VirtualPid::allocate(&next),
                Err(ProcessError::PidExhausted)
            ));
        }
    }

    #[cfg(feature = "v8")]
    #[test]
    fn invalid_stack_budgets_are_rejected_before_thread_creation() {
        for stack_size in [0, usize::MAX] {
            assert!(matches!(
                VirtualProcessTree::new(Engine::new(
                    crate::EngineConfig::default().with_stack_size(stack_size)
                )),
                Err(ProcessError::InvalidStackSize)
            ));
        }
    }

    #[cfg(feature = "v8")]
    #[test]
    fn supervisor_preserves_the_creating_threads_native_stack_size() {
        thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let (low, high) = crate::native_stack::bounds().unwrap();
                let tree = VirtualProcessTree::new(Engine::default()).unwrap();
                assert!(tree.thread_stack_size >= high - low);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[cfg(feature = "v8")]
    #[test]
    fn undersized_native_stack_is_rejected_before_guest_execution() {
        use std::time::Duration;

        if std::env::var_os("MOONRUN_SMALL_NATIVE_STACK_TEST").is_none() {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process_tree::tests::undersized_native_stack_is_rejected_before_guest_execution",
                    "--nocapture",
                ])
                .env("MOONRUN_SMALL_NATIVE_STACK_TEST", "1")
                .timeout(Duration::from_secs(15))
                .assert()
                .success();
            return;
        }
        let engine = Engine::default();
        let module = engine
            .compile(
                "recursive.wasm",
                wat::parse_str(
                    r#"(module
            (func $recurse (result i32) call $recurse i32.const 1 i32.add)
            (func (export "_start") call $recurse drop))"#,
                )
                .unwrap(),
            )
            .unwrap();
        let mut tree = VirtualProcessTree::new(engine).unwrap();
        // Bypass creation-time sizing to verify that the executing thread
        // checks its own native stack before entering V8.
        tree.thread_stack_size = 512 * 1024;
        let pid = tree.spawn(None, &module, RunOptions::default()).unwrap();
        assert!(matches!(tree.wait(pid), Err(ProcessError::RunFailed(error))
            if error.to_string().contains("native thread stack")));
        assert!(tree.snapshot().is_empty());
    }

    #[cfg(feature = "v8")]
    #[test]
    fn termination_is_not_completion_and_panics_are_reaped() {
        use std::sync::mpsc;
        use std::time::Duration;

        struct ThreadCleanup(mpsc::Receiver<()>);
        impl Drop for ThreadCleanup {
            fn drop(&mut self) {
                self.0.recv().unwrap();
            }
        }
        thread_local! {
            static CLEANUP: std::cell::RefCell<Option<ThreadCleanup>> = const { std::cell::RefCell::new(None) };
        }

        if std::env::var_os("MOONRUN_PENDING_PROCESS_TEST").is_none() {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process_tree::tests::termination_is_not_completion_and_panics_are_reaped",
                    "--nocapture",
                ])
                .env("MOONRUN_PENDING_PROCESS_TEST", "1")
                .timeout(Duration::from_secs(15))
                .assert()
                .success();
            return;
        }
        for (panic, native_tls) in [(false, false), (true, false), (false, true), (true, true)] {
            let tree = Arc::new(VirtualProcessTree::new(Engine::default()).unwrap());
            let pid = VirtualPid::allocate(&NEXT_PID).unwrap();
            let (terminator, receiver) = termination_channel();
            let (release, blocked) = mpsc::channel();
            let (finish_thread, cleanup) = mpsc::channel();
            let table = Arc::clone(&tree.table);
            let process_completion = Arc::new(ProcessCompletion::default());
            let reaper = process_completion.start_reaper(pid).unwrap();
            let mut entries = tree.table.lock().unwrap();
            let thread = thread::spawn(move || {
                let _cleanup = if native_tls {
                    CLEANUP.set(Some(ThreadCleanup(cleanup)));
                    None
                } else {
                    // Declared before the exit guard so cleanup blocks after
                    // Run exit is recorded, including during unwinding.
                    Some(ThreadCleanup(cleanup))
                };
                let _exit = ProcessExitGuard { table, pid };
                // Model host work that cannot yet return. A termination
                // request must not destroy it or make wait report success.
                blocked.recv().unwrap();
                assert!(!panic, "simulated host panic");
                let outcome = receiver.close().unwrap();
                assert_eq!(outcome, RunTermination::Exit(7));
                Ok(RunOutcome::Exited(7))
            });
            entries.insert(
                pid,
                Entry {
                    info: ProcessInfo {
                        pid,
                        parent: None,
                        state: ProcessState::Running,
                    },
                    terminator,
                    signals: crate::signal_channel().0,
                    completion: process_completion,
                    waiting: false,
                },
            );
            reaper.send(thread).unwrap();
            drop(entries);
            assert!(tree.terminate(pid, RunTermination::Exit(7)).unwrap());
            assert!(!tree.terminate(pid, RunTermination::Exit(8)).unwrap());
            assert_eq!(tree.snapshot()[0].state, ProcessState::Terminating);
            let module = tree
                .engine
                .compile("empty.wasm", wat::parse_str("(module)").unwrap())
                .unwrap();
            assert!(matches!(
                tree.spawn(Some(pid), &module, RunOptions::default()),
                Err(ProcessError::ParentNotRunning(_))
            ));
            let waiting_tree = Arc::clone(&tree);
            let (done, completion) = mpsc::channel();
            let waiter = thread::spawn(move || done.send(waiting_tree.wait(pid)).unwrap());
            // Wait until the waiter reserves the result, without permitting
            // the simulated host work to finish.
            while !tree.table.lock().unwrap()[&pid].waiting {
                thread::yield_now();
            }
            assert_eq!(tree.snapshot()[0].state, ProcessState::Terminating);
            assert!(matches!(
                completion.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));
            release.send(()).unwrap();
            while tree.snapshot()[0].state != ProcessState::Exited {
                thread::yield_now();
            }
            // Run exit is recorded, but cleanup still blocks thread teardown.
            // Only the completed join may publish the result.
            assert!(matches!(
                completion.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));

            // Windows TLS destructors hold the OS loader lock, which itself
            // blocks other threads' startup/exit. Check independent reaping
            // with ordinary post-Run cleanup there; retain the real TLS case
            // above to verify that join still waits for those destructors.
            let other = if cfg!(windows) && native_tls {
                None
            } else {
                let other = tree.spawn(None, &module, RunOptions::default()).unwrap();
                let other_tree = Arc::clone(&tree);
                let (other_done, other_completion) = mpsc::channel();
                let other_waiter = thread::spawn(move || {
                    other_done.send(other_tree.wait(other)).unwrap();
                });
                Some((
                    other_waiter,
                    other_completion.recv_timeout(Duration::from_secs(5)),
                ))
            };
            let pending_result = completion.try_recv();
            finish_thread.send(()).unwrap();
            assert!(matches!(pending_result, Err(mpsc::TryRecvError::Empty)));
            if let Some((other_waiter, other_result)) = other {
                other_waiter.join().unwrap();
                assert_eq!(
                    other_result
                        .expect("unrelated completion blocked by thread cleanup")
                        .unwrap(),
                    RunOutcome::Completed
                );
            }
            let result = completion.recv_timeout(Duration::from_secs(5)).unwrap();
            if panic {
                assert!(matches!(result, Err(ProcessError::Panicked(found)) if found == pid));
            } else {
                assert_eq!(result.unwrap(), RunOutcome::Exited(7));
            }
            waiter.join().unwrap();
            assert!(tree.snapshot().is_empty());
        }
    }

    #[cfg(feature = "v8")]
    #[test]
    fn dropping_the_supervisor_preserves_live_runs_until_teardown() {
        use std::time::Duration;

        if std::env::var_os("MOONRUN_DROPPED_TREE_TEST").is_none() {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process_tree::tests::dropping_the_supervisor_preserves_live_runs_until_teardown",
                    "--nocapture",
                ])
                .env("MOONRUN_DROPPED_TREE_TEST", "1")
                .timeout(Duration::from_secs(15))
                .assert()
                .success();
            return;
        }
        let engine = Engine::default();
        let module = engine
            .compile(
                "loop.wasm",
                wat::parse_str(r#"(module (func (export "_start") (loop br 0)))"#).unwrap(),
            )
            .unwrap();
        let tree = VirtualProcessTree::new(engine).unwrap();
        let pid = tree.spawn(None, &module, RunOptions::default()).unwrap();
        let terminator = tree.table.lock().unwrap()[&pid].terminator.clone();
        let table = Arc::downgrade(&tree.table);
        drop(tree);
        assert!(table.upgrade().is_some());
        assert!(terminator.terminate(RunTermination::Exit(0)));
        while table.upgrade().is_some() {
            thread::yield_now();
        }
        assert!(!terminator.terminate(RunTermination::Exit(1)));
    }
}
