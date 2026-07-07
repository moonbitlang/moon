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

use fs4::fs_std::FileExt;

use crate::constants::MOON_LOCK;

pub struct FileLock {
    _file: std::fs::File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        fs4::fs_std::FileExt::unlock(&self._file).unwrap();
    }
}

impl FileLock {
    pub fn lock(path: &std::path::Path) -> std::io::Result<Self> {
        Self::lock_with_verbosity(path, true)
    }

    pub fn lock_with_verbosity(path: &std::path::Path, verbose: bool) -> std::io::Result<Self> {
        let file = std::fs::File::create(path.join(MOON_LOCK))?;
        match file.try_lock_exclusive() {
            Ok(_) => Ok(FileLock { _file: file }),
            Err(_) => {
                if verbose {
                    #[cfg(not(test))]
                    eprintln!(
                        "Blocking waiting for file lock {} ...",
                        path.join(MOON_LOCK).display()
                    );
                }
                file.lock_exclusive()
                    .map_err(|e| std::io::Error::new(e.kind(), "failed to lock target dir"))?;
                Ok(FileLock { _file: file })
            }
        }
    }
}
