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

//! A Run-owned listener. Its private endpoint binds an immutable authority
//! ceiling; native descendants need only inherit the endpoint name. Closing
//! the owner closes admission, disconnects delegates, and joins their teardown.

use std::collections::HashMap;
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use rustix::event::{PollFd, PollFlags, poll};

use crate::policy::Policy;
use crate::{Engine, RunOptions};

#[derive(Default)]
struct Connections {
    closed: bool,
    streams: HashMap<usize, UnixStream>,
}

pub(crate) struct Session {
    directory: tempfile::TempDir,
    wake: UnixStream,
    connections: Arc<Mutex<Connections>>,
    listener: Option<JoinHandle<()>>,
}

impl Session {
    pub(crate) fn start(
        engine: Engine,
        policy: Option<Policy>,
        child_signal_mask: libc::sigset_t,
    ) -> anyhow::Result<Self> {
        let directory = tempfile::Builder::new().prefix("moonrun-").tempdir()?;
        let listener = UnixListener::bind(directory.path().join("ipc"))?;
        let (wake, interrupted) = UnixStream::pair()?;
        let connections = Arc::new(Mutex::new(Connections::default()));
        let state = Arc::clone(&connections);
        let (stack, thread_stack) = engine.execution_stack()?;
        let worker = std::thread::Builder::new()
            .name("moonrun-ipc-listener".into())
            .spawn(move || {
                std::thread::scope(|scope| {
                    let mut workers: Vec<std::thread::ScopedJoinHandle<'_, ()>> = Vec::new();
                    for id in 0..usize::MAX {
                        let mut events = [
                            PollFd::new(&listener, PollFlags::IN),
                            PollFd::new(&interrupted, PollFlags::IN),
                        ];
                        if rustix::io::retry_on_intr(|| poll(&mut events, None)).is_err()
                            || !events[1].revents().is_empty()
                        {
                            break;
                        }
                        let connection = listener.accept().map(|(stream, _)| stream);
                        let mut current = state.lock().unwrap();
                        if current.closed {
                            break;
                        }
                        let connection = match connection {
                            Ok(connection) => connection,
                            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                                continue;
                            }
                            Err(_) => break,
                        };
                        // Bound admission without moving resource exhaustion to
                        // an unbounded queue of blocked handshake threads.
                        if current.streams.len() >= 128 {
                            continue;
                        }
                        let Ok(control) = connection.try_clone() else {
                            continue;
                        };
                        current.streams.insert(id, control);
                        drop(current);
                        let state = &state;
                        let engine = &engine;
                        let policy = policy.clone();
                        let worker = std::thread::Builder::new()
                            .name("moonrun-ipc-run".into())
                            .stack_size(thread_stack)
                            .spawn_scoped(scope, move || {
                                let mut options = RunOptions::default().with_ipc(true);
                                if let Some(policy) = policy {
                                    options = options.with_parent_policy(policy);
                                }
                                options.native_stack_budget = Some(stack);
                                options.runtime.preserved_child_signal_mask =
                                    Some(child_signal_mask);
                                // Bound idle handshakes. serve clears the inactivity
                                // timeout before waiting for ordinary controls.
                                let _ = connection.set_read_timeout(Some(Duration::from_secs(30)));
                                let _ = super::serve(engine, connection, options);
                                state.lock().unwrap().streams.remove(&id);
                            });
                        match worker {
                            Ok(worker) => workers.push(worker),
                            Err(_) => {
                                state.lock().unwrap().streams.remove(&id);
                            }
                        }
                        // Reap finished threads during long-lived Runs, not only at exit.
                        let mut index = 0;
                        while index < workers.len() {
                            if workers[index].is_finished() {
                                let _ = workers.swap_remove(index).join();
                            } else {
                                index += 1;
                            }
                        }
                    }
                    let mut current = state.lock().unwrap();
                    current.closed = true;
                    for stream in current.streams.values() {
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                    drop(current);
                    for worker in workers {
                        let _ = worker.join();
                    }
                });
            })?;
        Ok(Self {
            directory,
            wake,
            connections,
            listener: Some(worker),
        })
    }

    pub(crate) fn endpoint(&self) -> std::path::PathBuf {
        self.directory.path().join("ipc")
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let mut state = self.connections.lock().unwrap();
        state.closed = true;
        for stream in state.streams.values() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        drop(state);
        // A private wake handle works even when the public backlog is full.
        let _ = self.wake.shutdown(Shutdown::Both);
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

#[cfg(all(test, feature = "v8"))]
mod tests {
    use super::*;

    #[test]
    fn teardown_disconnects_clients_that_do_not_read_errors() {
        const CHILD: &str = "MOONRUN_IPC_UNREAD_REPLY_TEST";
        if std::env::var_os(CHILD).is_none() {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "ipc::session::tests::teardown_disconnects_clients_that_do_not_read_errors",
                ])
                .env(CHILD, "1")
                .timeout(Duration::from_secs(10))
                .assert()
                .success();
            return;
        }
        use std::io::Read;
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("empty.wasm");
        std::fs::write(&program, b"\0asm\x01\0\0\0").unwrap();
        for execution_started in [false, true] {
            let session = Session::start(
                Engine::default(),
                None,
                crate::runtime::current_signal_mask().unwrap(),
            )
            .unwrap();
            let endpoint = session.endpoint();
            let mut connection = UnixStream::connect(&endpoint).unwrap();
            rustix::net::sockopt::set_socket_recv_buffer_size(&connection, 4096).unwrap();
            let stdio = [
                tempfile::tempfile().unwrap(),
                tempfile::tempfile().unwrap(),
                tempfile::tempfile().unwrap(),
            ];
            super::super::wire::send_stdio(&connection, &stdio).unwrap();
            let request = if execution_started {
                // Invalid test arguments fail inside execution's control scope:
                // StopReader has already shut down reads before sending this reply.
                serde_json::to_value(super::super::wire::Request {
                    version: super::super::wire::VERSION,
                    invocation: crate::ipc::Invocation {
                        program: program.clone(), working_directory: directory.path().to_owned(),
                        args: Vec::new(), environment: Vec::new(), inherited_policy: None,
                        no_stack_trace: false, stack_size: None,
                        test_args: Some(serde_json::json!({"package":"test", "file_and_index":"x".repeat(800_000)}).to_string()),
                    },
                }).unwrap()
            } else {
                // deny_unknown_fields includes this name in its error reply.
                serde_json::json!({
                    "version": super::super::wire::VERSION,
                    "invocation": { "x".repeat(800_000): true },
                })
            };
            super::super::wire::write(&connection, &request).unwrap();
            connection
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut prefix = [0; 4];
            connection.read_exact(&mut prefix).unwrap();
            assert!(u32::from_be_bytes(prefix) > 700_000);
            // Stop reading after the header. Teardown must not wait for the
            // rest to fit in the client's buffer, including on Darwin after
            // the control reader has already shut down reads.
            drop(session);
            assert!(!endpoint.exists());
            assert!(!endpoint.parent().unwrap().exists());
        }
    }
}
