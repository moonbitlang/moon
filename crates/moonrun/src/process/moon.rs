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

//! Resolve an authorized guest moonx invocation into a supervised child.

use std::ffi::OsString;
use std::fs::File;
use std::path::PathBuf;

use super::intercept::MoonInvocation;
use super::lookup::LookupPlan;
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::policy::Policy;
use crate::process_tree::ExecutionContext;
use crate::resource::{ResourcePublication, ResourceRef};
#[cfg(unix)]
use crate::runtime::Stdio;
use crate::runtime::StdioStream;
use crate::{RunOptions, WorkingDirectory};

#[derive(Debug)]
pub(super) struct Spawn {
    context: ExecutionContext,
    executable: PathBuf,
    args: Vec<OsString>,
    options: Option<RunOptions>,
    #[cfg(windows)]
    no_console_window: bool,
}

impl Spawn {
    pub(super) fn prepare(
        context: ExecutionContext,
        plan: LookupPlan,
        invocation: MoonInvocation,
        policy: Option<Policy>,
    ) -> AsyncHostResult<Self> {
        let MoonInvocation {
            args,
            environment,
            cwd,
            stdio,
            options,
        } = invocation;
        #[cfg(unix)]
        let executable = super::os::lookup::resolve_for_moon(&plan).map_err(io_error)?;
        #[cfg(windows)]
        let executable = super::os::lookup::select_image(&plan).map_err(io_error)?;
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
            .with_virtualize(true)
            .with_environment(environment)
            .with_working_directory(WorkingDirectory::Fixed(cwd))
            .with_stdio(
                duplicate(stdin, StdioStream::Stdin)?,
                duplicate(stdout, StdioStream::Stdout)?,
                duplicate(stderr, StdioStream::Stderr)?,
            );
        #[cfg(unix)]
        {
            run.runtime.preserved_child_signal_mask = Some(options.child_signal_mask);
        }
        if let Some(policy) = policy {
            run = run.with_parent_policy(policy);
        }
        Ok(Self {
            context,
            executable,
            args,
            options: Some(run),
            #[cfg(windows)]
            no_console_window: options.no_console_window,
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
