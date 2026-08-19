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

use std::ffi::{c_int, c_void};
use std::ptr::{self, NonNull};

use libsqlite3_sys as ffi;
use slotmap::Key;

use super::{SqliteHost, SqliteHostError, SqliteHostResult};
use crate::host::{HostResourceKind, null_handle};

// `libsqlite3-sys` intentionally omits SQLite's UTF-16 convenience APIs from
// its generated bindings. The bundled SQLite library still exports them.
unsafe extern "C" {
    fn sqlite3_prepare16_v2(
        database: *mut ffi::sqlite3,
        sql: *const c_void,
        sql_length: c_int,
        statement_out: *mut *mut ffi::sqlite3_stmt,
        tail_out: *mut *const c_void,
    ) -> c_int;
}

#[derive(Clone, Copy)]
pub(super) struct Statement {
    pub(super) pointer: NonNull<ffi::sqlite3_stmt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PrepareOutcome {
    pub(crate) code: i32,
    pub(crate) statement: Option<u64>,
    /// UTF-16 code units consumed from the supplied SQL view.
    pub(crate) tail_offset: u32,
}

impl SqliteHost {
    /// Prepare the first statement in a native-endian UTF-16 view.
    ///
    /// `tail_offset` is measured in UTF-16 code units from the start of `sql`.
    /// The runtime adapter combines it with the view's backing-string offset.
    pub(crate) fn prepare16_v2(
        &self,
        database: u64,
        sql: &[u16],
    ) -> SqliteHostResult<PrepareOutcome> {
        let byte_length = sql
            .len()
            .checked_mul(size_of::<u16>())
            .ok_or(SqliteHostError::Overflow)?;
        let native_length = i32::try_from(byte_length).map_err(|_| SqliteHostError::Overflow)?;
        let database = self.database(database)?;
        if !database.is_ready() {
            return Ok(PrepareOutcome {
                code: ffi::SQLITE_MISUSE,
                statement: None,
                tail_offset: 0,
            });
        }

        let native_start = sql.as_ptr().cast::<c_void>();
        let mut statement = ptr::null_mut();
        let mut native_tail = ptr::null();
        let code = unsafe {
            sqlite3_prepare16_v2(
                database.pointer().as_ptr(),
                native_start,
                native_length,
                &mut statement,
                &mut native_tail,
            )
        };
        // SAFETY: SQLite guarantees that `pzTail` points into the supplied SQL
        // and UTF-16 input keeps both pointers aligned to code units.
        let tail_offset = unsafe { native_tail.cast::<u16>().offset_from(sql.as_ptr()) };
        let tail_offset = u32::try_from(tail_offset).map_err(|_| SqliteHostError::Overflow)?;
        let statement =
            NonNull::new(statement).map(|pointer| self.insert_statement(Statement { pointer }));

        Ok(PrepareOutcome {
            code,
            statement,
            tail_offset,
        })
    }

    pub(crate) fn step(&self, statement: u64) -> SqliteHostResult<i32> {
        let statement = self.statement(statement)?;
        Ok(unsafe { ffi::sqlite3_step(statement.pointer.as_ptr()) })
    }

    pub(crate) fn finalize(&self, statement: u64) -> SqliteHostResult<i32> {
        if statement == null_handle() {
            return Ok(ffi::SQLITE_OK);
        }
        let statement = self.remove_statement(statement)?;
        // `sqlite3_finalize` always destroys the statement, even when it
        // reports an earlier execution error. Remove the guest handle before
        // transferring ownership to SQLite so no stale pointer can remain.
        Ok(unsafe { ffi::sqlite3_finalize(statement.pointer.as_ptr()) })
    }

    fn insert_statement(&self, statement: Statement) -> u64 {
        let key = self
            .keys
            .borrow_mut()
            .insert(HostResourceKind::SqliteStatement);
        let replaced = self.statements.borrow_mut().insert(key, statement);
        debug_assert!(replaced.is_none());
        key.data().as_ffi()
    }

    pub(super) fn statement(&self, handle: u64) -> SqliteHostResult<Statement> {
        let key = self
            .keys
            .borrow()
            .key(handle, HostResourceKind::SqliteStatement)
            .ok_or(SqliteHostError::InvalidHandle)?;
        self.statements
            .borrow()
            .get(key)
            .copied()
            .ok_or(SqliteHostError::InvalidHandle)
    }

    fn remove_statement(&self, handle: u64) -> SqliteHostResult<Statement> {
        let key = self
            .keys
            .borrow()
            .key(handle, HostResourceKind::SqliteStatement)
            .ok_or(SqliteHostError::InvalidHandle)?;
        let statement = self
            .statements
            .borrow_mut()
            .remove(key)
            .ok_or(SqliteHostError::InvalidHandle)?;
        let removed = self.keys.borrow_mut().remove(key);
        debug_assert_eq!(removed, Some(HostResourceKind::SqliteStatement));
        Ok(statement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::tests::{host, open_memory, utf16le};

    #[test]
    fn prepare_reports_the_first_statement_tail_in_code_units() {
        let host = host();
        let database = open_memory(&host);
        let first = "SELECT 1;";
        let sql = utf16le(&format!("{first} SELECT 2"));

        let outcome = host.prepare16_v2(database, &sql).unwrap();

        assert_eq!(outcome.code, ffi::SQLITE_OK);
        assert_eq!(outcome.tail_offset, first.encode_utf16().count() as u32);
        assert_eq!(
            host.finalize(outcome.statement.unwrap()),
            Ok(ffi::SQLITE_OK)
        );
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }

    #[test]
    fn complete_memory_lifecycle_crosses_only_the_host_interface() {
        let host = host();
        let database = open_memory(&host);
        let sql = utf16le("CREATE TABLE 数据(value INTEGER)");
        let outcome = host.prepare16_v2(database, &sql).unwrap();

        assert_eq!(outcome.code, ffi::SQLITE_OK);
        assert_eq!(outcome.tail_offset, sql.len() as u32);
        let statement = outcome.statement.unwrap();
        assert_eq!(host.step(statement), Ok(ffi::SQLITE_DONE));
        assert_eq!(host.finalize(statement), Ok(ffi::SQLITE_OK));
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }
}
