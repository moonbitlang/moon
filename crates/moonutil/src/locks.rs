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

use crate::{constants::MOON_LOCK, user_log::UserLog};

/// Lock a directory through its stable `.moon-lock` file.
///
/// Reports contention through `user_log` only after one second of waiting.
pub fn lock_directory(
    path: &std::path::Path,
    user_log: &UserLog,
) -> std::io::Result<std::fs::File> {
    acquire(&path.join(MOON_LOCK), user_log)
}

/// Lock a file through an adjacent `<filename>.lock` file.
///
/// Reports contention through `user_log` only after one second of waiting.
pub fn lock_file(path: &std::path::Path, user_log: &UserLog) -> std::io::Result<std::fs::File> {
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    acquire(std::path::Path::new(&lock_path), user_log)
}

/// Acquire an advisory lock and return its stable lock file as the guard.
///
/// Keep the returned file alive for the required lock lifetime. Lock files
/// must remain on disk after unlocking: removing one can let waiters on the old
/// file and newcomers on its replacement hold the same logical lock
/// simultaneously.
fn acquire(path: &std::path::Path, user_log: &UserLog) -> std::io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => {
            let (acquired, wait) = std::sync::mpsc::channel();
            std::thread::scope(|scope| {
                scope.spawn(move || {
                    if matches!(
                        wait.recv_timeout(std::time::Duration::from_secs(1)),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                    ) {
                        user_log.status(format!(
                            "Blocking waiting for file lock {} ...",
                            path.display()
                        ));
                    }
                });
                let result = file.lock().map_err(|error| {
                    std::io::Error::new(error.kind(), "failed to acquire file lock")
                });
                let _ = acquired.send(());
                result
            })?;
            Ok(file)
        }
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}
