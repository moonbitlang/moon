// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Process-owned Job state submitted to moonrun's shared thread pool.

#[cfg(windows)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::sync::Arc;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::resource::{ResourcePublication, ResourceRef};
use crate::runtime::{ChildStdio, Stdio, StdioBindings, WorkingDirectory};

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

#[derive(Debug)]
pub(crate) struct Job {
    kind: Kind,
}

#[derive(Debug)]
// Unix's native signal mask makes SpawnUnix substantially larger than a Wait.
// Keep it inline: this extraction does not need to add a per-Job allocation.
#[allow(clippy::large_enum_variant)]
enum Kind {
    #[cfg(unix)]
    SpawnUnix {
        path: OsString,
        args: Vec<OsString>,
        env: Vec<OsString>,
        options: SpawnOptions,
        stdio: [Option<ResourceRef>; 3],
        runtime_stdio: Option<Arc<StdioBindings>>,
        cwd: Option<OsString>,
        policy_inheritance: Option<crate::policy::PolicyInheritance>,
        result: Option<ResourcePublication>,
    },
    #[cfg(windows)]
    SpawnWindows {
        command_line: OsString,
        env: Vec<u16>,
        options: SpawnOptions,
        stdio: [Option<ResourceRef>; 3],
        runtime_stdio: Option<Arc<StdioBindings>>,
        cwd: Option<OsString>,
        policy_inheritance: Option<crate::policy::PolicyInheritance>,
        result: Option<ResourcePublication>,
    },
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
            kind: Kind::SpawnUnix {
                path,
                args,
                env,
                options,
                stdio: [stdin, stdout, stderr],
                runtime_stdio: None,
                cwd,
                policy_inheritance: None,
                result: None,
            },
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
            kind: Kind::SpawnWindows {
                command_line,
                env,
                options,
                stdio: [stdin, stdout, stderr],
                runtime_stdio: None,
                cwd,
                policy_inheritance: None,
                result: None,
            },
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
        match &mut self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix { cwd: job_cwd, .. } => {
                *job_cwd = Some(cwd);
                Ok(())
            }
            #[cfg(windows)]
            Kind::SpawnWindows { cwd: job_cwd, .. } => {
                *job_cwd = Some(cwd);
                Ok(())
            }
            Kind::WaitForProcess { .. } => Err(AsyncHostError::Badf),
        }
    }

    #[cfg(windows)]
    pub(crate) fn set_no_console_window(&mut self) -> AsyncHostResult<()> {
        match &mut self.kind {
            Kind::SpawnWindows { options, .. } => {
                options.no_console_window = true;
                Ok(())
            }
            Kind::WaitForProcess { .. } => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn take_spawn_result(&mut self) -> AsyncHostResult<Option<ResourcePublication>> {
        match &mut self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix { result, .. } => Ok(result.take()),
            #[cfg(windows)]
            Kind::SpawnWindows { result, .. } => Ok(result.take()),
            Kind::WaitForProcess { .. } => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn set_spawn_result(
        &mut self,
        resource: ResourcePublication,
    ) -> AsyncHostResult<()> {
        match &mut self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix { result, .. } => {
                *result = Some(resource);
                Ok(())
            }
            #[cfg(windows)]
            Kind::SpawnWindows { result, .. } => {
                *result = Some(resource);
                Ok(())
            }
            Kind::WaitForProcess { .. } => Err(AsyncHostError::Badf),
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
            #[cfg(unix)]
            Kind::SpawnUnix { cwd, .. } => Ok(cwd.as_deref()),
            #[cfg(windows)]
            Kind::SpawnWindows { cwd, .. } => Ok(cwd.as_deref()),
            Kind::WaitForProcess { .. } => Err(AsyncHostError::Badf),
        }
    }

    #[cfg(all(test, windows))]
    pub(crate) fn no_console_window(&self) -> AsyncHostResult<bool> {
        match &self.kind {
            Kind::SpawnWindows { options, .. } => Ok(options.no_console_window),
            Kind::WaitForProcess { .. } => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn run(&mut self) -> AsyncHostResult<i64> {
        let invokes_moonx = self.invokes_moonx();
        match &mut self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix {
                path,
                args,
                env,
                options,
                stdio,
                runtime_stdio,
                cwd,
                policy_inheritance,
                result,
            } => {
                if let Some(runtime_stdio) = runtime_stdio.take() {
                    apply_runtime_stdio(&runtime_stdio, stdio)?;
                }
                ambient::run_spawn_job_unix(
                    std::mem::take(path),
                    std::mem::take(args),
                    std::mem::take(env),
                    std::mem::take(stdio),
                    cwd.take(),
                    *options,
                    if invokes_moonx {
                        policy_inheritance.take()
                    } else {
                        None
                    },
                    result,
                )
            }
            #[cfg(windows)]
            Kind::SpawnWindows {
                command_line,
                env,
                options,
                stdio,
                runtime_stdio,
                cwd,
                policy_inheritance,
                result,
            } => {
                if let Some(runtime_stdio) = runtime_stdio.take() {
                    apply_runtime_stdio(&runtime_stdio, stdio)?;
                }
                ambient::run_spawn_job_windows(
                    std::mem::take(command_line),
                    std::mem::take(env),
                    std::mem::take(stdio),
                    cwd.take(),
                    *options,
                    if invokes_moonx {
                        policy_inheritance.take()
                    } else {
                        None
                    },
                    result,
                )
            }
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

    pub(super) fn configure_working_directory(&mut self, working_directory: &WorkingDirectory) {
        match &mut self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix { cwd, .. } => working_directory.configure_child_cwd(cwd),
            #[cfg(windows)]
            Kind::SpawnWindows { cwd, .. } => working_directory.configure_child_cwd(cwd),
            Kind::WaitForProcess { .. } => {}
        }
    }

    pub(super) fn set_stdio_bindings(&mut self, stdio: Arc<StdioBindings>) {
        match &mut self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix { runtime_stdio, .. } => *runtime_stdio = Some(stdio),
            #[cfg(windows)]
            Kind::SpawnWindows { runtime_stdio, .. } => *runtime_stdio = Some(stdio),
            Kind::WaitForProcess { .. } => {}
        }
    }

    pub(super) fn invokes_moonx(&self) -> bool {
        match &self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix { path, .. } => moonutil::constants::is_moonx_executable(path),
            #[cfg(windows)]
            Kind::SpawnWindows { command_line, .. } => {
                moonutil::constants::is_moonx_executable(OsStr::new(
                    &moonutil::shlex::get_argv0_windows(&command_line.to_string_lossy()),
                ))
            }
            Kind::WaitForProcess { .. } => false,
        }
    }

    pub(super) fn set_policy_inheritance(
        &mut self,
        inheritance: Option<crate::policy::PolicyInheritance>,
    ) {
        let slot = match &mut self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix {
                policy_inheritance, ..
            } => Some(policy_inheritance),
            #[cfg(windows)]
            Kind::SpawnWindows {
                policy_inheritance, ..
            } => Some(policy_inheritance),
            Kind::WaitForProcess { .. } => None,
        };
        if let Some(slot) = slot {
            *slot = inheritance;
        }
    }

    pub(super) fn check_policy(&self, process: &HostProcess) -> AsyncHostResult<()> {
        match &self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix { path, args, .. } => process.check_spawn_unix(path.as_os_str(), args),
            #[cfg(windows)]
            Kind::SpawnWindows { command_line, .. } => {
                process.check_spawn_windows(command_line.as_os_str())
            }
            Kind::WaitForProcess {
                handle,
                tracked_pid,
                pid,
                ..
            } => process.check_wait(handle.is_some(), *tracked_pid, *pid),
        }
    }

    pub(super) fn finish(&self, process: &HostProcess, ret: i64, err: i32) -> AsyncHostResult<()> {
        if err != 0 {
            return Ok(());
        }
        match &self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix { .. } => {
                if ret >= 0 {
                    process.track_spawned_child(ret as i32);
                }
            }
            #[cfg(windows)]
            Kind::SpawnWindows { .. } => {
                if ret >= 0 {
                    process.track_spawned_child(ret as i32);
                }
            }
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
        }
        Ok(())
    }

    pub(super) fn revoke_unclaimed_spawn(&self, process: &HostProcess, ret: i64, err: i32) {
        if err != 0 || ret < 0 {
            return;
        }
        let unclaimed = match &self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix { result, .. } => {
                !matches!(result, Some(ResourcePublication::Published(_)))
            }
            #[cfg(windows)]
            Kind::SpawnWindows { result, .. } => {
                !matches!(result, Some(ResourcePublication::Published(_)))
            }
            Kind::WaitForProcess { .. } => false,
        };
        if unclaimed {
            process.revoke_child_if_unreferenced(ret as i32);
        }
    }
}

fn apply_runtime_stdio(
    runtime_stdio: &StdioBindings,
    slots: &mut [Option<ResourceRef>; 3],
) -> AsyncHostResult<()> {
    for (slot, stream) in slots.iter_mut().zip(Stdio::ALL) {
        if slot.is_none() {
            let child = runtime_stdio.child(stream).map_err(|error| {
                error
                    .raw_os_error()
                    .map_or(AsyncHostError::Io, AsyncHostError::Native)
            })?;
            match child {
                #[cfg(unix)]
                ChildStdio::Inherit => {}
                #[cfg(windows)]
                ChildStdio::Handle(raw) => {
                    *slot = Some(Arc::new(crate::resource::Resource::stdio_file(raw)));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    ambient::PORTED_SYMBOLS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn direct_moonx_spawn_is_recognized_after_guest_environment() {
        let job = spawn_job(
            "moonx",
            vec!["MOONRUN_INHERITED_POLICY=guest".into(), "KEEP=value".into()],
        );

        assert!(job.invokes_moonx());
    }

    #[cfg(unix)]
    #[test]
    fn non_moonx_spawn_does_not_inherit_policy() {
        let job = spawn_job("moon", vec!["KEEP=value".into()]);

        assert!(!job.invokes_moonx());
    }

    #[cfg(unix)]
    #[test]
    fn guest_argv0_cannot_disguise_another_executable_as_moonx() {
        let job = Job::spawn_unix(
            "/bin/sh".into(),
            vec!["moonx".into()],
            Vec::new(),
            None,
            None,
            None,
            None,
            SpawnOptions {
                child_signal_mask: unsafe { std::mem::zeroed() },
            },
        );

        assert!(!job.invokes_moonx());
    }

    #[cfg(windows)]
    #[test]
    fn windows_embedded_quotes_cannot_bypass_moonx_inheritance() {
        let job = Job::spawn_windows(
            OsString::from(r#""moon"x user/module"#),
            "moonrun_inherited_policy=guest\0KEEP=value\0\0"
                .encode_utf16()
                .collect(),
            None,
            None,
            None,
            None,
            SpawnOptions {
                no_console_window: false,
                is_orphan: false,
            },
        );

        assert!(job.invokes_moonx());
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
