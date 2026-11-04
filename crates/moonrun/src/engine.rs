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

use crate::run_signal::{SignalReceiver, signal_channel};
use crate::run_termination::{RunTermination, RunTerminationReceiver, TerminationRequest};
use crate::runtime::{Executable, Runtime, RuntimeConfig, Stdio, WorkingDirectory};
use crate::source_map;
#[cfg(feature = "v8")]
use crate::v8 as backend;
#[cfg(all(not(feature = "v8"), not(feature = "wee8"), feature = "wasmtime"))]
use crate::wasmtime as backend;
#[cfg(all(feature = "wee8", not(feature = "v8")))]
use crate::wee8 as backend;
use anyhow::Context;
use clap::Parser;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

pub(crate) use backend::SUPPORTS_RUN_TERMINATION;

/// Process-wide configuration for the selected Engine Backend.
///
/// V8 flags are process-global and must be selected before the first run, so
/// all V8-backed [`Engine`] values in one process need the same configuration.
/// A Wasmtime-backed Engine owns its corresponding Wasmtime engine state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EngineConfig {
    pub(crate) stack_size: Option<usize>,
}

impl EngineConfig {
    /// Set the maximum guest stack size in KiB.
    ///
    /// V8 counts existing native caller frames against this budget. Compilation
    /// and execution reject a budget already exhausted by those frames.
    pub fn with_stack_size(mut self, stack_size: usize) -> Self {
        self.stack_size = Some(stack_size);
        self
    }
}

/// Initialize the selected backend's process-wide state on the calling thread.
///
/// With V8, call this before creating any threads that will compile or execute
/// Wasm, on their common ancestor thread (usually the application's main thread).
/// Linux memory protection keys are inherited when threads are created. Calling
/// this again from an existing thread does not repair that thread's permissions.
/// Moonrun does not check this ancestry or move execution to another thread.
///
/// Repeated calls with the same V8 configuration succeed; a different
/// configuration returns an error. Use the same configuration for every Engine.
/// Existing compile/run entry points still initialize lazily when necessary,
/// but multi-threaded embedders must arrange initialization explicitly.
///
/// wee8 initializes its global backend here; its stack size remains per Engine.
/// Wasmtime has no process-wide initialization here and configures each Engine
/// separately. Initialization lasts for the lifetime of the process.
pub fn initialize(config: EngineConfig) -> anyhow::Result<()> {
    #[cfg(feature = "v8")]
    {
        backend::initialize(&config)
    }
    #[cfg(all(feature = "wee8", not(feature = "v8")))]
    {
        let _ = config;
        ::wee8::initialize().map_err(Into::into)
    }
    #[cfg(all(feature = "wasmtime", not(any(feature = "v8", feature = "wee8"))))]
    {
        let _ = config;
        Ok(())
    }
}

/// Configuration for one execution, independent of thread placement.
#[derive(Debug, Default)]
pub struct RunOptions {
    pub(crate) args: Vec<String>,
    pub(crate) no_stack_trace: bool,
    pub(crate) test_args: Option<String>,
    pub(crate) runtime: RuntimeConfig,
    pub(crate) virtualize: Option<bool>,
    pub(crate) ipc: Option<bool>,
    // Required guest headroom in bytes for a supervisor-owned execution thread.
    // Ordinary Runs leave native stack placement and sizing to the caller.
    pub(crate) native_stack_budget: Option<usize>,
    pub(crate) termination_receiver: Option<RunTerminationReceiver>,
}

pub(crate) struct TestArgs {
    pub(crate) file_and_index: Vec<(String, Vec<std::ops::Range<u32>>)>,
}

impl std::str::FromStr for TestArgs {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> anyhow::Result<Self> {
        let file_and_index = if input.is_empty() {
            Vec::new()
        } else {
            input
                .split('/')
                .map(|entry| {
                    // Compiler metadata contains basenames, not directory paths.
                    // Only the final colon separates the name from its ranges.
                    let (file, ranges) = entry.rsplit_once(':').context("missing test ranges")?;
                    let ranges = ranges
                        .split(',')
                        .map(|range| {
                            let (start, end) = range
                                .split_once('-')
                                .with_context(|| format!("invalid test range {range:?}"))?;
                            Ok(start.parse()?..end.parse()?)
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    Ok((file.to_owned(), ranges))
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        };
        Ok(Self { file_and_index })
    }
}

impl RunOptions {
    /// Use Engine-managed process identities and internal child execution.
    /// Also captures owned root environment/cwd; IPC and explicit Runtime
    /// configuration can select owned state without virtual process identities.
    /// Requires V8. Moonx children execute internally; other authorized
    /// commands run as supervised native children with their OS PIDs.
    /// The existing process policy authorizes those invocations; virtualization
    /// does not grant additional permissions or sandbox preparation helpers.
    pub fn with_virtualize(mut self, virtualize: bool) -> Self {
        self.virtualize = Some(virtualize);
        self
    }

    /// Effective execution behavior: an explicit embedding choice takes
    /// precedence over the selected policy, which otherwise defaults to false.
    pub fn virtualize(&self) -> bool {
        self.virtualize.unwrap_or_else(|| {
            self.runtime
                .selected_policy
                .as_ref()
                .is_some_and(|policy| policy.virtualize)
        })
    }

    /// Offer this Run's native descendants an IPC entry into the same Engine.
    /// IPC-enabled Runs participate in Engine shutdown supervision without
    /// changing guest process identity. Normal completion joins their delegates.
    /// Each entry retains the Run's effective policy ceiling. V8 only;
    /// guest PID routing remains native. Disabled unless selected explicitly.
    pub fn with_ipc(mut self, enabled: bool) -> Self {
        self.ipc = Some(enabled);
        self
    }

    pub fn ipc(&self) -> bool {
        self.ipc
            .unwrap_or_else(|| self.runtime.selected_policy.as_ref().is_some_and(|p| p.ipc))
    }

    /// Supply an owned environment. An empty iterator means an empty environment.
    /// Policy environment provisioning, when present, takes precedence.
    /// Guest changes affect only this execution's environment.
    pub fn with_environment<I, K, V>(mut self, environment: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.runtime.default_environment = Some(
            environment
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        );
        self
    }

    // Only host-facing entries may fill absent root inputs from ambient state.
    // Guest derivation supplies complete inputs before process admission.
    pub(crate) fn prepare_root(&mut self) -> std::io::Result<()> {
        if self.virtualize() || self.ipc() {
            self.runtime.capture_host_state()?;
        }
        Ok(())
    }

    /// Bind a single-use receiver for external forced termination (V8 only).
    ///
    /// Other backends reject this option. See [`crate::RunTerminator::terminate`]
    /// for acceptance and completion semantics.
    pub fn with_termination_receiver(mut self, receiver: RunTerminationReceiver) -> Self {
        self.termination_receiver = Some(receiver);
        self
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn without_stack_trace(mut self) -> Self {
        self.no_stack_trace = true;
        self
    }

    /// Select tests with `filename:start-end,start-end/...` (half-open ranges).
    pub fn with_test_args(mut self, test_args: impl Into<String>) -> Self {
        self.test_args = Some(test_args.into());
        self
    }

    pub(crate) fn parsed_test_args(&self) -> anyhow::Result<Option<TestArgs>> {
        self.test_args
            .as_deref()
            .map(str::parse)
            .transpose()
            .context("invalid MoonBit test arguments")
    }

    /// Read and resolve a selected policy once. Its restrictions are additional
    /// to any inherited ceiling; its Env provisioning uses the Run's source Env.
    pub fn with_policy_file(self, policy_file: impl AsRef<Path>) -> anyhow::Result<Self> {
        self.with_policy_file_source_dir(policy_file, None)
    }

    #[doc(hidden)]
    pub fn with_policy_file_source_dir(
        mut self,
        policy_file: impl AsRef<Path>,
        source_dir: Option<&Path>,
    ) -> anyhow::Result<Self> {
        self.runtime.selected_policy = Some(
            crate::policy::read_file(policy_file.as_ref(), source_dir)
                .context("failed to load sandbox policy (experimental); run `moonrun --help` for policy format notes")?,
        );
        Ok(self)
    }

    /// Receive the internal restriction transfer format, not a user policy document.
    #[doc(hidden)]
    pub fn with_inherited_policy(mut self, policy: Vec<u8>) -> Self {
        self.runtime.inherited_policy = Some(crate::policy::InheritedPolicyInput::Transfer(policy));
        self
    }

    pub(crate) fn with_parent_policy(mut self, policy: crate::policy::Policy) -> Self {
        self.runtime.inherited_policy = Some(crate::policy::InheritedPolicyInput::Resolved(policy));
        self
    }

    /// Select the working-directory behavior for this run.
    ///
    /// [`WorkingDirectory::Ambient`] preserves process-global cwd behavior;
    /// [`WorkingDirectory::Fixed`] resolves paths against a caller-owned directory.
    /// Fixed roots must be existing directories and are canonicalized when
    /// execution begins.
    pub fn with_working_directory(mut self, working_directory: WorkingDirectory) -> Self {
        self.runtime.working_directory = working_directory;
        self
    }

    /// Bind this run's standard streams to caller-supplied files.
    ///
    /// These streams also supply default child streams; explicit guest-supplied
    /// child streams take precedence. Without this option, streams are ambient.
    /// The files are moved into the Runtime for this run.
    pub fn with_stdio(mut self, stdin: File, stdout: File, stderr: File) -> Self {
        self.runtime.stdio = Stdio::owned(stdin, stdout, stderr);
        self
    }

    /// Preserve the calling thread's current signal mask for child processes.
    ///
    /// This is CLI plumbing: it snapshots the mask before the process signal
    /// broker blocks its managed signals in every subsequently created thread.
    #[cfg(unix)]
    #[doc(hidden)]
    pub fn preserve_current_signal_mask_for_children(mut self) -> anyhow::Result<Self> {
        self.runtime.preserved_child_signal_mask = Some(crate::runtime::current_signal_mask()?);
        Ok(self)
    }
}

/// The observable result of one MoonBit Wasm run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Exited(i32),
    KilledBySignal(i32),
}

impl From<RunTermination> for RunOutcome {
    fn from(termination: RunTermination) -> Self {
        match termination {
            RunTermination::Exit(code) => Self::Exited(code),
            RunTermination::KilledBySignal(signal) => Self::KilledBySignal(signal),
        }
    }
}

/// Backend result before the public runner applies Moonrun's common lifecycle
/// policy. Guest failures are deliberately distinct from terminations requested
/// by guest code.
pub(crate) enum BackendRunOutcome {
    Completed,
    GuestFailure,
    Terminated(RunTermination),
}

/// Result of one backend call into guest code. Guest diagnostics are formatted
/// by the backend before common run or test-driver policy consumes them.
pub(crate) enum BackendCallOutcome {
    Completed,
    GuestFailure(String),
    Terminated(RunTermination),
}

#[derive(serde::Serialize)]
struct TestFailure<'a> {
    r#type: &'static str,
    file: &'a str,
    index: u32,
    message: &'a str,
}

pub(crate) fn complete_run_call(
    outcome: BackendCallOutcome,
    stdio: &Stdio,
) -> anyhow::Result<BackendRunOutcome> {
    match outcome {
        BackendCallOutcome::Completed => Ok(BackendRunOutcome::Completed),
        BackendCallOutcome::GuestFailure(message) => {
            stdio.with_stderr(|stderr| writeln!(stderr, "{message}"))?;
            Ok(BackendRunOutcome::GuestFailure)
        }
        BackendCallOutcome::Terminated(termination) => {
            Ok(BackendRunOutcome::Terminated(termination))
        }
    }
}

/// Run the backend-neutral MoonBit test-driver lifecycle. Backends provide
/// only the representation-specific calls and formatted guest diagnostics.
pub(crate) fn run_test_driver<D>(
    driver: &mut D,
    test_args: TestArgs,
    stdio: &Stdio,
    initialize: impl FnOnce(&mut D) -> anyhow::Result<BackendCallOutcome>,
    mut execute: impl FnMut(&mut D, &str, u32) -> anyhow::Result<BackendCallOutcome>,
    finish: impl FnOnce(&mut D) -> anyhow::Result<BackendCallOutcome>,
) -> anyhow::Result<BackendRunOutcome> {
    // Tests invoke exports directly. Initialize once before any test or finalizer,
    // preferring `_initialize` and falling back to `_start` for test executables.
    // Older modules may omit both exports.
    match initialize(driver).context("failed to initialize the MoonBit test driver")? {
        BackendCallOutcome::Completed => {}
        outcome => return complete_run_call(outcome, stdio),
    }
    let TestArgs { file_and_index } = test_args;
    for (file, ranges) in file_and_index {
        for index in ranges.into_iter().flatten() {
            match execute(driver, &file, index)? {
                BackendCallOutcome::Completed => {}
                BackendCallOutcome::GuestFailure(message) => {
                    let result = TestFailure {
                        r#type: "result",
                        file: &file,
                        index,
                        message: &message,
                    };
                    stdio.with_stdout(|stdout| {
                        writeln!(stdout, "----- BEGIN MOON TEST RESULT -----")?;
                        serde_json::to_writer(&mut *stdout, &result)?;
                        writeln!(stdout)?;
                        writeln!(stdout, "----- END MOON TEST RESULT -----")?;
                        Ok::<(), anyhow::Error>(())
                    })?;
                }
                BackendCallOutcome::Terminated(termination) => {
                    return Ok(BackendRunOutcome::Terminated(termination));
                }
            }
        }
    }
    complete_run_call(finish(driver)?, stdio)
}

struct ModuleData {
    name: String,
    executable: Executable,
    compiled: backend::CompiledModule,
    source_map: Option<source_map::SourceMap>,
}

/// An immutable Wasm module compiled by an [`Engine`].
///
/// Loading compiles the file once against the Engine. Cloning a Module reuses
/// that compiled representation, and each call to [`Engine::run`] creates
/// fresh per-run guest and Runtime state from it.
#[derive(Clone)]
pub struct Module(Arc<ModuleData>);

impl Module {
    pub(crate) fn name(&self) -> &str {
        &self.0.name
    }

    pub(crate) fn compiled(&self) -> &backend::CompiledModule {
        &self.0.compiled
    }

    pub(crate) fn executable(&self) -> &Executable {
        &self.0.executable
    }

    pub(crate) fn source_map(&self) -> Option<&source_map::SourceMap> {
        self.0.source_map.as_ref()
    }
}

impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.0.name)
            .finish_non_exhaustive()
    }
}

/// Moonrun's execution host: reusable modules, execution, and process lifecycle.
///
/// [`Engine::run`] executes on the calling thread; [`Engine::spawn`] creates a
/// managed thread whose result is consumed by [`Engine::wait`]. Clones share
/// the backend and process table; a new Engine creates a separate namespace.
/// Call [`initialize`] before creating threads that will use V8-backed Engines.
///
/// Environment, cwd and stdio default to ambient process state; callers can
/// bind an owned environment, owned streams and a fixed cwd. Ambient guest
/// environment mutations must not race other process-environment access.
/// [`RunOptions::with_virtualize`] provides owned root state and guest process
/// control. Parent links alone do not imply state or permission inheritance.
/// Dropping an Engine does not terminate or join its executions; callers must
/// explicitly terminate and wait when required. [`Engine::request_shutdown`]
/// closes managed, virtualized and IPC-enabled admission and requests termination of
/// all admitted members. Ordinary native caller-thread Runs retain independent control.
/// OS signal capture is left to the caller; a signal channel can direct
/// cooperative delivery to one Run.
#[derive(Clone, Debug)]
pub struct Engine {
    backend: Arc<backend::Engine>,
    pub(crate) config: EngineConfig,
    pub(crate) processes: crate::process_tree::ProcessTable,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Self {
        Self {
            backend: Arc::new(backend::Engine::new(config)),
            config,
            processes: Default::default(),
        }
    }

    /// Compile Wasm bytes into a reusable immutable Module.
    pub fn compile(
        &self,
        name: impl Into<String>,
        bytes: impl AsRef<[u8]>,
    ) -> anyhow::Result<Module> {
        let name = name.into();
        let compiled = self
            .backend
            .compile(bytes.as_ref())
            .with_context(|| format!("failed to compile `{name}`"))?;
        Ok(Module(Arc::new(ModuleData {
            name,
            executable: Executable::unavailable(),
            compiled,
            source_map: None,
        })))
    }

    /// Compile a Wasm file into a reusable immutable Module.
    pub fn load_file(&self, file: impl AsRef<Path>) -> anyhow::Result<Module> {
        let file = file.as_ref();
        if !file.exists() {
            anyhow::bail!("no such file");
        }
        if file.extension().and_then(|extension| extension.to_str()) != Some("wasm") {
            anyhow::bail!("Unsupported file type");
        }
        let bytes = std::fs::read(file).context("failed to read Wasm file")?;
        self.compile_file(file, &bytes, || {
            moonutil::wasm_source_map::load(file, &bytes)
                .and_then(|value| source_map::SourceMap::parse(&value))
        })
    }

    /// Compile captured file contents without reopening their original path.
    pub(crate) fn compile_file(
        &self,
        file: &Path,
        bytes: &[u8],
        load_source_map: impl FnOnce() -> Option<source_map::SourceMap>,
    ) -> anyhow::Result<Module> {
        let name = file.to_string_lossy().into_owned();
        let executable = Executable::from_file(file);
        let compiled = self
            .backend
            .compile(bytes)
            .with_context(|| format!("failed to compile `{name}`"))?;
        Ok(Module(Arc::new(ModuleData {
            name,
            executable,
            compiled,
            source_map: load_source_map(),
        })))
    }

    /// Execute one run synchronously on the calling thread.
    pub fn run(&self, module: &Module, options: RunOptions) -> anyhow::Result<RunOutcome> {
        self.run_with_signal_receiver(module, options, signal_channel().1)
    }

    /// Execute one run with a caller-provided signal receiver.
    ///
    /// Delivery is cooperative: the guest must first register interest and
    /// start its async signal waiter. Signals are not retained before that
    /// point and cannot yet forcibly interrupt guest execution.
    pub fn run_with_signal_receiver(
        &self,
        module: &Module,
        options: RunOptions,
        signals: SignalReceiver,
    ) -> anyhow::Result<RunOutcome> {
        self.run_inline(options, signals, |engine, options, signals, scope| {
            engine.run_in_scope(module, options, signals, scope.termination())
        })
    }

    /// Execute one Run using control owned by the surrounding execution scope.
    /// Runtime teardown completes here; only the receiver owner may finalize the
    /// result and close admission to termination requests.
    pub(crate) fn run_in_scope(
        &self,
        module: &Module,
        options: RunOptions,
        signals: SignalReceiver,
        termination: &TerminationRequest,
    ) -> anyhow::Result<RunOutcome> {
        debug_assert!(options.termination_receiver.is_none());
        let mut options = options;
        if options.ipc() {
            anyhow::ensure!(SUPPORTS_RUN_TERMINATION, "IPC sessions require V8");
            anyhow::ensure!(
                !options.virtualize(),
                "IPC virtual process sessions are not supported yet"
            );
            options.runtime.ipc_engine = Some(self.clone());
        }
        let outcome = (|| {
            if let Some(outcome) = termination.get() {
                return Ok(BackendRunOutcome::Terminated(outcome));
            }
            if let Some(bytes) = options.native_stack_budget {
                crate::native_stack::checked_limit(bytes)?;
            }
            let test_args = options.parsed_test_args()?;
            let runtime = Runtime::new(options.runtime, module.executable().clone(), signals)?;
            self.backend.run(
                module.name(),
                module.compiled(),
                module.source_map(),
                &options.args,
                test_args,
                options.no_stack_trace,
                runtime,
                termination.clone(),
            )
        })()?;
        Ok(match outcome {
            BackendRunOutcome::Completed => RunOutcome::Completed,
            BackendRunOutcome::GuestFailure => RunOutcome::Exited(1),
            BackendRunOutcome::Terminated(termination) => termination.into(),
        })
    }

    /// Prepare and execute Wasm using a host-selected Moon or moonx helper.
    /// Program arguments belong in `args`. Moonx frontend responses (including
    /// help/version and parse errors) use the embedded grammar and bound stdio,
    /// without launching the selected executable. Execution requests prepare
    /// Wasm through an external helper with host access, then run it here.
    /// Native/JS targets use ordinary OS process execution, not this handoff.
    pub fn run_moon(
        &self,
        executable: &Path,
        args: &[OsString],
        options: RunOptions,
    ) -> anyhow::Result<RunOutcome> {
        if !options.args.is_empty() || options.test_args.is_some() {
            return Err(crate::ProcessError::MoonArgumentsAlreadyConfigured.into());
        }
        self.run_inline(
            options,
            signal_channel().1,
            |engine, options, signals, scope| {
                engine.execute_moon_in_scope(
                    executable,
                    args,
                    options,
                    signals,
                    scope,
                    #[cfg(windows)]
                    false,
                )
            },
        )
    }

    pub(crate) fn execute_moon_in_scope(
        &self,
        executable: &Path,
        args: &[std::ffi::OsString],
        mut options: RunOptions,
        signals: crate::SignalReceiver,
        scope: &mut crate::process_tree::ExecutionScope<'_>,
        #[cfg(windows)] no_console_window: bool,
    ) -> anyhow::Result<RunOutcome> {
        if let Some(outcome) = scope.termination().get() {
            return Ok(outcome.into());
        }
        // Select the invocation before entering artifact preparation. A frontend
        // response is already complete: it needs neither a helper nor handoff
        // storage. Parsing also keeps private handoff flags out of the frontend
        // position without interpreting the program's opaque argument tail.
        if moonutil::constants::is_moonx_executable(executable.as_os_str()) {
            match moonutil::cli_support::MoonxCli::try_parse_from(
                std::iter::once(executable.as_os_str()).chain(args.iter().map(OsString::as_os_str)),
            ) {
                Ok(frontend) => {
                    if frontend.daemon {
                        anyhow::ensure!(
                            !options.virtualize(),
                            "IPC virtual process sessions are not supported yet"
                        );
                        options = options.with_ipc(true);
                        // This choice is already known: include preparation in
                        // Engine shutdown just as with_ipc(true) does at entry.
                        scope.bind_supervision(self, &mut options, &signals)?;
                    }
                }
                Err(response) => {
                    let output = response.render().to_string();
                    // Mirror Clap's CLI exit: output failure does not change the exit
                    // code. Use plain text on the bound streams, never the host's TTY
                    // detection or process-wide print/exit functions.
                    if response.use_stderr() {
                        let _ = options
                            .runtime
                            .stdio
                            .with_stderr(|out| out.write_all(output.as_bytes()));
                    } else {
                        let _ = options
                            .runtime
                            .stdio
                            .with_stdout(|out| out.write_all(output.as_bytes()));
                    }
                    return Ok(RunOutcome::Exited(response.exit_code()));
                }
            }
        }
        let (program, _directory) = match crate::process::prepare_moon(
            executable,
            args,
            &options.runtime,
            scope.termination(),
            #[cfg(windows)]
            no_console_window,
        )? {
            crate::process::Preparation::Ready { program, directory } => (*program, directory),
            crate::process::Preparation::Exited(outcome) => return Ok(outcome),
        };
        options.args = program.args;
        options.runtime.working_directory = WorkingDirectory::Fixed(program.working_directory);
        if options.runtime.selected_policy.is_none()
            && let Some((policy, source_dir)) = program.policy
        {
            options = options.with_policy_file_source_dir(policy, Some(&source_dir))?;
        }
        scope.bind_supervision(self, &mut options, &signals)?;
        // The delivery directory outlives Runtime construction and execution. Its
        // cleanup finishes before the Engine finalizes the execution result.
        let bytes = std::fs::read(&program.artifact)?;
        let module = self.compile_file(&program.executable, &bytes, || {
            program
                .source_map
                .as_deref()
                .and_then(crate::source_map::SourceMap::parse)
        })?;
        self.run_in_scope(&module, options, signals, scope.termination())
    }

    /// Load and synchronously execute one Wasm file.
    pub fn run_file(
        &self,
        file: impl AsRef<Path>,
        options: RunOptions,
    ) -> anyhow::Result<RunOutcome> {
        self.run_file_with_signal_receiver(file, options, signal_channel().1)
    }

    /// Load and execute a Wasm file on the calling thread with injected signals.
    /// Loading and execution share one termination scope.
    pub fn run_file_with_signal_receiver(
        &self,
        file: impl AsRef<Path>,
        options: RunOptions,
        signals: SignalReceiver,
    ) -> anyhow::Result<RunOutcome> {
        self.run_inline(options, signals, |engine, options, signals, scope| {
            let module = engine.load_file(file)?;
            engine.run_in_scope(&module, options, signals, scope.termination())
        })
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(EngineConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn test_args_preserve_names_and_selection_order() {
        let expected = vec![
            (
                "{Case:测试,e\u{301}😀\"\\\n\t%2F.mbt".to_owned(),
                vec![7..9, 0..2, 7..8],
            ),
            ("-other.mbt".to_owned(), vec![1..2]),
            (
                "{Case:测试,e\u{301}😀\"\\\n\t%2F.mbt".to_owned(),
                vec![3..3],
            ),
        ];
        assert_eq!(
            RunOptions::default()
                .with_test_args("{Case:测试,e\u{301}😀\"\\\n\t%2F.mbt:7-9,0-2,7-8/-other.mbt:1-2/{Case:测试,e\u{301}😀\"\\\n\t%2F.mbt:3-3")
                .parsed_test_args()
                .unwrap()
                .unwrap()
                .file_and_index,
            expected
        );
        assert!(
            RunOptions::default()
                .with_test_args("")
                .parsed_test_args()
                .unwrap()
                .unwrap()
                .file_and_index
                .is_empty()
        );
    }

    #[test]
    fn run_options_default_to_ambient_working_directory() {
        assert_eq!(
            RunOptions::default().runtime.working_directory,
            WorkingDirectory::Ambient
        );
        assert_eq!(
            RunOptions::default()
                .with_working_directory(WorkingDirectory::Ambient)
                .runtime
                .working_directory,
            WorkingDirectory::Ambient
        );
    }

    #[test]
    fn loaded_module_keeps_source_map_after_files_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let wasm_path = dir.path().join("main.wasm");
        let map_dir = dir.path().join("maps");
        let map_path = map_dir.join("original.map");
        let source_map = r#"{"version":3,"sources":["main.mbt"],"mappings":"AAAA"}"#;
        let section_name = b"sourceMappingURL";
        let map_name = b"maps/original.map";
        let section_len = 1 + section_name.len() + 1 + map_name.len();
        let wasm = [
            b"\0asm\x01\0\0\0".as_slice(),
            &[0, section_len as u8],
            &[section_name.len() as u8],
            section_name,
            &[map_name.len() as u8],
            map_name,
        ]
        .concat();
        std::fs::create_dir(map_dir).unwrap();
        std::fs::write(&wasm_path, wasm).unwrap();
        std::fs::write(&map_path, source_map).unwrap();

        let module = Engine::default().load_file(&wasm_path).unwrap();
        std::fs::remove_file(wasm_path).unwrap();
        std::fs::remove_file(map_path).unwrap();

        assert_eq!(
            module.source_map().and_then(|map| map.position(0)),
            Some(source_map::SourcePosition {
                file: "main.mbt",
                line: 1,
            })
        );
    }

    #[test]
    fn loaded_module_captures_its_absolute_executable_path() {
        let current_dir = std::env::current_dir().unwrap();
        let dir = tempfile::Builder::new()
            .prefix("moonrun-executable-origin.")
            .tempdir_in(&current_dir)
            .unwrap();
        let wasm_path = dir.path().join("main.wasm");
        std::fs::write(&wasm_path, b"\0asm\x01\0\0\0").unwrap();
        let relative_path = wasm_path.strip_prefix(&current_dir).unwrap();

        let module = Engine::default().load_file(relative_path).unwrap();
        let expected = std::path::absolute(relative_path).unwrap();
        std::fs::rename(&wasm_path, dir.path().join("renamed.wasm")).unwrap();

        assert_eq!(module.executable().path(), Ok(expected.as_path()));
    }

    #[test]
    fn module_compiled_from_bytes_has_no_executable_path() {
        let module = Engine::default()
            .compile("diagnostic-name", b"\0asm\x01\0\0\0")
            .unwrap();

        assert_eq!(
            module.executable().path(),
            Err(crate::async_host::AsyncHostError::Inval)
        );
    }
}
