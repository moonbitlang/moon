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

pub struct FileLock {
    _file: std::fs::File,
}

impl FileLock {
    pub fn lock(path: &std::path::Path) -> std::io::Result<Self> {
        Self::lock_with_verbosity(path, true)
    }

    pub fn lock_with_verbosity(path: &std::path::Path, verbose: bool) -> std::io::Result<Self> {
        let user_log = UserLog::new(if verbose {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Error
        });
        Self::lock_with_user_log(path, &user_log)
    }

    pub fn lock_with_user_log(path: &std::path::Path, user_log: &UserLog) -> std::io::Result<Self> {
        Self::lock_file_with_user_log(&path.join(MOON_LOCK), user_log)
    }

    /// Lock a stable file path.
    ///
    /// Callers must not remove the lock file after unlocking it. A waiter may
    /// still hold the old file open while a new caller creates and locks a
    /// different file at the same path, splitting one lock domain into two.
    pub fn lock_file_with_user_log(
        path: &std::path::Path,
        user_log: &UserLog,
    ) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(_) => Ok(FileLock { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => {
                #[cfg(test)]
                let _ = user_log;
                #[cfg(not(test))]
                user_log.status(format!(
                    "Blocking waiting for file lock {} ...",
                    path.display()
                ));
                file.lock().map_err(|error| {
                    std::io::Error::new(error.kind(), "failed to acquire file lock")
                })?;
                Ok(FileLock { _file: file })
            }
            Err(std::fs::TryLockError::Error(error)) => Err(error),
        }
    }
}
