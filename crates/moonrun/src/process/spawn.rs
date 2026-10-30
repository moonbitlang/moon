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

//! HostProcess interpretation and authorization of a guest spawn request.
//!
//! Ambient state stays with the native primitive when it can express the
//! Runtime's state. Explicit lookup captures only the inputs its candidates
//! need. OS queries and execution live below this module in `ambient`.

use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;

#[cfg(windows)]
use super::ambient;
use super::ambient::Executable;
use super::{HostProcess, SpawnOptions, lookup};
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::resource::{Resource, ResourceRef};
use crate::runtime::{Stdio, StdioStream, WorkingDirectory};

/// Guest inputs remain mutable until HostProcess prepares the operation.
/// The child environment is independent of the caller's executable search PATH.
#[derive(Debug)]
pub(super) struct SpawnRequest {
    #[cfg(unix)]
    pub(super) path: OsString,
    #[cfg(unix)]
    pub(super) args: Vec<OsString>,
    #[cfg(unix)]
    pub(super) env: Vec<OsString>,
    #[cfg(windows)]
    pub(super) command_line: OsString,
    #[cfg(windows)]
    pub(super) env: Vec<u16>,
    pub(super) options: SpawnOptions,
    pub(super) stdio: [Option<ResourceRef>; 3],
    pub(super) cwd: Option<OsString>,
}

#[derive(Debug)]
pub(super) struct NativeSpawn {
    // Keep the original argv/command line: lookup must not rewrite guest argv0
    // or change the lexical request that the process policy authorized.
    pub(super) request: SpawnRequest,
    // Runtime stdio Resources borrow descriptors. Pin their owner even if
    // HostProcess is released between preparation and native execution.
    pub(super) _stdio_owner: Arc<Stdio>,
    pub(super) executable: Executable,
    pub(super) policy_inheritance: Option<crate::policy::PolicyInheritance>,
}

pub(super) enum PreparedSpawn {
    Native(Box<NativeSpawn>),
    Moon(Box<super::moon::Spawn>),
}

impl HostProcess {
    pub(super) fn prepare_spawn(
        &self,
        mut request: SpawnRequest,
    ) -> AsyncHostResult<PreparedSpawn> {
        // The process policy authorizes the logical invocation before lookup,
        // handle duplication, or policy-transfer temporary I/O. Whether state
        // needs virtualization is a separate decision from authority.
        #[cfg(unix)]
        self.check_spawn_unix(&request.path, &request.args)?;
        #[cfg(windows)]
        self.check_spawn_windows(&request.command_line)?;

        if let WorkingDirectory::Fixed(root) = self.working_directory.as_ref() {
            request.cwd = Some(match request.cwd {
                Some(cwd) if cwd.is_empty() => cwd,
                Some(cwd) => root.join(cwd).into_os_string(),
                None => root.clone().into_os_string(),
            });
        }
        // Linux resolves relative images after child chdir. Windows executable
        // search and Darwin's post-spawn PATH stat still observe caller cwd, so
        // a Fixed Runtime directory needs explicit lookup on those platforms.
        let ambient_lookup = self.context.is_none()
            && self.environment.is_ambient()
            && (cfg!(target_os = "linux")
                || matches!(self.working_directory.as_ref(), WorkingDirectory::Ambient));

        if ambient_lookup {
            // Do not call getcwd just to inherit Ambient cwd: its pathname can
            // be unavailable while the directory remains usable.
            #[cfg(unix)]
            let (executable, invokes_moonx) = (
                Executable::Ambient(request.path.clone()),
                moonutil::constants::is_moonx_executable(&request.path),
            );
            #[cfg(windows)]
            let (executable, invokes_moonx) = (
                Executable::Ambient,
                if self.policy_inheritance.is_some() {
                    let arguments = ambient::lookup::parse_command_line(&request.command_line)
                        .map_err(io_error)?;
                    moonutil::constants::is_moonx_executable(&arguments[0])
                } else {
                    false
                },
            );
            request.configure_stdio(&self.stdio)?;
            return Ok(PreparedSpawn::Native(Box::new(NativeSpawn {
                request,
                _stdio_owner: Arc::clone(&self.stdio),
                executable,
                policy_inheritance: if invokes_moonx {
                    self.policy_inheritance.clone()
                } else {
                    None
                },
            })));
        }

        #[cfg(unix)]
        let plan = {
            let parent_path = (!request.path.as_encoded_bytes().contains(&b'/'))
                .then(|| self.environment.get("PATH".as_ref()))
                .flatten();
            let child_cwd = request.cwd.as_deref().map(Path::new);
            let mut plan = lookup::unix(
                &request.path,
                parent_path.as_deref(),
                child_cwd.filter(|cwd| cwd.is_absolute()),
            )
            .map_err(lookup_error)?;
            if plan.needs_cwd()
                || (self.context.is_some() && child_cwd.is_none_or(|cwd| !cwd.is_absolute()))
            {
                let parent_cwd = self.working_directory.current_dir().map_err(io_error)?;
                let child_cwd =
                    child_cwd.map_or_else(|| parent_cwd.clone(), |cwd| parent_cwd.join(cwd));
                plan = plan.resolve_from(&child_cwd).map_err(lookup_error)?;
                // Preserve an explicitly empty child cwd as an invalid native
                // argument, rather than turning it into the parent directory.
                if request.cwd.as_ref().is_none_or(|cwd| !cwd.is_empty()) {
                    request.cwd = Some(child_cwd.into_os_string());
                }
            }
            plan
        };
        #[cfg(windows)]
        let (arguments, plan) = {
            use std::os::windows::ffi::OsStrExt;

            let arguments =
                ambient::lookup::parse_command_line(&request.command_line).map_err(io_error)?;
            let program = &arguments[0];
            let searches_path = !program
                .encode_wide()
                .any(|unit| unit == b'\\' as u16 || unit == b'/' as u16);
            let parent_path = searches_path
                .then(|| self.environment.get("PATH".as_ref()))
                .flatten();
            let parent_cwd = match self.working_directory.as_ref() {
                WorkingDirectory::Fixed(cwd) => Some(cwd.as_path()),
                WorkingDirectory::Ambient => None,
            };
            let directories = if searches_path {
                let (application, system) =
                    ambient::lookup::windows_directories().map_err(io_error)?;
                let mut directories: Vec<_> = application.into_iter().collect();
                if self
                    .environment
                    .get("NoDefaultCurrentDirectoryInExePath".as_ref())
                    .is_none()
                {
                    // This explicit relative entry reserves the caller-cwd
                    // search position without observing its pathname yet.
                    directories.push(parent_cwd.unwrap_or(Path::new(".")).to_owned());
                }
                directories.extend(system);
                directories
            } else {
                Vec::new()
            };
            let mut plan =
                lookup::windows(program, parent_path.as_deref(), parent_cwd, directories)
                    .map_err(lookup_error)?;
            if plan.needs_cwd()
                || (self.context.is_some()
                    && request
                        .cwd
                        .as_deref()
                        .is_none_or(|cwd| !Path::new(cwd).is_absolute()))
            {
                let parent_cwd = self.working_directory.current_dir().map_err(io_error)?;
                plan = plan.resolve_from(&parent_cwd).map_err(lookup_error)?;
                if request.cwd.as_ref().is_none_or(|cwd| !cwd.is_empty()) {
                    request.cwd = Some(match request.cwd {
                        Some(cwd) => parent_cwd.join(cwd).into_os_string(),
                        None => parent_cwd.into_os_string(),
                    });
                }
            }
            (arguments, plan)
        };
        request.configure_stdio(&self.stdio)?;
        if let Some(context) = &self.context {
            #[cfg(unix)]
            let spawn = super::moon::Spawn::unix(
                context.clone(),
                request.path,
                plan,
                request.args,
                request.env,
                request.stdio,
                request.cwd,
                request.options,
                self.policy_inheritance.clone(),
            )?;
            #[cfg(windows)]
            let spawn = super::moon::Spawn::windows(
                context.clone(),
                plan,
                arguments,
                request.env,
                request.stdio,
                request.cwd,
                request.options,
                self.policy_inheritance.clone(),
            )?;
            return Ok(PreparedSpawn::Moon(Box::new(spawn)));
        }
        #[cfg(unix)]
        let (executable, invokes_moonx) = (
            Executable::Prepared(plan),
            moonutil::constants::is_moonx_executable(&request.path),
        );
        #[cfg(windows)]
        let (executable, invokes_moonx) = (
            Executable::Prepared(ambient::lookup::select_image(&plan).map_err(io_error)?),
            moonutil::constants::is_moonx_executable(&arguments[0]),
        );
        Ok(PreparedSpawn::Native(Box::new(NativeSpawn {
            request,
            _stdio_owner: Arc::clone(&self.stdio),
            executable,
            policy_inheritance: if invokes_moonx {
                self.policy_inheritance.clone()
            } else {
                None
            },
        })))
    }
}

fn lookup_error(error: lookup::LookupError) -> AsyncHostError {
    match error {
        #[cfg(unix)]
        lookup::LookupError::EmptyProgram => AsyncHostError::Native(libc::ENOENT),
        _ => AsyncHostError::Inval,
    }
}

fn io_error(error: std::io::Error) -> AsyncHostError {
    if error.kind() == std::io::ErrorKind::InvalidInput {
        return AsyncHostError::Inval;
    }
    error
        .raw_os_error()
        .map_or(AsyncHostError::Io, AsyncHostError::Native)
}

impl SpawnRequest {
    pub(super) fn configure_stdio(&mut self, runtime_stdio: &Stdio) -> AsyncHostResult<()> {
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
                *slot = Some(Arc::new(Resource::stdio_file(raw)));
            }
        }
        Ok(())
    }
}
