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

//! Native advisory file locks. Callers own resource validation and authorization;
//! blocking acquisition uses their execution bracket without owning Worker state.
//!
//! Windows locks the same one-byte sentinel range as async's native stubs, beyond
//! ordinary file data, so its mandatory locks approximate Unix advisory locking.

use std::io;

use super::{BlockingIo, RawFile};
use crate::upstream::ported_fns;

ported_fns! {
    #[ported(source = "src/fs/stub.c", original = "moonbitlang_async_errno_is_lock_violation")]
    pub(crate) fn errno_is_lock_violation(errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EWOULDBLOCK
        }
        #[cfg(windows)]
        {
            errno == windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION as i32
        }
    }

    #[ported(source = "src/fs/stub.c", original = "moonbitlang_async_try_lock_file")]
    pub(in crate::filesystem) fn try_lock(file: RawFile, exclusive: bool) -> io::Result<()> {
        #[cfg(unix)]
        {
            let operation = libc::LOCK_NB
                | if exclusive {
                    libc::LOCK_EX
                } else {
                    libc::LOCK_SH
                };
            if unsafe { libc::flock(file, operation) } < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::{
                LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
            };
            use windows_sys::Win32::System::IO::OVERLAPPED;

            let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
            overlapped.Anonymous.Anonymous.Offset = 0xfffffffe;
            overlapped.Anonymous.Anonymous.OffsetHigh = 0xffffffff;
            let flags = LOCKFILE_FAIL_IMMEDIATELY
                | if exclusive {
                    LOCKFILE_EXCLUSIVE_LOCK
                } else {
                    0
                };
            if unsafe { LockFileEx(file, flags, 0, 1, 0, &mut overlapped) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    #[ported(source = "src/fs/stub.c", original = "moonbitlang_async_unlock_file")]
    pub(in crate::filesystem) fn unlock(file: RawFile) -> io::Result<()> {
        #[cfg(unix)]
        {
            if unsafe { libc::flock(file, libc::LOCK_UN) } < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::UnlockFile;

            if unsafe { UnlockFile(file, 0xfffffffe, 0xffffffff, 1, 0) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }
}

/// Acquire a lock, allowing the caller to interrupt the blocking syscall.
/// Windows callers must supply a synchronous file handle.
pub(in crate::filesystem) fn lock_blocking(
    file: RawFile,
    exclusive: bool,
    blocking: &impl BlockingIo,
) -> io::Result<()> {
    #[cfg(unix)]
    {
        let operation = if exclusive {
            libc::LOCK_EX
        } else {
            libc::LOCK_SH
        };
        blocking.run(|| {
            if unsafe { libc::flock(file, operation) } < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LockFileEx};
        use windows_sys::Win32::System::IO::OVERLAPPED;

        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.Anonymous.Anonymous.Offset = 0xfffffffe;
        overlapped.Anonymous.Anonymous.OffsetHigh = 0xffffffff;
        let flags = if exclusive {
            LOCKFILE_EXCLUSIVE_LOCK
        } else {
            0
        };
        blocking.run(|| {
            if unsafe { LockFileEx(file, flags, 0, 1, 0, &mut overlapped) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::os::{DirectIo, raw_file};

    #[test]
    fn shared_and_exclusive_locks_contend_across_file_handles() {
        let file = tempfile::NamedTempFile::new().unwrap();
        // Separate opens, not duplicate handles: Unix flock locks share an open
        // file description, so a clone would not exercise contention.
        let other = file.reopen().unwrap();
        let first = raw_file(file.as_file());
        let second = raw_file(&other);

        try_lock(first, false).unwrap();
        lock_blocking(second, false, &DirectIo).unwrap();
        unlock(second).unwrap();
        assert!(errno_is_lock_violation(
            try_lock(second, true).unwrap_err().raw_os_error().unwrap()
        ));
        unlock(first).unwrap();

        lock_blocking(second, true, &DirectIo).unwrap();
        for exclusive in [false, true] {
            assert!(errno_is_lock_violation(
                try_lock(first, exclusive)
                    .unwrap_err()
                    .raw_os_error()
                    .unwrap()
            ));
        }
        // Windows' sentinel range must not prevent ordinary file-data I/O.
        std::fs::write(file.path(), b"unlocked data").unwrap();
        assert_eq!(std::fs::read(file.path()).unwrap(), b"unlocked data");
        unlock(second).unwrap();
        try_lock(first, true).unwrap();
        unlock(first).unwrap();
    }

    #[test]
    fn invalid_file_retains_native_error_codes() {
        #[cfg(unix)]
        let (file, error) = (-1, libc::EBADF);
        #[cfg(windows)]
        let (file, error) = (
            windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE,
            windows_sys::Win32::Foundation::ERROR_INVALID_HANDLE as i32,
        );
        for result in [
            try_lock(file, true),
            lock_blocking(file, true, &DirectIo),
            unlock(file),
        ] {
            assert_eq!(result.unwrap_err().raw_os_error(), Some(error));
        }
        assert!(!errno_is_lock_violation(error));
        assert!(!errno_is_lock_violation(0));
    }
}
