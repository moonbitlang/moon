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

//! Process-owned Job state submitted to moonrun's shared thread pool.

use std::ffi::OsString;
use std::sync::Arc;

use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::resource::Resource;
use crate::resource::{ResourcePublication, ResourceRef};
use crate::runtime::{Stdio, StdioStream};

use super::executable::Executable;
use super::{HostProcess, ambient};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpawnOptions {
    #[cfg(unix)]
    pub(crate) child_signal_mask: libc::sigset_t,
    #[cfg(windows)]
    pub(crate) no_console_window: bool,
    #[cfg(windows)]
    pub(crate) is_orphan: bool,
}

/// Guest inputs remain mutable until HostProcess prepares the operation.
/// The child environment is independent of the caller's executable search PATH.
#[derive(Debug, Clone)]
struct SpawnRequest {
    #[cfg(unix)]
    path: OsString,
    #[cfg(unix)]
    args: Vec<OsString>,
    #[cfg(unix)]
    env: Vec<OsString>,
    #[cfg(windows)]
    command_line: OsString,
    #[cfg(windows)]
    env: Vec<u16>,
    options: SpawnOptions,
    stdio: [Option<ResourceRef>; 3],
    cwd: Option<OsString>,
}

#[derive(Debug)]
struct NativeSpawn {
    // Keep the original argv/command line: lookup must not rewrite guest argv0
    // or change the lexical request that the process policy authorized.
    request: SpawnRequest,
    // Runtime stdio Resources borrow descriptors. Pin their owner even if
    // HostProcess is released between preparation and native execution.
    _stdio_owner: Arc<Stdio>,
    executable: Executable,
    policy_inheritance: Option<crate::policy::PolicyInheritance>,
}

#[derive(Debug)]
pub(crate) struct Job {
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    PendingSpawn(Box<SpawnRequest>),
    NativeSpawn {
        // Consumed once by execution; the variant retains native provenance
        // until the result is published or its unclaimed child is revoked.
        spawn: Option<Box<NativeSpawn>>,
        result: Option<ResourcePublication>,
    },
    SpawnMoon {
        request: Box<super::moon::Spawn>,
        result: Option<ResourcePublication>,
    },
    WaitForVirtualProcess(crate::process_tree::ProcessWait),
    WaitForProcess {
        handle: Option<ResourceRef>,
        // Host-derived identity for policy checks; never supplied by the guest.
        tracked_pid: Option<i32>,
        pid: i32,
        #[cfg(unix)]
        defer_reap: bool,
        #[cfg(windows)]
        cancel: Option<ResourceRef>,
    },
}

impl Job {
    pub(crate) fn wait_for_virtual_process(wait: crate::process_tree::ProcessWait) -> Self {
        Self {
            kind: Kind::WaitForVirtualProcess(wait),
        }
    }

    pub(crate) fn cancellation_override(
        &self,
    ) -> Option<crate::async_sys::internal::event_loop::thread_pool::JobCancellation> {
        match &self.kind {
            Kind::WaitForVirtualProcess(wait) => Some(wait.cancellation()),
            _ => None,
        }
    }

    pub(crate) fn consume_wait_result(&mut self) {
        if let Kind::WaitForVirtualProcess(wait) = &mut self.kind {
            wait.consume();
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(unix)]
    pub(crate) fn spawn_unix(
        path: OsString,
        args: Vec<OsString>,
        env: Vec<OsString>,
        stdin: Option<ResourceRef>,
        stdout: Option<ResourceRef>,
        stderr: Option<ResourceRef>,
        cwd: Option<OsString>,
        options: SpawnOptions,
    ) -> Self {
        Self {
            kind: Kind::PendingSpawn(Box::new(SpawnRequest {
                path,
                args,
                env,
                options,
                stdio: [stdin, stdout, stderr],
                cwd,
            })),
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(windows)]
    pub(crate) fn spawn_windows(
        command_line: OsString,
        env: Vec<u16>,
        stdin: Option<ResourceRef>,
        stdout: Option<ResourceRef>,
        stderr: Option<ResourceRef>,
        cwd: Option<OsString>,
        options: SpawnOptions,
    ) -> Self {
        Self {
            kind: Kind::PendingSpawn(Box::new(SpawnRequest {
                command_line,
                env,
                options,
                stdio: [stdin, stdout, stderr],
                cwd,
            })),
        }
    }

    pub(crate) fn wait_for_process(
        handle: Option<ResourceRef>,
        tracked_pid: Option<i32>,
        pid: i32,
        #[cfg(unix)] defer_reap: bool,
    ) -> AsyncHostResult<Self> {
        Ok(Self {
            kind: Kind::WaitForProcess {
                handle,
                tracked_pid,
                pid,
                #[cfg(unix)]
                defer_reap,
                #[cfg(windows)]
                cancel: Some(Arc::new(ambient::make_wait_for_process_cancel()?)),
            },
        })
    }

    pub(crate) fn set_cwd(&mut self, cwd: OsString) -> AsyncHostResult<()> {
        let Kind::PendingSpawn(request) = &mut self.kind else {
            return Err(AsyncHostError::Badf);
        };
        request.cwd = Some(cwd);
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn set_no_console_window(&mut self) -> AsyncHostResult<()> {
        let Kind::PendingSpawn(request) = &mut self.kind else {
            return Err(AsyncHostError::Badf);
        };
        request.options.no_console_window = true;
        Ok(())
    }

    pub(crate) fn take_spawn_result(&mut self) -> AsyncHostResult<Option<ResourcePublication>> {
        match &mut self.kind {
            Kind::NativeSpawn { result, .. } | Kind::SpawnMoon { result, .. } => Ok(result.take()),
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn set_spawn_result(
        &mut self,
        resource: ResourcePublication,
    ) -> AsyncHostResult<()> {
        match &mut self.kind {
            Kind::NativeSpawn { result, .. } | Kind::SpawnMoon { result, .. } => {
                *result = Some(resource);
                Ok(())
            }
            _ => Err(AsyncHostError::Badf),
        }
    }

    #[cfg(windows)]
    pub(crate) fn cancellation_resource(&self) -> Option<ResourceRef> {
        match &self.kind {
            Kind::WaitForProcess {
                cancel: Some(cancel),
                ..
            } => Some(Arc::clone(cancel)),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn cwd(&self) -> AsyncHostResult<Option<&std::ffi::OsStr>> {
        match &self.kind {
            Kind::PendingSpawn(request) => Ok(request.cwd.as_deref()),
            _ => Err(AsyncHostError::Badf),
        }
    }

    #[cfg(all(test, windows))]
    pub(crate) fn no_console_window(&self) -> AsyncHostResult<bool> {
        match &self.kind {
            Kind::PendingSpawn(request) => Ok(request.options.no_console_window),
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn run(&mut self) -> AsyncHostResult<i64> {
        match &mut self.kind {
            // No OS primitive can execute a guest request that skipped its owner.
            Kind::PendingSpawn(_) => Err(AsyncHostError::Inval),
            Kind::NativeSpawn { spawn, result } => {
                let NativeSpawn {
                    request,
                    _stdio_owner,
                    executable,
                    policy_inheritance,
                } = *spawn.take().ok_or(AsyncHostError::Inval)?;
                #[cfg(unix)]
                return ambient::run_spawn_job_unix(
                    executable,
                    request.args,
                    request.env,
                    request.stdio,
                    request.cwd,
                    request.options,
                    policy_inheritance,
                    result,
                );
                #[cfg(windows)]
                return ambient::run_spawn_job_windows(
                    executable,
                    request.command_line,
                    request.env,
                    request.stdio,
                    request.cwd,
                    request.options,
                    policy_inheritance,
                    result,
                );
            }
            Kind::SpawnMoon { request, result } => request.run(result),
            Kind::WaitForVirtualProcess(wait) => wait.run(),
            Kind::WaitForProcess {
                handle,
                tracked_pid: _,
                pid,
                #[cfg(unix)]
                defer_reap,
                #[cfg(windows)]
                cancel,
            } => ambient::run_wait_for_process_job(
                handle.take(),
                *pid,
                #[cfg(unix)]
                *defer_reap,
                #[cfg(windows)]
                cancel.take(),
            ),
        }
    }

    pub(super) fn prepare(&mut self, process: &HostProcess) -> AsyncHostResult<()> {
        match &self.kind {
            Kind::PendingSpawn(request) => {
                // The existing policy authorizes the logical invocation, before
                // lookup, handle duplication, or policy-transfer temporary I/O.
                #[cfg(unix)]
                process.check_spawn_unix(&request.path, &request.args)?;
                #[cfg(windows)]
                process.check_spawn_windows(&request.command_line)?;

                let mut request = (**request).clone();
                let parent_cwd = process.working_directory.current_dir().map_err(|error| {
                    error
                        .raw_os_error()
                        .map_or(AsyncHostError::Io, AsyncHostError::Native)
                })?;
                // Snapshot cwd once along with PATH. Relative executable search
                // uses child cwd on Unix and caller cwd on Windows. Preserve an
                // explicit empty cwd so the native primitive can reject it.
                request.cwd = Some(match request.cwd {
                    Some(cwd) if cwd.is_empty() => cwd,
                    Some(cwd) => parent_cwd.join(cwd).into_os_string(),
                    None => parent_cwd.clone().into_os_string(),
                });
                let parent_path = process.environment.get("PATH".as_ref());
                #[cfg(unix)]
                let executable = Executable::new(
                    &request.path,
                    parent_path.as_deref(),
                    request
                        .cwd
                        .as_deref()
                        .filter(|cwd| !cwd.is_empty())
                        .map(std::path::Path::new)
                        .unwrap_or(&parent_cwd),
                )?;
                #[cfg(windows)]
                let executable = Executable::new(
                    &request.command_line,
                    parent_path.as_deref(),
                    &parent_cwd,
                    process
                        .environment
                        .get("NoDefaultCurrentDirectoryInExePath".as_ref())
                        .is_none(),
                )?;
                request.configure_stdio(&process.stdio)?;
                self.kind = if let Some(context) = &process.context {
                    #[cfg(unix)]
                    let spawn = super::moon::Spawn::unix(
                        context.clone(),
                        executable,
                        request.args,
                        request.env,
                        request.stdio,
                        request.cwd,
                        request.options,
                        process.policy_inheritance.clone(),
                    )?;
                    #[cfg(windows)]
                    let spawn = super::moon::Spawn::windows(
                        context.clone(),
                        executable,
                        request.env,
                        request.stdio,
                        request.cwd,
                        request.options,
                        process.policy_inheritance.clone(),
                    )?;
                    Kind::SpawnMoon {
                        request: Box::new(spawn),
                        result: None,
                    }
                } else {
                    let invokes_moonx =
                        moonutil::constants::is_moonx_executable(executable.program());
                    Kind::NativeSpawn {
                        spawn: Some(Box::new(NativeSpawn {
                            request,
                            _stdio_owner: Arc::clone(&process.stdio),
                            executable,
                            policy_inheritance: if invokes_moonx {
                                process.policy_inheritance.clone()
                            } else {
                                None
                            },
                        })),
                        result: None,
                    }
                };
                Ok(())
            }
            Kind::WaitForProcess {
                handle,
                tracked_pid,
                pid,
                ..
            } => process.check_wait(handle.is_some(), *tracked_pid, *pid),
            Kind::WaitForVirtualProcess(_) => Ok(()),
            Kind::NativeSpawn { .. } | Kind::SpawnMoon { .. } => Err(AsyncHostError::Inval),
        }
    }

    pub(super) fn finish(&self, process: &HostProcess, ret: i64, err: i32) -> AsyncHostResult<()> {
        if err != 0 {
            return Ok(());
        }
        match &self.kind {
            Kind::NativeSpawn { .. } if ret >= 0 => process.track_spawned_child(ret as i32),
            Kind::WaitForProcess {
                pid,
                #[cfg(unix)]
                defer_reap,
                ..
            } => {
                process.finish_waited_child(
                    *pid,
                    #[cfg(unix)]
                    *defer_reap,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn revoke_unclaimed_spawn(&self, process: &HostProcess, ret: i64, err: i32) {
        if err != 0 || ret < 0 {
            return;
        }
        if let Kind::NativeSpawn { result, .. } = &self.kind
            && !matches!(result, Some(ResourcePublication::Published(_)))
        {
            process.revoke_child_if_unreferenced(ret as i32);
        }
    }
}

impl SpawnRequest {
    fn configure_stdio(&mut self, runtime_stdio: &Stdio) -> AsyncHostResult<()> {
        #[cfg(unix)]
        if matches!(runtime_stdio, Stdio::Owned(_)) {
            for (slot, stream) in self.stdio.iter_mut().zip(StdioStream::ALL) {
                if slot.is_none() {
                    let raw = runtime_stdio.raw(stream).map_err(|_| AsyncHostError::Io)?;
                    *slot = Some(Arc::new(Resource::stdio_file(raw)));
                }
            }
        }
        #[cfg(windows)]
        for (slot, stream) in self.stdio.iter_mut().zip(StdioStream::ALL) {
            // GUI processes may have no standard streams. Preserve absence
            // for CreateProcessW instead of rejecting spawn.
            if slot.is_none()
                && let Ok(raw) = runtime_stdio.raw(stream)
            {
                *slot = Some(Arc::new(crate::resource::Resource::stdio_file(raw)));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    ambient::PORTED_SYMBOLS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Env, WorkingDirectory};

    #[cfg(unix)]
    fn host(working_directory: WorkingDirectory, stdio: Stdio) -> HostProcess {
        HostProcess::new(
            None,
            None,
            Arc::new(working_directory),
            Arc::new(stdio),
            None,
            Arc::new(Env::ambient()),
        )
    }

    #[cfg(unix)]
    fn spawn_job(argv0: &str, env: Vec<OsString>) -> Job {
        Job::spawn_unix(
            argv0.into(),
            vec![argv0.into()],
            env,
            None,
            None,
            None,
            None,
            SpawnOptions {
                child_signal_mask: unsafe { std::mem::zeroed() },
            },
        )
    }

    #[cfg(unix)]
    #[test]
    fn fixed_child_cwd_preserves_explicit_empty_value() {
        let directory = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        std::fs::create_dir(root.join("child")).unwrap();
        let working_directory = WorkingDirectory::Fixed(root.clone());

        for (cwd, succeeds) in [
            (None, true),
            (Some(OsString::new()), false),
            (Some(OsString::from("child")), true),
        ] {
            let expected = if cwd.as_deref() == Some(std::ffi::OsStr::new("child")) {
                root.join("child")
            } else {
                root.clone()
            };
            let mut job = Job::spawn_unix(
                "/bin/sh".into(),
                vec![
                    "sh".into(),
                    "-c".into(),
                    "test \"$PWD\" = \"$EXPECTED\"".into(),
                ],
                vec![format!("EXPECTED={}", expected.display()).into()],
                None,
                None,
                None,
                None,
                SpawnOptions {
                    child_signal_mask: unsafe { std::mem::zeroed() },
                },
            );
            if let Some(cwd) = cwd {
                job.set_cwd(cwd).unwrap();
            }
            let process = host(working_directory.clone(), Stdio::Ambient);
            let result = process.prepare_job(&mut job).and_then(|()| job.run());
            if let Ok(pid) = result {
                let status = Job::wait_for_process(None, None, pid as i32, false)
                    .unwrap()
                    .run()
                    .unwrap();
                assert_eq!(status, 0, "child used an unexpected working directory");
            }
            assert_eq!(result.is_ok(), succeeds);
        }
    }

    #[cfg(unix)]
    #[test]
    fn owned_child_stdio_supplies_defaults_and_preserves_explicit_streams() {
        use std::fs::{self, File};
        use std::os::fd::AsRawFd;

        for explicit_stdout in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let dir = directory.path();
            fs::write(dir.join("stdin"), "input\n").unwrap();
            let stdio = Stdio::owned(
                File::open(dir.join("stdin")).unwrap(),
                File::create(dir.join("stdout")).unwrap(),
                File::create(dir.join("stderr")).unwrap(),
            );
            let output = File::create(dir.join("explicit-stdout")).unwrap();
            let stdout =
                explicit_stdout.then(|| Arc::new(Resource::stdio_file(output.as_raw_fd())));
            let mut job = Job::spawn_unix(
                "/bin/sh".into(),
                vec![
                    "sh".into(),
                    "-c".into(),
                    "IFS= read -r line; printf 'out:%s\n' \"$line\"; printf 'err:%s\n' \"$line\" >&2".into(),
                ],
                Vec::new(),
                None,
                stdout,
                None,
                None,
                SpawnOptions {
                    child_signal_mask: unsafe { std::mem::zeroed() },
                },
            );
            host(WorkingDirectory::Ambient, stdio)
                .prepare_job(&mut job)
                .unwrap();
            let pid = job.run().unwrap() as i32;
            let status = Job::wait_for_process(None, None, pid, false)
                .unwrap()
                .run()
                .unwrap();
            assert_eq!(status, 0);
            assert_eq!(fs::read(dir.join("stderr")).unwrap(), b"err:input\n");
            let (default, explicit) = if explicit_stdout {
                (b"".as_slice(), b"out:input\n".as_slice())
            } else {
                (b"out:input\n".as_slice(), b"".as_slice())
            };
            assert_eq!(fs::read(dir.join("stdout")).unwrap(), default);
            assert_eq!(fs::read(dir.join("explicit-stdout")).unwrap(), explicit);
        }
    }

    #[cfg(unix)]
    #[test]
    fn prepared_spawn_keeps_its_lookup_after_runtime_environment_changes() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        for (name, code) in [("first", 17), ("second", 29)] {
            let bin = directory.path().join(name);
            std::fs::create_dir(&bin).unwrap();
            let executable = bin.join("probe");
            std::fs::write(&executable, format!("#!/bin/sh\nexit {code}\n")).unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let environment = Arc::new(Env::owned([("PATH".into(), "first".into())]).unwrap());
        let process = HostProcess::new(
            None,
            None,
            Arc::new(WorkingDirectory::Fixed(directory.path().to_owned())),
            Arc::new(Stdio::Ambient),
            None,
            Arc::clone(&environment),
        );
        let mut job = spawn_job("probe", Vec::new());
        process.prepare_job(&mut job).unwrap();
        environment.set("PATH".into(), "second".into()).unwrap();
        drop(process);
        let pid = job.run().unwrap() as i32;
        assert_eq!(
            Job::wait_for_process(None, None, pid, false).unwrap().run(),
            Ok(17)
        );
    }

    #[test]
    fn only_direct_moonx_invocations_prepare_policy_inheritance() {
        let directory = tempfile::tempdir().unwrap();
        let environment =
            Env::owned([("PATH".into(), directory.path().as_os_str().to_owned())]).unwrap();
        let (mut policy, _) =
            crate::policy::load_inherited_json(br#"{"process":{"spawn":true}}"#, &environment)
                .unwrap();
        let process = HostProcess::new(
            policy.take_process_policy(),
            policy.take_policy_inheritance(),
            Arc::new(WorkingDirectory::Fixed(directory.path().to_owned())),
            Arc::new(Stdio::Ambient),
            None,
            Arc::new(environment),
        );
        for program in ["moonx", "moon"] {
            #[cfg(windows)]
            std::fs::copy(
                std::env::current_exe().unwrap(),
                directory.path().join(format!("{program}.exe")),
            )
            .unwrap();
            #[cfg(unix)]
            let mut job = spawn_job(program, vec!["MOONRUN_INHERITED_POLICY=guest".into()]);
            #[cfg(windows)]
            let mut job = Job::spawn_windows(
                program.into(),
                vec![0, 0],
                None,
                None,
                None,
                None,
                SpawnOptions {
                    no_console_window: false,
                    is_orphan: false,
                },
            );
            // Native Unix argv0 is independent of the executable request.
            #[cfg(unix)]
            if let Kind::PendingSpawn(request) = &mut job.kind {
                request.args[0] = "moonx".into();
            }
            process.prepare_job(&mut job).unwrap();
            let Kind::NativeSpawn {
                spawn: Some(spawn), ..
            } = &job.kind
            else {
                panic!("expected native spawn")
            };
            assert_eq!(spawn.policy_inheritance.is_some(), program == "moonx");
            assert_eq!(job.set_cwd("changed".into()), Err(AsyncHostError::Badf));
        }
    }

    #[cfg(unix)]
    #[test]
    fn unprepared_spawn_cannot_execute() {
        let mut job = spawn_job("/bin/sh", Vec::new());
        assert_eq!(job.run(), Err(AsyncHostError::Inval));
    }

    #[cfg(unix)]
    #[test]
    fn ambient_stdio_preserves_native_inheritance() {
        let mut job = spawn_job("moon", Vec::new());

        let Kind::PendingSpawn(request) = &mut job.kind else {
            unreachable!()
        };
        request.configure_stdio(&Stdio::Ambient).unwrap();
        assert!(request.stdio.iter().all(Option::is_none));
    }

    #[cfg(windows)]
    #[test]
    fn ambient_stdio_fills_missing_handles_without_replacing_guest_streams() {
        let explicit_stdin = Arc::new(crate::resource::Resource::stdio_file(
            Stdio::Ambient.raw(StdioStream::Stdin).unwrap(),
        ));
        let mut job = Job::spawn_windows(
            OsString::from("cmd.exe /D /C exit 0"),
            vec![0, 0],
            Some(Arc::clone(&explicit_stdin)),
            None,
            None,
            None,
            SpawnOptions {
                no_console_window: false,
                is_orphan: false,
            },
        );

        let Kind::PendingSpawn(request) = &mut job.kind else {
            unreachable!()
        };
        request.configure_stdio(&Stdio::Ambient).unwrap();
        assert!(Arc::ptr_eq(
            request.stdio[0].as_ref().unwrap(),
            &explicit_stdin
        ));
        assert!(request.stdio[1..].iter().all(Option::is_some));
    }

    #[test]
    fn job_executors_reference_native_worker_symbols() {
        let async_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../third_party/moonbitlang_async");
        for symbol in ported_symbols() {
            let source_path = async_root.join(symbol.source);
            let contents = std::fs::read_to_string(&source_path)
                .unwrap_or_else(|error| panic!("failed to read {:?}: {error}", source_path));
            assert!(
                contents.contains(symbol.native_symbol),
                "{:?} does not contain native worker symbol {} for {}::{}",
                source_path,
                symbol.native_symbol,
                symbol.rust_module,
                symbol.rust_symbol
            );
        }
    }
}
