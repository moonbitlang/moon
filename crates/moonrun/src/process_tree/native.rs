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

//! Native admission and completion in the Engine namespace.

use super::*;
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::process::{NativeChild, NativeSpawn};

impl Engine {
    pub(super) fn spawn_native(
        &self,
        parent: VirtualPid,
        spawn: NativeSpawn,
    ) -> AsyncHostResult<SpawnedProcess> {
        let mut table = self.processes.0.lock().unwrap();
        if table.shutting_down
            || table
                .entries
                .get(&parent)
                .is_none_or(|entry| entry.info().state != ProcessState::Running)
        {
            return Err(AsyncHostError::PermissionDenied);
        }
        let pid = VirtualPid::allocate(&NEXT_PID).map_err(|_| AsyncHostError::Io)?;
        let completion = Arc::new(ProcessCompletion::default());
        let reaper_completion = Arc::clone(&completion);
        let shared_table = Arc::clone(&self.processes.0);
        let (sender, receiver) = mpsc::channel::<Arc<NativeChild>>();
        // Reserve the sole OS waiter before creating a child. Dropping sender
        // cancels this reservation when spawn fails, preserving its original errno.
        thread::Builder::new()
            .name(format!("moonrun-native-{pid}"))
            .spawn(move || {
                let Ok(child) = receiver.recv() else {
                    return;
                };
                let exit = ProcessExitGuard {
                    table: shared_table,
                    pid,
                    reap_on_drop: false,
                };
                let result = child
                    .wait()
                    .map(|status| {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            if let Some(signal) = status.signal() {
                                return RunOutcome::KilledBySignal(signal);
                            }
                        }
                        RunOutcome::Exited(status.code().expect("terminated native child"))
                    })
                    .map_err(|error| ProcessError::RunFailed(error.into()));
                drop(child);
                drop(exit);
                reaper_completion.publish(result);
            })
            .map_err(io_error)?;
        // Deliberately serialize OS spawn and publication with shutdown. This
        // may delay shutdown while the OS starts an image, but never leaves a
        // successful child outside the table or without its reserved reaper.
        let child = Arc::new(spawn.spawn_owned()?);
        table.entries.insert(
            pid,
            Entry {
                info: ProcessInfo {
                    pid,
                    parent: Some(parent),
                    state: ProcessState::Running,
                },
                control: ProcessControl::Native(Arc::clone(&child)),
                completion: Arc::clone(&completion),
                waiting: false,
            },
        );
        assert!(
            sender.send(child).is_ok(),
            "reserved native reaper disconnected"
        );
        Ok(SpawnedProcess {
            pid,
            #[cfg(windows)]
            completion,
        })
    }
}

pub(super) fn io_error(error: std::io::Error) -> AsyncHostError {
    error
        .raw_os_error()
        .map_or(AsyncHostError::Io, AsyncHostError::Native)
}

#[cfg(all(test, feature = "v8"))]
mod tests;
