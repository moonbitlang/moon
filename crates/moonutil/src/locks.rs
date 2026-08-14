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
