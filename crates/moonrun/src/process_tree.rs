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

//! Execution admission, control, and reaping owned by the Engine.

mod guest;
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

use crate::engine::{GuestProcessMode, SUPPORTS_RUN_TERMINATION};
use crate::run_termination::{RunTerminationReceiver, TerminationRequest};
use crate::{Engine, Module, RunOptions, RunOutcome, RunTermination, RunTerminator};
use crate::{WorkingDirectory, termination_channel};

/// A process identity in moonrun's virtual namespace, never an OS PID.
///
/// Values occupy the upper half of positive `i32`s and are not reused yet.
/// This range is only a convention: safety comes from table lookup, never from
/// assuming the OS cannot allocate the same integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtualPid(i32);

// Allocate across Engines so an ID from another host cannot alias a local Run.
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
    /// Admitted for execution, including preparation and execution cleanup.
    Running,
    /// Termination was requested; teardown may still be blocked.
    Terminating,
    /// Execution returned or unwound; thread reaping may still be pending.
    Exited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: VirtualPid,
    /// `None` denotes a root or a child adopted by the Engine.
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
    #[error("spawned execution owns the Run termination receiver")]
    TerminationAlreadyConfigured,
    #[error("Moon handoff owns the program arguments; pass them through the Moon command")]
    MoonArgumentsAlreadyConfigured,
    #[error("virtual execution requires an owned environment and fixed working directory")]
    InvalidVirtualConfiguration,
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

impl Entry {
    fn info(&self) -> ProcessInfo {
        let mut info = self.info;
        // A caller-supplied terminator reaches the shared request directly.
        // Derive this state from that request instead of requiring every
        // termination source to update a second copy in the process table.
        if info.state == ProcessState::Running && self.terminator.requested().is_some() {
            info.state = ProcessState::Terminating;
        }
        info
    }
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

/// The sole process table shared by clones of one Engine.
/// Entries contain control and completion, never the Engine or a Runtime.
#[derive(Clone, Default)]
pub(crate) struct ProcessTable(Table);

impl std::fmt::Debug for ProcessTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessTable").finish_non_exhaustive()
    }
}

impl Engine {
    fn guest_stack_budget(&self) -> Result<usize, ProcessError> {
        self.config
            .stack_size
            .unwrap_or(1024)
            .checked_mul(1024)
            .filter(|bytes| *bytes != 0)
            .ok_or(ProcessError::InvalidStackSize)
    }

    /// Start a Run on a new thread, with an optional live parent in this Engine.
    ///
    /// `GuestProcessMode::Virtual` captures the host environment and cwd unless
    /// explicitly provisioned. Native mode preserves ambient host state. Stdio
    /// follows `options`. Failures after admission are returned by `wait`.
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
        let mut options = options;
        options.prepare_root()?;
        let module = module.clone();
        self.spawn_execution(
            parent,
            options,
            signals,
            move |engine, options, signals, termination| {
                engine.run_in_scope(&module, options, signals, termination)
            },
        )
        .map(|spawned| spawned.pid)
    }

    /// Admit a prepared executable through the same dispatch as a Moon helper
    /// result. Native process nodes are not yet implemented.
    pub fn spawn_executable_with_signal_receiver(
        &self,
        parent: Option<VirtualPid>,
        executable: &Path,
        kind: moonutil::run_handoff::ExecutionKind,
        options: RunOptions,
        signals: crate::SignalReceiver,
    ) -> Result<VirtualPid, ProcessError> {
        let mut options = options;
        options.prepare_root()?;
        let executable = executable.to_owned();
        self.spawn_execution(
            parent,
            options,
            signals,
            move |engine, options, signals, termination| {
                engine.execute_artifact_in_scope(
                    kind,
                    &executable,
                    &executable,
                    |bytes| {
                        moonutil::wasm_source_map::load(&executable, bytes)
                            .as_deref()
                            .and_then(crate::source_map::SourceMap::parse)
                    },
                    options,
                    signals,
                    termination,
                )
            },
        )
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
        let mut options = options;
        options.prepare_root()?;
        let executable = executable.to_owned();
        let args = args.to_vec();
        self.spawn_execution(
            parent,
            options,
            crate::signal_channel().1,
            move |engine, options, signals, termination| {
                engine.execute_moon_in_scope(
                    &executable,
                    &args,
                    options,
                    signals,
                    termination,
                    #[cfg(windows)]
                    false,
                )
            },
        )
        .map(|spawned| spawned.pid)
    }

    /// Execute on the calling thread. Virtual execution reserves its own result
    /// so another waiter cannot consume the value this call must return.
    pub(crate) fn run_inline(
        &self,
        mut options: RunOptions,
        signals: crate::SignalReceiver,
        execute: impl FnOnce(
            &Engine,
            RunOptions,
            crate::SignalReceiver,
            &TerminationRequest,
        ) -> anyhow::Result<RunOutcome>,
    ) -> anyhow::Result<RunOutcome> {
        let virtual_process = options.guest_process_mode == GuestProcessMode::Virtual;
        if !SUPPORTS_RUN_TERMINATION {
            if virtual_process {
                return Err(ProcessError::UnsupportedBackend.into());
            }
            anyhow::ensure!(
                options.termination_receiver.is_none(),
                "the selected engine backend does not support external Run termination"
            );
        }
        // The receiver is declared after the exit guard so panic unwinding
        // closes control before removing the PID and adopting its children.
        let mut exit = None;
        let receiver = options.termination_receiver.take().unwrap_or_else(|| {
            if virtual_process {
                termination_channel().1
            } else {
                RunTerminationReceiver {
                    request: Default::default(),
                }
            }
        });
        let result = (|| {
            if let Some(outcome) = receiver.request.get() {
                drop((execute, options, signals));
                return Ok(outcome.into());
            }
            options.prepare_root()?;
            if virtual_process {
                options.native_stack_budget = Some(self.guest_stack_budget()?);
                let pid = VirtualPid::allocate(&NEXT_PID)?;
                options.runtime.process_context = Some(ProcessContext {
                    pid,
                    engine: self.clone(),
                });
                self.processes.0.lock().unwrap().insert(
                    pid,
                    Entry {
                        info: ProcessInfo {
                            pid,
                            parent: None,
                            state: ProcessState::Running,
                        },
                        terminator: receiver.sender(),
                        signals: signals.sender(),
                        completion: Arc::new(ProcessCompletion::default()),
                        waiting: true,
                    },
                );
                exit = Some(ProcessExitGuard {
                    table: Arc::clone(&self.processes.0),
                    pid,
                    reap_on_drop: true,
                });
            } else {
                options.runtime.process_context = None;
            }
            execute(self, options, signals, &receiver.request)
        })();
        let result = match receiver.close() {
            Some(outcome) => Ok(outcome.into()),
            None => result,
        };
        drop(receiver);
        drop(exit);
        result
    }

    pub(crate) fn spawn_execution(
        &self,
        parent: Option<VirtualPid>,
        mut options: RunOptions,
        signal_receiver: crate::SignalReceiver,
        execute: impl FnOnce(
            &Engine,
            RunOptions,
            crate::SignalReceiver,
            &TerminationRequest,
        ) -> anyhow::Result<RunOutcome>
        + Send
        + 'static,
    ) -> Result<SpawnedProcess, ProcessError> {
        if !SUPPORTS_RUN_TERMINATION {
            return Err(ProcessError::UnsupportedBackend);
        }
        if options.termination_receiver.is_some() {
            return Err(ProcessError::TerminationAlreadyConfigured);
        }
        let guest_stack_budget = self.guest_stack_budget()?;
        let required_stack = guest_stack_budget
            .checked_add(2 * 1024 * 1024)
            .ok_or(ProcessError::InvalidStackSize)?;
        let (low, high) = crate::native_stack::bounds()?;
        let native_stack_size = high
            .checked_sub(low)
            .ok_or_else(|| std::io::Error::other("invalid native thread stack bounds"))?;
        let thread_stack_size = required_stack.max(native_stack_size);
        options.native_stack_budget = Some(guest_stack_budget);
        if options.guest_process_mode == GuestProcessMode::Virtual
            && (options.runtime.default_environment.is_none()
                || !matches!(
                    options.runtime.working_directory,
                    WorkingDirectory::Fixed(_)
                ))
        {
            return Err(ProcessError::InvalidVirtualConfiguration);
        }
        let mut table = self.processes.0.lock().unwrap();
        if let Some(parent) = parent {
            let entry = table.get(&parent).ok_or(ProcessError::NotFound(parent))?;
            if entry.info().state != ProcessState::Running {
                return Err(ProcessError::ParentNotRunning(parent));
            }
        }
        let pid = VirtualPid::allocate(&NEXT_PID)?;
        options.runtime.process_context = (options.guest_process_mode == GuestProcessMode::Virtual)
            .then(|| ProcessContext {
                pid,
                engine: self.clone(),
            });
        let (terminator, receiver) = termination_channel();
        let signals = signal_receiver.sender();
        let engine = self.clone();
        let shared_table = Arc::clone(&self.processes.0);
        let completion = Arc::new(ProcessCompletion::default());
        let reaper = completion.start_reaper(pid)?;
        // Keep admission locked through publication. Even an immediate return
        // or panic must find its entry, and a parent cannot exit in between.
        let thread = thread::Builder::new()
            .name(format!("moonrun-{pid}"))
            .stack_size(thread_stack_size)
            .spawn(move || {
                let _exit = ProcessExitGuard {
                    table: shared_table,
                    pid,
                    reap_on_drop: false,
                };
                // Close control before publishing exit, including on unwind.
                let receiver = receiver;
                let result = match receiver.request.get() {
                    Some(outcome) => {
                        drop((execute, options, signal_receiver));
                        Ok(outcome.into())
                    }
                    None => execute(&engine, options, signal_receiver, &receiver.request),
                };
                // Admission owns cancellation through preparation too, including
                // artifact loading errors before the Wasm engine is entered.
                match receiver.close() {
                    Some(outcome) => Ok(outcome.into()),
                    None => result,
                }
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
            .expect("process reaper must accept its execution");
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
        let table = self.processes.0.lock().unwrap();
        let entry = table.get(&pid).ok_or(ProcessError::NotFound(pid))?;
        // Lock order is table then termination. Run completion acquires the
        // table only after execution has closed the termination channel.
        Ok(entry.terminator.terminate(outcome))
    }

    /// Join and reap exactly once, including Run failures and thread panics.
    /// No table lock is held while waiting for Run and thread teardown.
    pub fn wait(&self, pid: VirtualPid) -> Result<RunOutcome, ProcessError> {
        let claim = WaitClaim::new(Arc::clone(&self.processes.0), pid)?;
        let mut result = claim.completion.result.lock().unwrap();
        while result.is_none() {
            result = claim.completion.changed.wait(result).unwrap();
        }
        let outcome = result.take().unwrap();
        drop(result);
        self.processes.0.lock().unwrap().remove(&pid);
        drop(claim);
        outcome
    }

    /// Snapshot the members, including exited processes awaiting reaping.
    /// External termination requests may change their states concurrently.
    pub fn snapshot(&self) -> Vec<ProcessInfo> {
        self.processes
            .0
            .lock()
            .unwrap()
            .values()
            .map(Entry::info)
            .collect()
    }
}

// Mark exit on panic too, after execution and control have unwound. The reaper
// publishes completion separately, after native thread cleanup and join.
// A spawned thread retains the table even if the embedding owner drops its
// Engine. Inline execution instead removes the entry when its caller returns.
struct ProcessExitGuard {
    table: Table,
    pid: VirtualPid,
    reap_on_drop: bool,
}

impl Drop for ProcessExitGuard {
    fn drop(&mut self) {
        let mut table = self.table.lock().unwrap();
        if self.reap_on_drop {
            table.remove(&self.pid);
        } else {
            table.get_mut(&self.pid).unwrap().info.state = ProcessState::Exited;
        }
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

    #[test]
    fn native_inline_execution_stays_on_the_caller_without_a_process_entry() {
        let engine = Engine::default();
        let caller = thread::current().id();
        assert_eq!(
            engine
                .run_inline(
                    RunOptions::default(),
                    crate::signal_channel().1,
                    |engine, options, _, _| {
                        assert_eq!(thread::current().id(), caller);
                        assert!(options.runtime.process_context.is_none());
                        assert!(options.runtime.default_environment.is_none());
                        assert!(matches!(
                            options.runtime.working_directory,
                            WorkingDirectory::Ambient
                        ));
                        assert!(engine.snapshot().is_empty());
                        Ok(RunOutcome::Completed)
                    },
                )
                .unwrap(),
            RunOutcome::Completed
        );
    }

    #[cfg(feature = "v8")]
    #[test]
    fn virtual_inline_execution_stays_on_the_caller_and_owns_its_result() {
        let engine = Engine::default();
        let module = engine
            .compile("empty.wasm", wat::parse_str("(module)").unwrap())
            .unwrap();
        let caller = thread::current().id();
        let (terminator, receiver) = termination_channel();
        let result = engine.run_inline(
            RunOptions::default()
                .with_guest_process_mode(GuestProcessMode::Virtual)
                .with_termination_receiver(receiver),
            crate::signal_channel().1,
            |engine, options, signals, termination| {
                assert_eq!(thread::current().id(), caller);
                let pid = options.runtime.process_context.as_ref().unwrap().pid;
                assert!(options.runtime.default_environment.is_some());
                assert!(matches!(
                    options.runtime.working_directory,
                    WorkingDirectory::Fixed(_)
                ));
                assert_eq!(engine.snapshot().len(), 1);
                assert!(matches!(engine.clone().wait(pid), Err(ProcessError::AlreadyWaiting(id)) if id == pid));
                // A newly created Engine owns a different namespace.
                assert!(matches!(Engine::default().wait(pid), Err(ProcessError::NotFound(id)) if id == pid));
                engine.run_in_scope(&module, options, signals, termination)?;
                // Engine control and the supplied receiver are one channel,
                // including after the Run has released its Runtime.
                assert!(terminator.terminate(RunTermination::Exit(17)));
                assert_eq!(engine.snapshot()[0].state, ProcessState::Terminating);
                assert!(!engine.terminate(pid, RunTermination::Exit(18)).unwrap());
                assert!(matches!(
                    engine.spawn_execution(
                        Some(pid),
                        RunOptions::default(),
                        crate::signal_channel().1,
                        |_, _, _, _| panic!("terminating parent admitted a child"),
                    ),
                    Err(ProcessError::ParentNotRunning(id)) if id == pid
                ));
                Ok(RunOutcome::Completed)
            },
        );
        assert_eq!(result.unwrap(), RunOutcome::Exited(17));
        assert!(!terminator.terminate(RunTermination::Exit(19)));
        assert!(engine.snapshot().is_empty());
    }

    #[cfg(feature = "v8")]
    #[test]
    fn inline_panic_closes_control_reaps_the_root_and_adopts_its_children() {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        use std::time::Duration;

        let engine = Engine::default();
        let (terminator, receiver) = termination_channel();
        let (release, pending) = mpsc::channel();
        let mut child = None;
        let result = catch_unwind(AssertUnwindSafe(|| {
            engine.run_inline(
                RunOptions::default()
                    .with_guest_process_mode(GuestProcessMode::Virtual)
                    .with_termination_receiver(receiver),
                crate::signal_channel().1,
                |engine, options, _, _| {
                    let parent = options.runtime.process_context.as_ref().unwrap().pid;
                    child = Some(
                        engine
                            .spawn_execution(
                                Some(parent),
                                RunOptions::default(),
                                crate::signal_channel().1,
                                move |_, _, _, _| {
                                    pending.recv_timeout(Duration::from_secs(5)).unwrap();
                                    Ok(RunOutcome::Completed)
                                },
                            )
                            .unwrap()
                            .pid,
                    );
                    panic!("simulated inline execution panic");
                },
            )
        }));
        assert!(result.is_err());
        assert!(!terminator.terminate(RunTermination::Exit(1)));
        let child = child.unwrap();
        assert_eq!(
            engine.snapshot(),
            [ProcessInfo {
                pid: child,
                parent: None,
                state: ProcessState::Running,
            }]
        );
        release.send(()).unwrap();
        assert_eq!(engine.wait(child).unwrap(), RunOutcome::Completed);
        assert!(engine.snapshot().is_empty());
    }

    #[cfg(feature = "v8")]
    #[test]
    fn guest_admission_requires_prepared_virtual_state() {
        let engine = Engine::default();
        for options in [
            RunOptions::default(),
            RunOptions::default().with_environment([("ONLY_GUEST", "value")]),
            RunOptions::default()
                .with_working_directory(WorkingDirectory::Fixed(std::env::current_dir().unwrap())),
        ] {
            assert!(matches!(
                engine.spawn_execution(
                    None,
                    options.with_guest_process_mode(GuestProcessMode::Virtual),
                    crate::signal_channel().1,
                    |_, _, _, _| panic!("incomplete virtual state admitted"),
                ),
                Err(ProcessError::InvalidVirtualConfiguration)
            ));
            assert!(engine.snapshot().is_empty());
        }
    }

    #[cfg(feature = "v8")]
    #[test]
    fn invalid_stack_budgets_are_rejected_before_thread_creation() {
        for stack_size in [0, usize::MAX] {
            let engine = Engine::new(crate::EngineConfig::default().with_stack_size(stack_size));
            assert!(matches!(
                engine.spawn_execution(
                    None,
                    RunOptions::default(),
                    crate::signal_channel().1,
                    |_, _, _, _| panic!("invalid stack admitted"),
                ),
                Err(ProcessError::InvalidStackSize)
            ));
            assert!(engine.snapshot().is_empty());
        }
    }

    #[cfg(feature = "v8")]
    #[test]
    fn spawn_preserves_the_calling_threads_native_stack_size() {
        thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let (low, high) = crate::native_stack::bounds().unwrap();
                let engine = Engine::default();
                let spawned = engine
                    .spawn_execution(
                        None,
                        RunOptions::default(),
                        crate::signal_channel().1,
                        move |_, _, _, _| {
                            let (child_low, child_high) = crate::native_stack::bounds()?;
                            assert!(child_high - child_low >= high - low);
                            Ok(RunOutcome::Completed)
                        },
                    )
                    .unwrap();
                assert_eq!(engine.wait(spawned.pid).unwrap(), RunOutcome::Completed);
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
        // Inline execution must check the caller's real stack, rather than
        // assume it received the larger stack allocated by spawn.
        let inline_engine = engine.clone();
        let result = thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(move || {
                inline_engine.run(
                    &module,
                    RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual),
                )
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("native thread stack")
        );
        assert!(engine.snapshot().is_empty());
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
            let tree = Arc::new(Engine::default());
            let pid = VirtualPid::allocate(&NEXT_PID).unwrap();
            let (terminator, receiver) = termination_channel();
            let (release, blocked) = mpsc::channel();
            let (finish_thread, cleanup) = mpsc::channel();
            let table = Arc::clone(&tree.processes.0);
            let process_completion = Arc::new(ProcessCompletion::default());
            let reaper = process_completion.start_reaper(pid).unwrap();
            let mut entries = tree.processes.0.lock().unwrap();
            let thread = thread::spawn(move || {
                let _cleanup = if native_tls {
                    CLEANUP.set(Some(ThreadCleanup(cleanup)));
                    None
                } else {
                    // Declared before the exit guard so cleanup blocks after
                    // Run exit is recorded, including during unwinding.
                    Some(ThreadCleanup(cleanup))
                };
                let _exit = ProcessExitGuard {
                    table,
                    pid,
                    reap_on_drop: false,
                };
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
            while !tree.processes.0.lock().unwrap()[&pid].waiting {
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
    fn execution_termination_remains_open_after_run_teardown() {
        use std::sync::mpsc;
        use std::time::Duration;

        let engine = Engine::default();
        let module = engine
            .compile("empty.wasm", wat::parse_str("(module)").unwrap())
            .unwrap();
        let tree = engine;
        for (run_error, cleanup_panic) in [(false, false), (true, false), (false, true)] {
            let module = module.clone();
            let (returned, run_returned) = mpsc::channel();
            let (release, cleanup) = mpsc::channel();
            let options = if run_error {
                RunOptions::default().with_test_args("invalid test arguments")
            } else {
                RunOptions::default()
            };
            let spawned = tree
                .spawn_execution(
                    None,
                    options,
                    crate::signal_channel().1,
                    move |engine, options, signals, termination| {
                        let result = engine.run_in_scope(&module, options, signals, termination);
                        assert_eq!(result.is_err(), run_error);
                        returned.send(()).unwrap();
                        // Execution can retain delivery files after its Run returns.
                        // Model that cleanup without depending on filesystem timing.
                        cleanup.recv_timeout(Duration::from_secs(5)).unwrap();
                        assert!(!cleanup_panic, "simulated artifact cleanup panic");
                        result
                    },
                )
                .unwrap();
            run_returned.recv_timeout(Duration::from_secs(5)).unwrap();
            let terminator = tree.processes.0.lock().unwrap()[&spawned.pid]
                .terminator
                .clone();
            let accepted = tree
                .terminate(spawned.pid, RunTermination::Exit(42))
                .unwrap();
            let (done, completion) = mpsc::channel();
            let waiting_tree = tree.clone();
            let waiter = thread::spawn(move || done.send(waiting_tree.wait(spawned.pid)).unwrap());
            assert!(matches!(
                completion.recv_timeout(Duration::from_millis(25)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            release.send(()).unwrap();
            let result = completion.recv_timeout(Duration::from_secs(5)).unwrap();
            waiter.join().unwrap();
            assert!(
                accepted,
                "Run teardown closed the execution termination channel"
            );
            if cleanup_panic {
                assert!(matches!(result, Err(ProcessError::Panicked(pid)) if pid == spawned.pid));
            } else {
                assert_eq!(result.unwrap(), RunOutcome::Exited(42));
            }
            assert!(!terminator.terminate(RunTermination::Exit(43)));
            assert!(tree.snapshot().is_empty());
        }
    }

    #[cfg(feature = "v8")]
    #[test]
    fn dropping_the_engine_preserves_live_runs_until_teardown() {
        use std::time::Duration;

        if std::env::var_os("MOONRUN_DROPPED_TREE_TEST").is_none() {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process_tree::tests::dropping_the_engine_preserves_live_runs_until_teardown",
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
        let tree = engine;
        let pid = tree.spawn(None, &module, RunOptions::default()).unwrap();
        let terminator = tree.processes.0.lock().unwrap()[&pid].terminator.clone();
        let table = Arc::downgrade(&tree.processes.0);
        drop(tree);
        assert!(table.upgrade().is_some());
        assert!(terminator.terminate(RunTermination::Exit(0)));
        while table.upgrade().is_some() {
            thread::yield_now();
        }
        assert!(!terminator.terminate(RunTermination::Exit(1)));
    }
}
