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

mod runner;

use std::ffi::OsString;
#[cfg(windows)]
use std::sync::Arc;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::resource::{ResourcePublication, ResourceRef};

use super::HostProcess;

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
        cwd: Option<OsString>,
        inherited_policy: Option<crate::policy::PolicySnapshot>,
        result: Option<ResourcePublication>,
    },
    #[cfg(windows)]
    SpawnWindows {
        command_line: OsString,
        env: Vec<u16>,
        options: SpawnOptions,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        inherited_policy: Option<crate::policy::PolicySnapshot>,
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
                cwd,
                inherited_policy: None,
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
                cwd,
                inherited_policy: None,
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
                cancel: Some(Arc::new(runner::make_wait_for_process_cancel()?)),
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
        match &mut self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix {
                path,
                args,
                env,
                options,
                stdio,
                cwd,
                inherited_policy,
                result,
            } => runner::run_spawn_job_unix(
                std::mem::take(path),
                std::mem::take(args),
                std::mem::take(env),
                std::mem::take(stdio),
                cwd.take(),
                inherited_policy.take(),
                *options,
                result,
            ),
            #[cfg(windows)]
            Kind::SpawnWindows {
                command_line,
                env,
                options,
                stdio,
                cwd,
                inherited_policy,
                result,
            } => runner::run_spawn_job_windows(
                std::mem::take(command_line),
                std::mem::take(env),
                std::mem::take(stdio),
                cwd.take(),
                inherited_policy.take(),
                *options,
                result,
            ),
            Kind::WaitForProcess {
                handle,
                tracked_pid: _,
                pid,
                #[cfg(unix)]
                defer_reap,
                #[cfg(windows)]
                cancel,
            } => runner::run_wait_for_process_job(
                handle.take(),
                *pid,
                #[cfg(unix)]
                *defer_reap,
                #[cfg(windows)]
                cancel.take(),
            ),
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

    pub(super) fn prepare_inherited_moonx(
        &mut self,
        policy: &crate::policy::Policy,
    ) -> AsyncHostResult<()> {
        match &mut self.kind {
            #[cfg(unix)]
            Kind::SpawnUnix {
                args,
                env,
                inherited_policy,
                ..
            } => {
                crate::async_sys::process::set_process_env_var(
                    env,
                    std::ffi::OsStr::new(moonutil::constants::MOONRUN_INHERITED_POLICY),
                    None,
                );
                if !args
                    .first()
                    .is_some_and(|arg| moonutil::constants::is_moonx_executable(arg))
                {
                    return Ok(());
                }
                let Some(snapshot) = policy.snapshot_for_child().map_err(|error| {
                    eprintln!("failed to snapshot sandbox policy: {error:#}");
                    AsyncHostError::Io
                })?
                else {
                    return Ok(());
                };
                *inherited_policy = Some(snapshot);
                Ok(())
            }
            #[cfg(windows)]
            Kind::SpawnWindows {
                command_line,
                env,
                inherited_policy,
                ..
            } => {
                crate::async_sys::process::set_process_env_var(
                    env,
                    std::ffi::OsStr::new(moonutil::constants::MOONRUN_INHERITED_POLICY),
                    None,
                );
                let argv0 = moonutil::shlex::get_argv0_windows(&command_line.to_string_lossy());
                if !moonutil::constants::is_moonx_executable(std::ffi::OsStr::new(&argv0)) {
                    return Ok(());
                }
                let Some(snapshot) = policy.snapshot_for_child().map_err(|error| {
                    eprintln!("failed to snapshot sandbox policy: {error:#}");
                    AsyncHostError::Io
                })?
                else {
                    return Ok(());
                };
                *inherited_policy = Some(snapshot);
                Ok(())
            }
            Kind::WaitForProcess { .. } => Ok(()),
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

#[cfg(windows)]
pub(super) fn cancel_wait(cancel: &ResourceRef) -> AsyncHostResult<()> {
    runner::cancel_wait_for_process(cancel)
}

#[cfg(test)]
fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    runner::PORTED_SYMBOLS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[cfg(unix)]
    #[test]
    fn policy_bearing_moonx_spawn_receives_a_snapshot_without_rewriting_the_command() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(
            &policy_file,
            r#"
[env.set]
UNICODE = "策略-🌙"

[process]
spawn = true
"#,
        )
        .unwrap();
        let policy = crate::policy::Policy::from_file(&policy_file).unwrap();
        let mut job = Job::spawn_unix(
            OsString::from("moon"),
            vec![
                OsString::from("moonx"),
                OsString::from("user/module"),
                OsString::from("参数-🌙"),
            ],
            vec![OsString::from("MOONRUN_INHERITED_POLICY=guest")],
            None,
            None,
            None,
            None,
            SpawnOptions {
                child_signal_mask: unsafe { std::mem::zeroed() },
            },
        );

        job.prepare_inherited_moonx(&policy).unwrap();

        let Kind::SpawnUnix {
            path,
            args,
            env,
            inherited_policy,
            ..
        } = &job.kind
        else {
            unreachable!()
        };
        assert_eq!(path, "moon");
        assert_eq!(args, &["moonx", "user/module", "参数-🌙"]);
        assert!(env.is_empty());
        assert!(inherited_policy.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn ordinary_spawn_cannot_supply_inherited_policy_capability() {
        let mut job = Job::spawn_unix(
            OsString::from("program"),
            vec![OsString::from("program")],
            vec![OsString::from("MOONRUN_INHERITED_POLICY=guest")],
            None,
            None,
            None,
            None,
            SpawnOptions {
                child_signal_mask: unsafe { std::mem::zeroed() },
            },
        );

        job.prepare_inherited_moonx(&crate::policy::Policy::allow_all())
            .unwrap();

        let Kind::SpawnUnix { env, .. } = &job.kind else {
            unreachable!()
        };
        assert!(env.is_empty());
    }
}
