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

//! Caller-owned cancellation and native process launch configuration.

#[cfg(unix)]
use std::collections::BTreeSet;
use std::{
    ffi::OsString,
    fmt,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, Default)]
pub struct RunControl(Arc<Mutex<ControlState>>);
#[derive(Debug, Default)]
struct ControlState {
    cancelled: bool,
    #[cfg(unix)]
    children: BTreeSet<i32>,
}
impl RunControl {
    pub fn cancel(&self) {
        let mut state = self.0.lock().unwrap();
        state.cancelled = true;
        #[cfg(unix)]
        for pid in &state.children {
            unsafe {
                libc::kill(-*pid, libc::SIGKILL);
            }
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.lock().unwrap().cancelled
    }
    #[cfg(unix)]
    pub(crate) fn track_child(&self, pid: i32) {
        let mut state = self.0.lock().unwrap();
        state.children.insert(pid);
        if state.cancelled {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    #[cfg(unix)]
    pub(crate) fn finish_child(&self, pid: i32) {
        let mut state = self.0.lock().unwrap();
        if state.children.remove(&pid) {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    pub(crate) fn stop_children(&self) {
        #[cfg(unix)]
        let state = self.0.lock().unwrap();
        #[cfg(unix)]
        for pid in &state.children {
            unsafe {
                libc::kill(-*pid, libc::SIGKILL);
            }
        }
    }
}

pub struct NativeCommand {
    pub program: OsString,
    /// Includes argv[0].
    pub args: Vec<OsString>,
    /// Native NAME=value entries.
    pub env: Vec<OsString>,
    pub cwd: Option<OsString>,
    /// Files kept open in both parent and native child until the spawn Job retires.
    pub inherited_files: Vec<Arc<std::fs::File>>,
}
type LaunchFn = dyn Fn(&mut NativeCommand) -> std::io::Result<()> + Send + Sync;
#[derive(Clone)]
pub struct ChildLauncher(pub Arc<LaunchFn>);
impl fmt::Debug for ChildLauncher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChildLauncher")
    }
}
