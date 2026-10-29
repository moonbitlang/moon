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

//! Resolve an authorized guest Moon invocation into a supervised child.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::path::{Path, PathBuf};

use super::SpawnOptions;
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::policy::PolicyInheritance;
use crate::process_tree::ProcessContext;
use crate::resource::{ResourcePublication, ResourceRef};
#[cfg(unix)]
use crate::runtime::Stdio;
use crate::runtime::{Env, StdioStream};
use crate::{RunOptions, WorkingDirectory};

#[derive(Debug)]
pub(super) struct Spawn {
    context: ProcessContext,
    executable: PathBuf,
    args: Vec<OsString>,
    options: Option<RunOptions>,
    #[cfg(windows)]
    no_console_window: bool,
}

impl Spawn {
    #[cfg(unix)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn unix(
        context: ProcessContext,
        program: OsString,
        args: Vec<OsString>,
        env: Vec<OsString>,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        options: SpawnOptions,
        policy: Option<PolicyInheritance>,
        parent_env: &Env,
    ) -> AsyncHostResult<Self> {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let env = env
            .into_iter()
            .map(|entry| {
                let bytes = entry.into_vec();
                let split = bytes
                    .iter()
                    .position(|byte| *byte == b'=')
                    .ok_or(AsyncHostError::Inval)?;
                Ok((
                    OsStr::from_bytes(&bytes[..split]).to_owned(),
                    OsStr::from_bytes(&bytes[split + 1..]).to_owned(),
                ))
            })
            .collect::<AsyncHostResult<Vec<_>>>()?;
        // Moon selects moonx by argv[0]. Do not silently change a forged argv[0].
        if args.first().and_then(|arg| Path::new(arg).file_name())
            != Path::new(&program).file_name()
        {
            return Err(AsyncHostError::Inval);
        }
        let mut spawn = Self::prepare(
            context,
            program,
            args.into_iter().skip(1).collect(),
            env,
            stdio,
            cwd,
            policy,
            parent_env,
        )?;
        spawn.options.as_mut().unwrap().preserved_child_signal_mask =
            Some(options.child_signal_mask);
        Ok(spawn)
    }

    #[cfg(windows)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn windows(
        context: ProcessContext,
        command_line: OsString,
        env: Vec<u16>,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        options: SpawnOptions,
        policy: Option<PolicyInheritance>,
        parent_env: &Env,
    ) -> AsyncHostResult<Self> {
        use std::os::windows::ffi::OsStringExt;
        if command_line.is_empty() || command_line.as_encoded_bytes().contains(&0) {
            return Err(AsyncHostError::Inval);
        }
        let args = moonutil::shlex::split_windows_os(&command_line).map_err(io_error)?;
        let program = args.first().ok_or(AsyncHostError::Inval)?.clone();
        let env = env
            .split(|unit| *unit == 0)
            .take_while(|entry| !entry.is_empty())
            .map(|entry| {
                // Windows also carries hidden drive-directory entries (=C:=...).
                let split = entry
                    .iter()
                    .enumerate()
                    .skip(1)
                    .find(|(_, unit)| **unit == b'=' as u16)
                    .map(|(index, _)| index)
                    .ok_or(AsyncHostError::Inval)?;
                Ok((
                    OsString::from_wide(&entry[..split]),
                    OsString::from_wide(&entry[split + 1..]),
                ))
            })
            .collect::<AsyncHostResult<Vec<_>>>()?;
        let mut spawn = Self::prepare(
            context,
            program,
            args.into_iter().skip(1).collect(),
            env,
            stdio,
            cwd,
            policy,
            parent_env,
        )?;
        spawn.no_console_window = options.no_console_window;
        Ok(spawn)
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare(
        context: ProcessContext,
        program: OsString,
        args: Vec<OsString>,
        env: Vec<(OsString, OsString)>,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        policy: Option<PolicyInheritance>,
        parent_env: &Env,
    ) -> AsyncHostResult<Self> {
        if program.as_encoded_bytes().contains(&0)
            || args.iter().any(|arg| arg.as_encoded_bytes().contains(&0))
        {
            return Err(AsyncHostError::Inval);
        }
        // Validate the encoded child environment before admitting a PID. This
        // keeps malformed guest requests as spawn errors rather than child exits.
        let env = Env::owned(env)
            .map_err(|_| AsyncHostError::Inval)?
            .entries();
        let name = Path::new(&program)
            .file_name()
            .ok_or(AsyncHostError::Inval)?;
        let moon = if cfg!(windows) {
            name.eq_ignore_ascii_case("moon") || name.eq_ignore_ascii_case("moon.exe")
        } else {
            name == "moon" || name == "moon.exe"
        };
        if !moon && !moonutil::constants::is_moonx_executable(&program) {
            #[cfg(unix)]
            return Err(AsyncHostError::Native(libc::ENOTSUP));
            #[cfg(windows)]
            return Err(AsyncHostError::Native(
                windows_sys::Win32::Foundation::ERROR_NOT_SUPPORTED as i32,
            ));
        }
        let cwd = PathBuf::from(cwd.ok_or(AsyncHostError::Inval)?);
        if cwd.as_os_str().is_empty() {
            return Err(AsyncHostError::Inval);
        }
        let executable = moonutil::toolchain::resolve_executable_in_paths(
            &program,
            parent_env.get(OsStr::new("PATH")).as_deref(),
            &cwd,
        )
        .map_err(|_| {
            #[cfg(unix)]
            return AsyncHostError::Native(libc::ENOENT);
            #[cfg(windows)]
            return AsyncHostError::Native(
                windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND as i32,
            );
        })?;
        let duplicate =
            |resource: Option<ResourceRef>, stream: StdioStream| -> AsyncHostResult<File> {
                #[cfg(unix)]
                {
                    use std::os::fd::{AsRawFd, BorrowedFd};
                    let raw = match &resource {
                        Some(resource) => resource.as_file()?.as_raw_fd(),
                        None => Stdio::Ambient.raw(stream).map_err(io_error)?,
                    };
                    // Runtime/resource ownership pins the borrowed descriptor until duplication.
                    Ok(File::from(
                        unsafe { BorrowedFd::borrow_raw(raw) }
                            .try_clone_to_owned()
                            .map_err(io_error)?,
                    ))
                }
                #[cfg(windows)]
                {
                    use std::os::windows::io::{AsRawHandle, BorrowedHandle};
                    let raw = match &resource {
                        Some(resource) => resource.as_file()?.as_raw_handle(),
                        None => {
                            return std::fs::OpenOptions::new()
                                .read(true)
                                .write(true)
                                .open("NUL")
                                .map_err(io_error);
                        }
                    };
                    let _ = stream;
                    Ok(File::from(
                        unsafe { BorrowedHandle::borrow_raw(raw) }
                            .try_clone_to_owned()
                            .map_err(io_error)?,
                    ))
                }
            };
        let [stdin, stdout, stderr] = stdio;
        let mut run = RunOptions::default()
            .with_working_directory(WorkingDirectory::Fixed(cwd))
            .with_stdio(
                duplicate(stdin, StdioStream::Stdin)?,
                duplicate(stdout, StdioStream::Stdout)?,
                duplicate(stderr, StdioStream::Stderr)?,
            );
        run.default_environment = Some(env);
        if let Some(policy) = policy {
            run = run.with_inherited_policy(policy.contents());
        }
        Ok(Self {
            context,
            executable,
            args,
            options: Some(run),
            #[cfg(windows)]
            no_console_window: false,
        })
    }

    pub(super) fn run(&mut self, result: &mut Option<ResourcePublication>) -> AsyncHostResult<i64> {
        let options = self.options.take().ok_or(AsyncHostError::Inval)?;
        let spawned = self.context.spawn_moon(
            &self.executable,
            &self.args,
            options,
            #[cfg(windows)]
            self.no_console_window,
        )?;
        let pid = spawned.pid.get();
        #[cfg(windows)]
        {
            *result = Some(ResourcePublication::Unpublished(
                crate::resource::Resource::process(crate::process_tree::ProcessHandle::from_spawn(
                    &self.context,
                    spawned,
                )),
            ));
        }
        #[cfg(unix)]
        let _ = result;
        Ok(i64::from(pid))
    }
}

fn io_error(error: std::io::Error) -> AsyncHostError {
    error
        .raw_os_error()
        .map_or(AsyncHostError::Io, AsyncHostError::Native)
}
