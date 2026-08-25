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

use std::ffi::{CStr, c_void};
use std::ptr::{self, NonNull};

use libsqlite3_sys as ffi;
use slotmap::Key;

use super::policy::{ensure_open_flags, ensure_valid_database, install_authorizer};
use super::{SqliteHost, SqliteHostError, SqliteHostResult};
use crate::runtime::{HostResourceKind, null_handle};

// `libsqlite3-sys` intentionally omits SQLite's UTF-16 convenience APIs from
// its generated bindings. The bundled SQLite library still exports them.
unsafe extern "C" {
    fn sqlite3_errmsg16(database: *mut ffi::sqlite3) -> *const c_void;
}

#[derive(Clone, Copy)]
pub(super) enum Database {
    /// The connection opened and the Host policy was installed.
    Ready(NonNull<ffi::sqlite3>),
    /// SQLite returned a connection together with an error. It remains valid
    /// only for reading the error and closing the connection.
    Failed(NonNull<ffi::sqlite3>),
}

impl Database {
    pub(super) fn pointer(self) -> NonNull<ffi::sqlite3> {
        match self {
            Self::Ready(pointer) | Self::Failed(pointer) => pointer,
        }
    }

    pub(super) fn is_ready(self) -> bool {
        matches!(self, Self::Ready(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpenOutcome {
    pub(crate) code: i32,
    /// Like `sqlite3_open_v2`, most failures still return a connection so the
    /// caller can retrieve its error message and close it.
    pub(crate) database: Option<u64>,
}

impl SqliteHost {
    pub(crate) fn open_v2(&self, filename: &CStr, flags: i32, vfs: u64) -> OpenOutcome {
        let flags = ensure_open_flags(flags);
        if let Err(code) = ensure_valid_database(&self.policy, filename, flags, vfs) {
            return OpenOutcome {
                code,
                database: None,
            };
        }

        let mut database = ptr::null_mut();
        let code =
            unsafe { ffi::sqlite3_open_v2(filename.as_ptr(), &mut database, flags, ptr::null()) };
        let Some(database) = NonNull::new(database) else {
            return OpenOutcome {
                code,
                database: None,
            };
        };

        let code = if code == ffi::SQLITE_OK {
            install_authorizer(database)
        } else {
            code
        };

        let database = if code == ffi::SQLITE_OK {
            Database::Ready(database)
        } else {
            Database::Failed(database)
        };
        OpenOutcome {
            code,
            database: Some(self.insert_database(database)),
        }
    }

    /// Return the current connection error length in UTF-16 code units,
    /// excluding its trailing NUL.
    pub(crate) fn errmsg16_length(&self, database: u64) -> SqliteHostResult<u32> {
        let database = self.database(database)?;
        let message = unsafe { sqlite3_errmsg16(database.pointer().as_ptr()) };
        // No other thread can access this run-local connection, and the scan
        // finishes before another SQLite call can invalidate the pointer.
        Ok(unsafe { utf16_string_length(message) }?.unwrap_or(0))
    }

    pub(crate) fn errcode(&self, database: u64) -> SqliteHostResult<i32> {
        let database = self.database(database)?;
        Ok(unsafe { ffi::sqlite3_errcode(database.pointer().as_ptr()) })
    }

    pub(crate) fn extended_errcode(&self, database: u64) -> SqliteHostResult<i32> {
        let database = self.database(database)?;
        Ok(unsafe { ffi::sqlite3_extended_errcode(database.pointer().as_ptr()) })
    }

    pub(crate) fn changes64(&self, database: u64) -> SqliteHostResult<i64> {
        let database = self.database(database)?;
        if !database.is_ready() {
            return Err(SqliteHostError::InvalidInput);
        }
        Ok(unsafe { ffi::sqlite3_changes64(database.pointer().as_ptr()) })
    }

    /// Copy SQLite's current connection error while its pointer is valid.
    ///
    /// The returned length and `output` are measured in UTF-16 content code
    /// units; SQLite's trailing NUL is not copied. If `output` cannot hold the
    /// complete message, it is left unchanged and the returned length tells
    /// the caller how much space to allocate.
    pub(crate) fn copy_errmsg16(&self, database: u64, output: &mut [u16]) -> SqliteHostResult<u32> {
        let database = self.database(database)?;
        let message = unsafe { sqlite3_errmsg16(database.pointer().as_ptr()) };
        // No other thread can access this run-local connection, and copying
        // finishes before another SQLite call can invalidate the pointer.
        Ok(unsafe { copy_utf16_string(message, output) }?.unwrap_or(0))
    }

    pub(crate) fn close(&self, database: u64) -> SqliteHostResult<i32> {
        if database == null_handle() {
            return Ok(ffi::SQLITE_OK);
        }
        let database_handle = database;
        let database = self.database(database_handle)?;
        let pointer = database.pointer();
        let code = unsafe { ffi::sqlite3_close(pointer.as_ptr()) };
        if code == ffi::SQLITE_OK {
            let removed = self.remove_database(database_handle)?;
            debug_assert_eq!(removed.pointer(), pointer);
        }
        Ok(code)
    }

    fn insert_database(&self, database: Database) -> u64 {
        let key = self
            .keys
            .borrow_mut()
            .insert(HostResourceKind::SqliteDatabase);
        let replaced = self.databases.borrow_mut().insert(key, database);
        debug_assert!(replaced.is_none());
        key.data().as_ffi()
    }

    pub(super) fn database(&self, handle: u64) -> SqliteHostResult<Database> {
        let key = self
            .keys
            .borrow()
            .key(handle, HostResourceKind::SqliteDatabase)
            .ok_or(SqliteHostError::InvalidHandle)?;
        self.databases
            .borrow()
            .get(key)
            .copied()
            .ok_or(SqliteHostError::InvalidHandle)
    }

    fn remove_database(&self, handle: u64) -> SqliteHostResult<Database> {
        let key = self
            .keys
            .borrow()
            .key(handle, HostResourceKind::SqliteDatabase)
            .ok_or(SqliteHostError::InvalidHandle)?;
        let database = self
            .databases
            .borrow_mut()
            .remove(key)
            .ok_or(SqliteHostError::InvalidHandle)?;
        let removed = self.keys.borrow_mut().remove(key);
        debug_assert_eq!(removed, Some(HostResourceKind::SqliteDatabase));
        Ok(database)
    }
}

/// Measure a native-endian, NUL-terminated SQLite UTF-16 string, excluding NUL.
/// Return `None` when SQLite supplied a NULL pointer.
///
/// # Safety
///
/// A non-NULL `value` must point to a valid SQLite-owned UTF-16 string whose
/// lifetime covers this call.
pub(super) unsafe fn utf16_string_length(value: *const c_void) -> SqliteHostResult<Option<u32>> {
    if value.is_null() {
        return Ok(None);
    }

    let value = value.cast::<u16>();
    let mut length = 0_usize;
    // SAFETY: SQLite guarantees a NUL-terminated UTF-16 string for every
    // non-NULL result, and the caller guarantees its lifetime for this scan.
    while unsafe { *value.add(length) } != 0 {
        length += 1;
    }
    u32::try_from(length)
        .map(Some)
        .map_err(|_| SqliteHostError::Overflow)
}

/// Copy a native-endian, NUL-terminated SQLite UTF-16 string.
/// Return `None` when SQLite supplied a NULL pointer.
///
/// # Safety
///
/// A non-NULL `value` must point to a valid SQLite-owned UTF-16 string whose
/// lifetime covers this call.
pub(super) unsafe fn copy_utf16_string(
    value: *const c_void,
    output: &mut [u16],
) -> SqliteHostResult<Option<u32>> {
    if value.is_null() {
        return Ok(None);
    }

    // SAFETY: upheld by this function's caller.
    let Some(length) = unsafe { utf16_string_length(value) }? else {
        return Ok(None);
    };
    let content_length = length as usize;
    if output.len() >= content_length && content_length != 0 {
        // SAFETY: the scan above proved that the slice contains at least
        // `content_length` code units before its NUL terminator.
        let value = unsafe { std::slice::from_raw_parts(value.cast::<u16>(), content_length) };
        output[..content_length].copy_from_slice(value);
    }
    Ok(Some(length))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::tests::{host, open_memory, utf16le};

    #[test]
    fn error_messages_report_and_copy_content_length() {
        let host = host();
        let database = open_memory(&host);
        let sql = utf16le("SELECT 不存在");
        let outcome = host.prepare16_v2(database, &sql).unwrap();
        assert_eq!(outcome.code, ffi::SQLITE_ERROR);
        assert_eq!(host.errcode(database), Ok(ffi::SQLITE_ERROR));
        assert_eq!(host.extended_errcode(database), Ok(ffi::SQLITE_ERROR));

        let length = host.errmsg16_length(database).unwrap();
        let mut complete = vec![0xffff; length as usize];
        assert_eq!(host.copy_errmsg16(database, &mut complete), Ok(length));
        let message = String::from_utf16(&complete).unwrap();
        assert!(message.starts_with("no such column"));
        assert!(message.ends_with("不存在"));

        let mut short = [0xffff; 5];
        assert_eq!(host.copy_errmsg16(database, &mut short), Ok(length));
        assert_eq!(short, [0xffff; 5]);
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }

    #[test]
    fn utf16_helpers_distinguish_unavailable_and_empty_strings() {
        let empty = [0_u16];
        let mut output = [0xffff];

        assert_eq!(unsafe { utf16_string_length(ptr::null()) }, Ok(None));
        assert_eq!(
            unsafe { utf16_string_length(empty.as_ptr().cast()) },
            Ok(Some(0))
        );
        assert_eq!(
            unsafe { copy_utf16_string(ptr::null(), &mut output) },
            Ok(None)
        );
        assert_eq!(
            unsafe { copy_utf16_string(empty.as_ptr().cast(), &mut output) },
            Ok(Some(0))
        );
        assert_eq!(output, [0xffff]);
    }

    #[test]
    fn changes64_rejects_a_failed_connection() {
        let host = host();
        let outcome = host.open_v2(c":memory:", 0, null_handle());
        assert_eq!(outcome.code, ffi::SQLITE_MISUSE);
        let database = outcome.database.unwrap();

        assert_eq!(host.changes64(database), Err(SqliteHostError::InvalidInput));
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }
}
