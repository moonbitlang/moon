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

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::async_host::{AsyncHostError, AsyncHostResult};

use super::jobs;
use super::types::{HostFileTable, HostHandle, Job};

#[derive(Debug, Clone)]
pub(crate) struct HostProcess {
    child: Arc<Mutex<Option<Child>>>,
}

impl PartialEq for HostProcess {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.child, &other.child)
    }
}

impl Eq for HostProcess {}

impl HostProcess {
    pub(crate) fn new(child: Child) -> Self {
        Self {
            child: Arc::new(Mutex::new(Some(child))),
        }
    }

    fn wait(&self) -> AsyncHostResult<i32> {
        let mut child = self.child.lock().unwrap();
        let Some(mut child) = child.take() else {
            return Err(AsyncHostError::Badf);
        };
        let status = child.wait().map_err(native_io_error)?;
        Ok(status.code().unwrap_or(1))
    }
}

pub(crate) trait HostProcessTable {
    fn insert_process(&mut self, process: HostProcess) -> AsyncHostResult<HostHandle>;

    fn get_process(&self, handle: HostHandle) -> AsyncHostResult<HostProcess>;
}

pub(crate) fn spawn_process(
    files: &mut impl HostFileTable,
    processes: &mut impl HostProcessTable,
    command: String,
    args: Vec<String>,
    stdin: HostHandle,
    stdout: HostHandle,
    stderr: HostHandle,
) -> AsyncHostResult<HostHandle> {
    let mut process = Command::new(command);
    process.args(args);

    if stdin >= 0 {
        let file = files.with_file_mut(stdin, |file| file.try_clone().map_err(native_io_error))?;
        process.stdin(Stdio::from(file));
    }
    if stdout >= 0 {
        let file = files.with_file_mut(stdout, |file| file.try_clone().map_err(native_io_error))?;
        process.stdout(Stdio::from(file));
    }
    if stderr >= 0 {
        let file = files.with_file_mut(stderr, |file| file.try_clone().map_err(native_io_error))?;
        process.stderr(Stdio::from(file));
    }

    let child = process.spawn().map_err(native_io_error)?;
    processes.insert_process(HostProcess::new(child))
}

pub(crate) fn make_wait_for_process_job_from_handle(
    processes: &impl HostProcessTable,
    process: HostHandle,
) -> AsyncHostResult<Job> {
    Ok(jobs::make_wait_for_process_job(
        processes.get_process(process)?,
    ))
}

#[allow(dead_code)]
pub(super) fn run_wait_for_process_job(process: &HostProcess) -> AsyncHostResult<i64> {
    process.wait().map(i64::from)
}

fn native_io_error(error: std::io::Error) -> AsyncHostError {
    AsyncHostError::Native(
        error
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}
