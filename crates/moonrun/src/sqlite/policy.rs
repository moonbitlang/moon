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

use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr::{self, NonNull};

use libsqlite3_sys as ffi;

use crate::host::null_handle;

/// Ensure the requested database and VFS are available in the MVP.
pub(super) fn ensure_valid_database(filename: &CStr, vfs: u64) -> Result<(), i32> {
    if filename.to_bytes() != b":memory:" || vfs != null_handle() {
        return Err(ffi::SQLITE_CANTOPEN);
    }
    Ok(())
}

/// Preserve SQLite's access-mode bits and remove every extension flag.
pub(super) fn ensure_open_flags(flags: i32) -> i32 {
    flags & (ffi::SQLITE_OPEN_READONLY | ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE)
}

/// Install the complete MVP SQL policy before guest SQL can be prepared.
///
/// The authorizer rejects database attachment and every guest PRAGMA except
/// the encoding selection a caller may perform before schema initialization.
pub(super) fn install_authorizer(database: NonNull<ffi::sqlite3>) -> i32 {
    unsafe {
        ffi::sqlite3_set_authorizer(
            database.as_ptr(),
            Some(untrusted_authorizer),
            ptr::null_mut(),
        )
    }
}

unsafe extern "C" fn untrusted_authorizer(
    _context: *mut c_void,
    action: c_int,
    argument1: *const c_char,
    _argument2: *const c_char,
    _database: *const c_char,
    _trigger: *const c_char,
) -> c_int {
    // SAFETY: SQLite owns the callback arguments and keeps their
    // NUL-terminated strings valid for the duration of this call.
    match action {
        ffi::SQLITE_PRAGMA
            if !argument1.is_null()
                && unsafe { CStr::from_ptr(argument1) }
                    .to_bytes()
                    .eq_ignore_ascii_case(b"encoding") =>
        {
            ffi::SQLITE_OK
        }
        ffi::SQLITE_ATTACH | ffi::SQLITE_DETACH | ffi::SQLITE_PRAGMA => ffi::SQLITE_DENY,
        _ => ffi::SQLITE_OK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_flags_preserve_only_access_modes() {
        for flags in [
            ffi::SQLITE_OPEN_READONLY,
            ffi::SQLITE_OPEN_READWRITE,
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE,
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE | 0x4000_0000,
        ] {
            assert_eq!(
                ensure_open_flags(flags),
                flags
                    & (ffi::SQLITE_OPEN_READONLY
                        | ffi::SQLITE_OPEN_READWRITE
                        | ffi::SQLITE_OPEN_CREATE)
            );
        }
    }

    #[test]
    fn database_policy_accepts_only_private_memory_and_the_default_vfs() {
        assert_eq!(ensure_valid_database(c":memory:", null_handle()), Ok(()));
        assert_eq!(
            ensure_valid_database(c"database.sqlite", null_handle()),
            Err(ffi::SQLITE_CANTOPEN)
        );
        assert_eq!(
            ensure_valid_database(c":memory:", u64::MAX),
            Err(ffi::SQLITE_CANTOPEN)
        );
    }
}
