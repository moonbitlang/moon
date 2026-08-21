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

use super::connection::{copy_utf16_string, utf16_string_length};
use super::statement::Statement;
use super::{SqliteHost, SqliteHostError, SqliteHostResult};

// `libsqlite3-sys` intentionally omits SQLite's UTF-16 convenience APIs from
// its generated bindings. The bundled SQLite library still exports them.
unsafe extern "C" {
    fn sqlite3_column_name16(statement: *mut ffi::sqlite3_stmt, column: c_int) -> *const c_void;
    fn sqlite3_column_text16(statement: *mut ffi::sqlite3_stmt, column: c_int) -> *const c_void;
    fn sqlite3_column_bytes16(statement: *mut ffi::sqlite3_stmt, column: c_int) -> c_int;
}

impl SqliteHost {
    pub(crate) fn column_count(&self, statement: u64) -> SqliteHostResult<i32> {
        let statement = self.statement(statement)?;
        Ok(unsafe { ffi::sqlite3_column_count(statement.pointer.as_ptr()) })
    }

    /// Return the UTF-16 column-name length, or `-1` when SQLite could not
    /// allocate the converted name.
    pub(crate) fn column_name16_length(
        &self,
        statement: u64,
        column: i32,
    ) -> SqliteHostResult<i32> {
        let statement = self.result_column(statement, column)?;
        let name = unsafe { sqlite3_column_name16(statement.pointer.as_ptr(), column) };
        // The scan completes before another SQLite call can invalidate the
        // statement-owned pointer.
        column_name_length(unsafe { utf16_string_length(name) }?)
    }

    /// Copy a UTF-16 column name when it fits.
    ///
    /// The returned content length excludes SQLite's trailing NUL. A short
    /// output leaves the output unchanged. `-1` reports that SQLite could not
    /// allocate the UTF-16 column name.
    pub(crate) fn copy_column_name16(
        &self,
        statement: u64,
        column: i32,
        output: &mut [u16],
    ) -> SqliteHostResult<i32> {
        let statement = self.result_column(statement, column)?;
        let name = unsafe { sqlite3_column_name16(statement.pointer.as_ptr(), column) };
        // The copy completes before another SQLite call can invalidate the
        // statement-owned pointer.
        column_name_length(unsafe { copy_utf16_string(name, output) }?)
    }

    pub(crate) fn column_type(&self, statement: u64, column: i32) -> SqliteHostResult<i32> {
        let statement = self.current_column(statement, column)?;
        Ok(unsafe { ffi::sqlite3_column_type(statement.pointer.as_ptr(), column) })
    }

    pub(crate) fn column_double(&self, statement: u64, column: i32) -> SqliteHostResult<f64> {
        let statement = self.current_column(statement, column)?;
        Ok(unsafe { ffi::sqlite3_column_double(statement.pointer.as_ptr(), column) })
    }

    pub(crate) fn column_int64(&self, statement: u64, column: i32) -> SqliteHostResult<i64> {
        let statement = self.current_column(statement, column)?;
        Ok(unsafe { ffi::sqlite3_column_int64(statement.pointer.as_ptr(), column) })
    }

    pub(crate) fn column_text16_length(
        &self,
        statement: u64,
        column: i32,
    ) -> SqliteHostResult<u32> {
        let Some((_, length)) = self.text16_column(statement, column)? else {
            return Ok(0);
        };
        u32::try_from(length).map_err(|_| SqliteHostError::Overflow)
    }

    /// Copy a UTF-16 column when it fits.
    ///
    /// The returned content length excludes SQLite's trailing NUL. A short
    /// output and a NULL or failed conversion leave the output unchanged;
    /// `sqlite3_errcode` lets the caller distinguish SQL NULL from a conversion
    /// allocation failure.
    pub(crate) fn copy_column_text16(
        &self,
        statement: u64,
        column: i32,
        output: &mut [u16],
    ) -> SqliteHostResult<u32> {
        let Some((pointer, length)) = self.text16_column(statement, column)? else {
            return Ok(0);
        };
        if output.len() >= length && length != 0 {
            // SAFETY: SQLite reported `length` content units for this pointer,
            // and no SQLite call can invalidate it before this synchronous
            // copy completes.
            let value = unsafe { std::slice::from_raw_parts(pointer, length) };
            output[..length].copy_from_slice(value);
        }
        u32::try_from(length).map_err(|_| SqliteHostError::Overflow)
    }

    pub(crate) fn column_blob_length(&self, statement: u64, column: i32) -> SqliteHostResult<u32> {
        self.blob_column(statement, column)
            .and_then(|(_, length)| u32::try_from(length).map_err(|_| SqliteHostError::Overflow))
    }

    /// Copy a blob when it fits and return its full byte length.
    pub(crate) fn copy_column_blob(
        &self,
        statement: u64,
        column: i32,
        output: &mut [u8],
    ) -> SqliteHostResult<u32> {
        let (pointer, length) = self.blob_column(statement, column)?;
        if output.len() >= length && length != 0 {
            let pointer = pointer.ok_or(SqliteHostError::InvalidInput)?;
            // SAFETY: SQLite reported `length` bytes for this pointer, and no
            // SQLite call can invalidate it before the copy completes.
            let value = unsafe { std::slice::from_raw_parts(pointer, length) };
            output[..length].copy_from_slice(value);
        }
        u32::try_from(length).map_err(|_| SqliteHostError::Overflow)
    }

    /// Validate metadata access, which is available without a current row.
    fn result_column(&self, statement: u64, column: i32) -> SqliteHostResult<Statement> {
        let statement = self.statement(statement)?;
        let columns = unsafe { ffi::sqlite3_column_count(statement.pointer.as_ptr()) };
        if !(0..columns).contains(&column) {
            return Err(SqliteHostError::InvalidInput);
        }
        Ok(statement)
    }

    fn current_column(&self, statement: u64, column: i32) -> SqliteHostResult<Statement> {
        let statement = self.statement(statement)?;
        let columns = unsafe { ffi::sqlite3_data_count(statement.pointer.as_ptr()) };
        if !(0..columns).contains(&column) {
            return Err(SqliteHostError::InvalidInput);
        }
        Ok(statement)
    }

    fn text16_column(
        &self,
        statement: u64,
        column: i32,
    ) -> SqliteHostResult<Option<(*const u16, usize)>> {
        let statement = self.current_column(statement, column)?;
        let pointer = unsafe { sqlite3_column_text16(statement.pointer.as_ptr(), column) };
        if pointer.is_null() {
            // Do not make another SQLite call before the guest can inspect
            // sqlite3_errcode() to distinguish SQL NULL from conversion OOM.
            return Ok(None);
        }
        // SQLite requires this order so conversion cannot invalidate the
        // pointer before its size is known.
        let byte_length = unsafe { sqlite3_column_bytes16(statement.pointer.as_ptr(), column) };
        if byte_length < 0 || byte_length % 2 != 0 {
            return Err(SqliteHostError::InvalidInput);
        }
        let length = byte_length as usize / size_of::<u16>();
        Ok(Some((pointer.cast(), length)))
    }

    fn blob_column(
        &self,
        statement: u64,
        column: i32,
    ) -> SqliteHostResult<(Option<*const u8>, usize)> {
        let statement = self.current_column(statement, column)?;
        let pointer = unsafe { ffi::sqlite3_column_blob(statement.pointer.as_ptr(), column) };
        if pointer.is_null() {
            // SQLite also uses NULL for SQL NULL, zero-length blobs, and OOM.
            // Preserve the connection error until the guest inspects it.
            return Ok((None, 0));
        }
        // SQLite requires the value accessor before the matching byte count.
        let length = unsafe { ffi::sqlite3_column_bytes(statement.pointer.as_ptr(), column) };
        if length < 0 {
            return Err(SqliteHostError::InvalidInput);
        }
        Ok((Some(pointer.cast()), length as usize))
    }
}

// Column indexes are validated before SQLite is called, so an unavailable
// column name is specifically the allocation failure documented by SQLite.
fn column_name_length(length: Option<u32>) -> SqliteHostResult<i32> {
    let Some(length) = length else {
        return Ok(-1);
    };
    i32::try_from(length).map_err(|_| SqliteHostError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::tests::{host, open_memory, utf16le};

    #[test]
    fn bindings_round_trip_through_typed_columns_and_copied_buffers() {
        let host = host();
        let database = open_memory(&host);
        let sql = utf16le("SELECT ?1, ?2, ?3, ?4, ?5");
        let statement = host
            .prepare16_v2(database, &sql)
            .unwrap()
            .statement
            .unwrap();
        let text = utf16le("值😀");
        let blob = [0, 0x7f, 0xff];

        assert_eq!(
            host.bind_int64(statement, 1, 5_000_000_000),
            Ok(ffi::SQLITE_OK)
        );
        assert_eq!(host.bind_double(statement, 2, 3.5), Ok(ffi::SQLITE_OK));
        assert_eq!(host.bind_text16(statement, 3, &text), Ok(ffi::SQLITE_OK));
        assert_eq!(host.bind_blob(statement, 4, &blob), Ok(ffi::SQLITE_OK));
        assert_eq!(host.bind_null(statement, 5), Ok(ffi::SQLITE_OK));
        assert_eq!(host.bind_null(statement, 6), Ok(ffi::SQLITE_RANGE));
        assert_eq!(host.step(statement), Ok(ffi::SQLITE_ROW));

        assert_eq!(host.column_count(statement), Ok(5));
        assert_eq!(host.column_type(statement, 0), Ok(ffi::SQLITE_INTEGER));
        assert_eq!(host.column_type(statement, 1), Ok(ffi::SQLITE_FLOAT));
        assert_eq!(host.column_type(statement, 2), Ok(ffi::SQLITE_TEXT));
        assert_eq!(host.column_type(statement, 3), Ok(ffi::SQLITE_BLOB));
        assert_eq!(host.column_type(statement, 4), Ok(ffi::SQLITE_NULL));
        assert_eq!(host.column_int64(statement, 0), Ok(5_000_000_000));
        assert_eq!(host.column_double(statement, 1), Ok(3.5));

        assert_eq!(
            host.column_text16_length(statement, 2),
            Ok(text.len() as u32)
        );
        let mut short_text = [0xffff; 2];
        assert_eq!(
            host.copy_column_text16(statement, 2, &mut short_text),
            Ok(text.len() as u32)
        );
        assert_eq!(short_text, [0xffff; 2]);
        let mut output_text = vec![0xffff; text.len()];
        assert_eq!(
            host.copy_column_text16(statement, 2, &mut output_text),
            Ok(text.len() as u32)
        );
        assert_eq!(output_text, text);

        assert_eq!(host.column_blob_length(statement, 3), Ok(blob.len() as u32));
        let mut short_blob = [0xaa; 2];
        assert_eq!(
            host.copy_column_blob(statement, 3, &mut short_blob),
            Ok(blob.len() as u32)
        );
        assert_eq!(short_blob, [0xaa; 2]);
        let mut output_blob = [0; 3];
        assert_eq!(
            host.copy_column_blob(statement, 3, &mut output_blob),
            Ok(blob.len() as u32)
        );
        assert_eq!(output_blob, blob);

        assert_eq!(host.finalize(statement), Ok(ffi::SQLITE_OK));
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }

    #[test]
    fn column_access_requires_a_current_row_and_valid_index() {
        let host = host();
        let database = open_memory(&host);
        let sql = utf16le("SELECT 42");
        let statement = host
            .prepare16_v2(database, &sql)
            .unwrap()
            .statement
            .unwrap();

        assert_eq!(
            host.column_type(statement, 0),
            Err(SqliteHostError::InvalidInput)
        );
        assert_eq!(host.step(statement), Ok(ffi::SQLITE_ROW));
        assert_eq!(
            host.column_int64(statement, -1),
            Err(SqliteHostError::InvalidInput)
        );
        assert_eq!(
            host.column_int64(statement, 1),
            Err(SqliteHostError::InvalidInput)
        );
        assert_eq!(host.column_int64(statement, 0), Ok(42));
        assert_eq!(host.step(statement), Ok(ffi::SQLITE_DONE));
        assert_eq!(
            host.column_type(statement, 0),
            Err(SqliteHostError::InvalidInput)
        );
        assert_eq!(host.finalize(statement), Ok(ffi::SQLITE_OK));
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }

    #[test]
    fn column_name_access_requires_a_valid_result_index() {
        let host = host();
        let database = open_memory(&host);
        let sql = utf16le("SELECT 42 AS value");
        let statement = host
            .prepare16_v2(database, &sql)
            .unwrap()
            .statement
            .unwrap();

        assert_eq!(
            host.column_name16_length(statement, -1),
            Err(SqliteHostError::InvalidInput)
        );
        assert_eq!(
            host.column_name16_length(statement, 1),
            Err(SqliteHostError::InvalidInput)
        );
        let mut output = [0xffff; 5];
        assert_eq!(
            host.copy_column_name16(statement, -1, &mut output),
            Err(SqliteHostError::InvalidInput)
        );
        assert_eq!(
            host.copy_column_name16(statement, 1, &mut output),
            Err(SqliteHostError::InvalidInput)
        );
        assert_eq!(output, [0xffff; 5]);

        assert_eq!(host.finalize(statement), Ok(ffi::SQLITE_OK));
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }

    #[test]
    fn column_names_distinguish_empty_strings_from_unavailable_names() {
        let host = host();
        let database = open_memory(&host);
        let sql = utf16le("SELECT 42 AS \"\"");
        let statement = host
            .prepare16_v2(database, &sql)
            .unwrap()
            .statement
            .unwrap();

        assert_eq!(host.column_name16_length(statement, 0), Ok(0));
        let mut output = [0xffff];
        assert_eq!(host.copy_column_name16(statement, 0, &mut output), Ok(0));
        assert_eq!(output, [0xffff]);
        assert_eq!(
            unsafe { utf16_string_length(std::ptr::null()) }.and_then(column_name_length),
            Ok(-1)
        );

        assert_eq!(host.finalize(statement), Ok(ffi::SQLITE_OK));
        assert_eq!(host.close(database), Ok(ffi::SQLITE_OK));
    }
}
