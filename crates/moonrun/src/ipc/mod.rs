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

//! Experimental execution over an already-authorized local connection.
//!
//! The embedding host owns connection admission, scheduling and Engine lifetime.
//! RunOptions::with_ipc enables Run-owned discovery for native descendants.
//! The low-level adapter is not an authentication protocol. Giving a peer
//! a connection delegates the ability to select a Wasm program to run with the
//! host-supplied restrictions. See `docs/dev/design/ipc-execution.md`.

pub(crate) mod session;
#[cfg(test)]
mod tests;
mod wire;

/// Inherited routing metadata for native descendants of an IPC-enabled Run.
pub const ENDPOINT_ENV: &str = "MOONRUN_IPC_ENDPOINT";

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, IsTerminal};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::policy::{InheritedPolicyInput, Policy};
use crate::{Engine, RunOptions, RunOutcome, RunTermination, termination_channel};

/// Explicit caller state for one execution. No field defaults to daemon state.
///
/// Program and working directory must be absolute. Unix paths and environment
/// values retain their original bytes. Stdio is transferred separately as OS
/// resources, not paths to reopen. `inherited_policy` is an optional additional
/// restriction transfer, not proof of authority or a user policy document.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    #[serde(with = "wire::path")]
    pub program: PathBuf,
    pub args: Vec<String>,
    #[serde(with = "wire::path")]
    pub working_directory: PathBuf,
    pub environment: Vec<(OsString, OsString)>,
    pub inherited_policy: Option<Vec<u8>>,
    pub no_stack_trace: bool,
    pub test_args: Option<String>,
    pub stack_size: Option<usize>,
}

impl Invocation {
    fn validate(&self) -> io::Result<()> {
        if !self.program.is_absolute() || !self.working_directory.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "IPC program and working directory must be absolute",
            ));
        }
        Ok(())
    }
}

/// One submitted execution. Dropping it shuts down the connection and requests
/// cancellation, even when a cloned control handle remains alive. Only `wait`
/// confirms completion; losing the connection leaves the outcome unknown.
/// There is no automatic retry or local fallback after submission.
pub struct ClientRun {
    stream: UnixStream,
    control: RunControl,
}

impl ClientRun {
    /// Connect a normal CLI invocation, capturing its state before any Engine
    /// is initialized. A stale endpoint is an error, never a local retry.
    pub fn connect(
        endpoint: impl AsRef<std::path::Path>,
        program: impl AsRef<std::path::Path>,
        mut options: RunOptions,
        config: crate::EngineConfig,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !options.virtualize(),
            "IPC virtual process sessions are not supported yet"
        );
        anyhow::ensure!(
            options.termination_receiver.is_none(),
            "IPC client owns its termination control"
        );
        let source = options
            .runtime
            .default_environment
            .take()
            .unwrap_or_else(|| {
                std::env::vars_os()
                    .filter(|(key, _)| key != ENDPOINT_ENV)
                    .collect()
            });
        let (policy, environment) = options
            .runtime
            .realize_policy(crate::runtime::Env::owned(source)?)?;
        let invocation = Invocation {
            program: dunce::canonicalize(program)?,
            args: options.args,
            working_directory: options.runtime.working_directory.current_dir()?,
            environment: environment.entries(),
            inherited_policy: policy.as_ref().map(Policy::to_transfer).transpose()?,
            no_stack_trace: options.no_stack_trace,
            test_args: options.test_args,
            stack_size: config.stack_size,
        };
        let stdio = options.runtime.stdio.into_files()?;
        let stream = UnixStream::connect(endpoint)
            .context("failed to connect to inherited moonrun IPC session")?;
        Self::submit(stream, invocation, stdio).map_err(Into::into)
    }

    /// Submit exactly one invocation on a fresh, connected Unix stream.
    /// Files and pipes retain their identity and flags. Terminal streams require
    /// both client and execution host to belong to their foreground group.
    pub fn submit(
        stream: UnixStream,
        invocation: Invocation,
        stdio: [File; 3],
    ) -> io::Result<Self> {
        stream.set_nonblocking(false)?;
        let control = RunControl {
            stream: Arc::new(Mutex::new(stream.try_clone()?)),
        };
        let run = Self { stream, control };
        invocation.validate()?;
        check_terminals(&stdio)?;
        // Serialize and bound the request before delegating any descriptors.
        let request = wire::encode(&wire::Request {
            version: wire::VERSION,
            invocation,
        })?;
        wire::send_stdio(&run.stream, &stdio)?;
        wire::write_frame(&run.stream, &request)?;
        Ok(run)
    }

    /// Make a control sender for use while another thread waits for completion.
    pub fn control(&self) -> RunControl {
        self.control.clone()
    }

    /// Consume the final outcome, published only after Engine teardown.
    /// Transport errors and remote execution errors are distinct from guest exit.
    pub fn wait(self) -> anyhow::Result<RunOutcome> {
        match wire::read::<wire::Reply>(&self.stream).context("IPC execution outcome is unknown")? {
            wire::Reply::Completed => Ok(RunOutcome::Completed),
            wire::Reply::Exited(code) => Ok(RunOutcome::Exited(code)),
            wire::Reply::KilledBySignal(signal) => Ok(RunOutcome::KilledBySignal(signal)),
            wire::Reply::Failed(message) => anyhow::bail!("remote execution failed: {message}"),
        }
    }
}

impl Drop for ClientRun {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

/// Control for only the execution associated with this connection.
#[derive(Clone)]
pub struct RunControl {
    stream: Arc<Mutex<UnixStream>>,
}

impl RunControl {
    /// Forward a CLI signal. The host delivers it cooperatively when the guest
    /// registered interest; otherwise it requests this Run's forced termination.
    pub fn signal(&self, signal: i32) -> io::Result<()> {
        if ![libc::SIGINT, libc::SIGTERM, libc::SIGHUP].contains(&signal) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported IPC signal",
            ));
        }
        wire::write(&self.stream.lock().unwrap(), &wire::Control::Signal(signal))
    }

    /// Send a forced termination request. Success means the message was sent,
    /// not that termination was accepted or cleanup finished. The first accepted
    /// request wins; completion can race with delivery. Use `ClientRun::wait` to
    /// observe the outcome. Use `signal` for cooperative CLI signal delivery.
    pub fn terminate(&self, termination: RunTermination) -> io::Result<()> {
        let control = match termination {
            RunTermination::Exit(code) => wire::Control::Exit(code),
            RunTermination::KilledBySignal(signal) => wire::Control::KilledBySignal(signal),
        };
        wire::write(&self.stream.lock().unwrap(), &control)
    }
}

/// Serve one invocation on the calling thread using an existing Engine.
///
/// The caller must authorize the connection before calling this function and
/// provide any host restrictions through `options`. Client restrictions only
/// narrow them. Caller state replaces args, env, cwd and stdio; host-selected
/// policy provisioning still applies to that supplied environment.
///
/// This initial adapter supports V8 and native guest process routing only.
/// It neither installs OS signal handlers nor changes process cwd/environment.
/// Disconnect requests Exit(1) for this Run, never Engine-wide shutdown.
/// Synchronous host I/O can still delay teardown. Native children keep their
/// existing native lifetime semantics; this function does not reap their trees.
/// Guest `getpid` observes the execution host, not the client process.
///
/// On Darwin, receiving descriptors and setting CLOEXEC are separate calls.
/// Overlapping native spawn can inherit these fds, including another Run's pipe
/// ends. Hosts needing atomic inheritance isolation must coordinate admission
/// with all native process creation; this adapter supplies no such global lock.
/// Linux receives the descriptors atomically with CLOEXEC.
///
/// Ok means the final reply was sent, including when it reports an execution
/// error. An I/O error means delivery failed; a caller must not retry execution.
pub fn serve(engine: &Engine, stream: UnixStream, mut options: RunOptions) -> anyhow::Result<()> {
    // Keeping this guard inside the scoped-thread closure also wakes its reader
    // if execution unwinds, before scope tries to join that reader.
    struct StopReader<'a>(&'a UnixStream);
    impl Drop for StopReader<'_> {
        fn drop(&mut self) {
            let _ = self.0.shutdown(Shutdown::Read);
        }
    }

    stream.set_nonblocking(false)?;
    let (terminator, receiver) = termination_channel();
    let result = (|| -> anyhow::Result<RunOutcome> {
        anyhow::ensure!(
            crate::engine::SUPPORTS_RUN_TERMINATION,
            "IPC execution requires external Run termination support"
        );
        anyhow::ensure!(
            !options.virtualize(),
            "IPC virtual process sessions are not supported yet"
        );
        anyhow::ensure!(
            options.termination_receiver.is_none(),
            "IPC owns its Run termination receiver"
        );
        let stdio = wire::receive_stdio(&stream)?;
        let request: wire::Request = wire::read(&stream)?;
        anyhow::ensure!(
            request.version == wire::VERSION,
            "unsupported IPC protocol version"
        );
        let invocation = request.invocation;
        invocation.validate()?;
        check_terminals(&stdio)?;
        if let Some(stack) = invocation.stack_size {
            anyhow::ensure!(
                stack.checked_mul(1024) == Some(engine.guest_stack_budget()?),
                "IPC stack size differs from the execution host"
            );
        }
        options.no_stack_trace = invocation.no_stack_trace;
        options.test_args = invocation.test_args;
        stream.set_read_timeout(None)?;
        if let Some(additional) = invocation.inherited_policy {
            let additional = Policy::from_transfer(&additional)?;
            let inherited = match options.runtime.inherited_policy.take() {
                None => additional,
                Some(InheritedPolicyInput::Resolved(host)) => host.intersect(additional),
                Some(InheritedPolicyInput::Transfer(bytes)) => {
                    Policy::from_transfer(&bytes)?.intersect(additional)
                }
            };
            options = options.with_parent_policy(inherited);
        }
        let [stdin, stdout, stderr] = stdio;
        options = options
            .with_args(invocation.args)
            .with_environment(invocation.environment)
            .with_working_directory(crate::WorkingDirectory::Fixed(invocation.working_directory))
            .with_stdio(stdin, stdout, stderr)
            .with_termination_receiver(receiver);
        let (signal_sender, signals) = crate::signal_channel();
        std::thread::scope(|scope| {
            let reader = &stream;
            let control = std::thread::Builder::new()
                .name("moonrun-ipc-control".into())
                .spawn_scoped(scope, move || {
                    loop {
                        let termination = match wire::read::<wire::Control>(reader) {
                            Ok(wire::Control::Signal(signal)) => {
                                if ![libc::SIGINT, libc::SIGTERM, libc::SIGHUP].contains(&signal) {
                                    terminator.terminate(RunTermination::Exit(1));
                                    break;
                                }
                                if signal_sender.send(signal) != Ok(true) {
                                    terminator.terminate(RunTermination::KilledBySignal(signal));
                                }
                                continue;
                            }
                            Ok(wire::Control::Exit(code)) => RunTermination::Exit(code),
                            Ok(wire::Control::KilledBySignal(signal)) => {
                                RunTermination::KilledBySignal(signal)
                            }
                            Err(_) => {
                                terminator.terminate(RunTermination::Exit(1));
                                break;
                            }
                        };
                        terminator.terminate(termination);
                    }
                })?;
            let stop_reader = StopReader(&stream);
            let outcome =
                engine.run_file_with_signal_receiver(invocation.program, options, signals);
            // Stop only the reader here; keep the write half for the final reply.
            drop(stop_reader);
            let joined = control.join();
            joined.map_err(|_| anyhow::anyhow!("IPC control reader panicked"))?;
            outcome
        })
    })();
    let reply = match result {
        Ok(RunOutcome::Completed) => wire::Reply::Completed,
        Ok(RunOutcome::Exited(code)) => wire::Reply::Exited(code),
        Ok(RunOutcome::KilledBySignal(signal)) => wire::Reply::KilledBySignal(signal),
        Err(error) => wire::Reply::Failed(format!("{error:#}")),
    };
    wire::write_reply(&stream, &reply).context("failed to send IPC execution outcome")
}

// The MVP serves descendants in the host's foreground terminal group. It does
// not move a terminal's foreground group to a shared execution host. Pipes and
// files remain usable regardless of process-group placement.
fn check_terminals(stdio: &[File; 3]) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    for file in stdio.iter().filter(|file| file.is_terminal()) {
        // SAFETY: the borrowed descriptor remains open during these queries.
        if unsafe { libc::tcgetpgrp(file.as_raw_fd()) != libc::getpgrp() } {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "IPC terminal use requires the execution host and client in the foreground group",
            ));
        }
    }
    Ok(())
}
