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
//! This is not a listening daemon or an authentication protocol. Giving a peer
//! a connection delegates the ability to select a Wasm program to run with the
//! host-supplied restrictions. See `docs/dev/design/ipc-execution.md`.

#[cfg(test)]
mod tests;
mod wire;

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
    /// Submit exactly one invocation on a fresh, connected Unix stream.
    /// Files and pipes are supported; terminal bindings are rejected until
    /// foreground process-group and signal ownership have an explicit contract.
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
        if stdio.iter().any(File::is_terminal) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "IPC terminal stdio is not supported",
            ));
        }
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
    /// Send a forced termination request. Success means the message was sent,
    /// not that termination was accepted or cleanup finished. The first accepted
    /// request wins; completion can race with delivery. Use `ClientRun::wait` to
    /// observe the outcome. This does not deliver cooperative guest signals.
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
        anyhow::ensure!(
            !stdio.iter().any(File::is_terminal),
            "IPC terminal stdio is not supported"
        );
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
        std::thread::scope(|scope| {
            let reader = &stream;
            let control = std::thread::Builder::new()
                .name("moonrun-ipc-control".into())
                .spawn_scoped(scope, move || {
                    loop {
                        let termination = match wire::read::<wire::Control>(reader) {
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
            let outcome = engine.run_file(invocation.program, options);
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
    wire::write(&stream, &reply).context("failed to send IPC execution outcome")
}
