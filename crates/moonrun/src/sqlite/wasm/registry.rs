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

use super::context::{SqliteError, with_memory_context};
use super::registry_macros::declare_sqlite_imports;
use super::{bind, column, connection, statement};
use crate::v8::context::{ImportArgs, V8ImportError, V8RunContext};

pub(crate) const MOONBIT_SQLITE_MODULE: &str = "moonbitlang/sqlite";

// This is the complete `moonbitlang/sqlite` ABI surface. The declaration keeps
// callbacks and registration generated together, so adding a symbol cannot
// update one without the other. Each entry names its implementation target;
// the registry does not expose how the macro reaches that target.
declare_sqlite_imports! {
    Runtime::null_handle() -> u64 => "sqlite3_null_handle";

    connection::open_v2(
        filename: u32,
        filename_length: i32,
        database_out: u32,
        flags: i32,
        vfs: u64,
    ) -> i32 => "sqlite3_open_v2";
    SqliteHost::errmsg16_length(database: u64)
        -> u32 => "sqlite3_errmsg16_length";
    connection::errmsg16(
        database: u64,
        output: u32,
        capacity: u32,
    ) -> u32 => "sqlite3_errmsg16";
    SqliteHost::errcode(database: u64) -> i32 => "sqlite3_errcode";
    SqliteHost::extended_errcode(database: u64)
        -> i32 => "sqlite3_extended_errcode";
    SqliteHost::changes64(database: u64) -> i64 => "sqlite3_changes64";
    SqliteHost::db_mutex(database: u64) -> u64 => "sqlite3_db_mutex";
    SqliteHost::mutex_enter(mutex: u64) -> void => "sqlite3_mutex_enter";
    SqliteHost::mutex_leave(mutex: u64) -> void => "sqlite3_mutex_leave";
    SqliteHost::close(database: u64) -> i32 => "sqlite3_close";

    statement::prepare16_v2(
        database: u64,
        sql: u32,
        sql_offset: i32,
        sql_length: i32,
        statement_out: u32,
        tail_out: u32,
    ) -> i32 => "sqlite3_prepare16_v2";
    SqliteHost::bind_null(statement: u64, index: i32)
        -> i32 => "sqlite3_bind_null";
    SqliteHost::bind_int64(statement: u64, index: i32, value: i64)
        -> i32 => "sqlite3_bind_int64";
    SqliteHost::bind_double(statement: u64, index: i32, value: f64)
        -> i32 => "sqlite3_bind_double";
    bind::bind_text16(
        statement: u64,
        index: i32,
        value: u32,
        value_offset: i32,
        value_length: i32,
    ) -> i32 => "sqlite3_bind_text16";
    bind::bind_blob(
        statement: u64,
        index: i32,
        value: u32,
        value_offset: i32,
        value_length: i32,
    ) -> i32 => "sqlite3_bind_blob";
    SqliteHost::step(statement: u64) -> i32 => "sqlite3_step";
    SqliteHost::reset(statement: u64) -> i32 => "sqlite3_reset";
    SqliteHost::clear_bindings(statement: u64)
        -> i32 => "sqlite3_clear_bindings";
    SqliteHost::finalize(statement: u64) -> i32 => "sqlite3_finalize";
    SqliteHost::column_count(statement: u64) -> i32 => "sqlite3_column_count";
    SqliteHost::column_name16_length(statement: u64, column: i32)
        -> i32 => "sqlite3_column_name16_length";
    column::column_name16(
        statement: u64,
        column: i32,
        output: u32,
        capacity: u32,
    ) -> i32 => "sqlite3_column_name16";
    SqliteHost::column_type(statement: u64, column: i32)
        -> i32 => "sqlite3_column_type";
    SqliteHost::column_int64(statement: u64, column: i32)
        -> i64 => "sqlite3_column_int64";
    SqliteHost::column_double(statement: u64, column: i32)
        -> f64 => "sqlite3_column_double";
    SqliteHost::column_text16_length(statement: u64, column: i32)
        -> u32 => "sqlite3_column_text16_length";
    column::column_text16(
        statement: u64,
        column: i32,
        output: u32,
        capacity: u32,
    ) -> u32 => "sqlite3_column_text16";
    SqliteHost::column_blob_length(statement: u64, column: i32)
        -> u32 => "sqlite3_column_blob_length";
    column::column_blob(
        statement: u64,
        column: i32,
        output: u32,
        capacity: u32,
    ) -> u32 => "sqlite3_column_blob";
}
