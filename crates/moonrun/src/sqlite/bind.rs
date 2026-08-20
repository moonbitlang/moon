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

use libsqlite3_sys as ffi;

use super::{SqliteHost, SqliteHostError, SqliteHostResult};

// `libsqlite3-sys` intentionally omits SQLite's UTF-16 convenience APIs from
// its generated bindings. The bundled SQLite library still exports them.
unsafe extern "C" {
    fn sqlite3_bind_text16(
        statement: *mut ffi::sqlite3_stmt,
        index: c_int,
        value: *const c_void,
        byte_length: c_int,
        destructor: ffi::sqlite3_destructor_type,
    ) -> c_int;
}

impl SqliteHost {
    pub(crate) fn bind_null(&self, statement: u64, index: i32) -> SqliteHostResult<i32> {
        let statement = self.statement(statement)?;
        Ok(unsafe { ffi::sqlite3_bind_null(statement.pointer.as_ptr(), index) })
    }

    pub(crate) fn bind_int64(
        &self,
        statement: u64,
        index: i32,
        value: i64,
    ) -> SqliteHostResult<i32> {
        let statement = self.statement(statement)?;
        Ok(unsafe { ffi::sqlite3_bind_int64(statement.pointer.as_ptr(), index, value) })
    }

    pub(crate) fn bind_double(
        &self,
        statement: u64,
        index: i32,
        value: f64,
    ) -> SqliteHostResult<i32> {
        let statement = self.statement(statement)?;
        Ok(unsafe { ffi::sqlite3_bind_double(statement.pointer.as_ptr(), index, value) })
    }

    /// Bind native-endian UTF-16 and make SQLite copy it before returning.
    pub(crate) fn bind_text16(
        &self,
        statement: u64,
        index: i32,
        value: &[u16],
    ) -> SqliteHostResult<i32> {
        let byte_length = value
            .len()
            .checked_mul(size_of::<u16>())
            .ok_or(SqliteHostError::Overflow)?;
        let byte_length = i32::try_from(byte_length).map_err(|_| SqliteHostError::Overflow)?;
        let statement = self.statement(statement)?;
        Ok(unsafe {
            sqlite3_bind_text16(
                statement.pointer.as_ptr(),
                index,
                value.as_ptr().cast(),
                byte_length,
                ffi::SQLITE_TRANSIENT(),
            )
        })
    }

    /// Bind bytes and make SQLite copy them before returning.
    pub(crate) fn bind_blob(
        &self,
        statement: u64,
        index: i32,
        value: &[u8],
    ) -> SqliteHostResult<i32> {
        let length = i32::try_from(value.len()).map_err(|_| SqliteHostError::Overflow)?;
        let statement = self.statement(statement)?;
        Ok(unsafe {
            ffi::sqlite3_bind_blob(
                statement.pointer.as_ptr(),
                index,
                value.as_ptr().cast(),
                length,
                ffi::SQLITE_TRANSIENT(),
            )
        })
    }

    pub(crate) fn clear_bindings(&self, statement: u64) -> SqliteHostResult<i32> {
        let statement = self.statement(statement)?;
        Ok(unsafe { ffi::sqlite3_clear_bindings(statement.pointer.as_ptr()) })
    }
}
