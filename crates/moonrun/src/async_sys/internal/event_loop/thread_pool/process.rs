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

use std::process::Child;
use std::sync::{Arc, Mutex};

use crate::async_host::{AsyncHostError, AsyncHostResult};

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
