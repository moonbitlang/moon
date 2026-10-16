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

use super::context::{ImportContext, SqliteError};
use super::{bind, column, connection, jobs, statement};
use crate::runtime::Runtime;
use crate::wasm_imports::{Caller, ImportError, ImportRegistry};

use super::MOONBIT_SQLITE_MODULE;

/// One numeric ABI definition for every backend. Host-only operations do not
/// require guest memory; memory-based operations preserve the existing checked
/// SQLite context and its null-pointer contract.
pub(crate) fn register<R: ImportRegistry<Runtime>>(registry: &mut R) -> Result<(), R::Error> {
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_null_handle",
        |caller: Caller<'_, Runtime>| Ok(caller.data.null_handle()),
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_make_open_job",
        |caller: Caller<'_, Runtime>, filename: u32, length: i32, flags: i32| {
            let mut context = ImportContext::try_from(caller)?;
            jobs::make_open_job(&mut context, filename, length, flags).map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_make_prepare_job",
        |caller: Caller<'_, Runtime>, database: u64, sql: u32, offset: i32, length: i32| {
            let mut context = ImportContext::try_from(caller)?;
            jobs::make_prepare_job(&mut context, database, sql, offset, length)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_make_step_job",
        |caller: Caller<'_, Runtime>, database: u64, statement: u64| {
            let mut context = ImportContext::try_from(caller)?;
            jobs::make_step_job(&mut context, database, statement).map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_make_finalize_job",
        |caller: Caller<'_, Runtime>, database: u64, statement: u64| {
            let mut context = ImportContext::try_from(caller)?;
            jobs::make_finalize_job(&mut context, database, statement).map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_make_discard_job",
        |caller: Caller<'_, Runtime>, job: u64| {
            let mut context = ImportContext::try_from(caller)?;
            jobs::make_discard_job(&mut context, job).map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_job_result",
        |caller: Caller<'_, Runtime>, job: u64, output: u32| {
            let mut context = ImportContext::try_from(caller)?;
            jobs::job_result(&mut context, job, output).map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_take_job_handle",
        |caller: Caller<'_, Runtime>, job: u64| {
            let mut context = ImportContext::try_from(caller)?;
            jobs::take_job_handle(&mut context, job).map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_job_message16_length",
        |caller: Caller<'_, Runtime>, job: u64| {
            let mut context = ImportContext::try_from(caller)?;
            jobs::job_message16_length(&mut context, job).map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_job_message16",
        |caller: Caller<'_, Runtime>, job: u64, output: u32, capacity: u32| {
            let mut context = ImportContext::try_from(caller)?;
            jobs::job_message16(&mut context, job, output, capacity).map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_open_v2",
        |caller: Caller<'_, Runtime>,
         filename: u32,
         filename_length: i32,
         database_out: u32,
         flags: i32,
         vfs: u64| {
            let mut context = ImportContext::try_from(caller)?;
            connection::open_v2(
                &mut context,
                filename,
                filename_length,
                database_out,
                flags,
                vfs,
            )
            .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_errmsg16_length",
        |caller: Caller<'_, Runtime>, database: u64| {
            caller
                .data
                .sqlite()
                .errmsg16_length(database)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_errmsg16",
        |caller: Caller<'_, Runtime>, database: u64, output: u32, capacity: u32| {
            let mut context = ImportContext::try_from(caller)?;
            connection::errmsg16(&mut context, database, output, capacity)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_errcode",
        |caller: Caller<'_, Runtime>, database: u64| {
            caller
                .data
                .sqlite()
                .errcode(database)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_extended_errcode",
        |caller: Caller<'_, Runtime>, database: u64| {
            caller
                .data
                .sqlite()
                .extended_errcode(database)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_changes64",
        |caller: Caller<'_, Runtime>, database: u64| {
            caller
                .data
                .sqlite()
                .changes64(database)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_db_mutex",
        |caller: Caller<'_, Runtime>, database: u64| {
            caller
                .data
                .sqlite()
                .db_mutex(database)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_mutex_enter",
        |caller: Caller<'_, Runtime>, mutex: u64| {
            caller
                .data
                .sqlite()
                .mutex_enter(mutex)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_mutex_leave",
        |caller: Caller<'_, Runtime>, mutex: u64| {
            caller
                .data
                .sqlite()
                .mutex_leave(mutex)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_close",
        |caller: Caller<'_, Runtime>, database: u64| {
            caller
                .data
                .sqlite()
                .close(database)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_prepare16_v2",
        |caller: Caller<'_, Runtime>,
         database: u64,
         sql: u32,
         sql_offset: i32,
         sql_length: i32,
         statement_out: u32,
         tail_out: u32| {
            let mut context = ImportContext::try_from(caller)?;
            statement::prepare16_v2(
                &mut context,
                database,
                sql,
                sql_offset,
                sql_length,
                statement_out,
                tail_out,
            )
            .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_bind_null",
        |caller: Caller<'_, Runtime>, statement: u64, index: i32| {
            caller
                .data
                .sqlite()
                .bind_null(statement, index)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_bind_int64",
        |caller: Caller<'_, Runtime>, statement: u64, index: i32, value: i64| {
            caller
                .data
                .sqlite()
                .bind_int64(statement, index, value)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_bind_double",
        |caller: Caller<'_, Runtime>, statement: u64, index: i32, value: f64| {
            caller
                .data
                .sqlite()
                .bind_double(statement, index, value)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_bind_text16",
        |caller: Caller<'_, Runtime>,
         statement: u64,
         index: i32,
         value: u32,
         value_offset: i32,
         value_length: i32| {
            let mut context = ImportContext::try_from(caller)?;
            bind::bind_text16(
                &mut context,
                statement,
                index,
                value,
                value_offset,
                value_length,
            )
            .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_bind_blob",
        |caller: Caller<'_, Runtime>,
         statement: u64,
         index: i32,
         value: u32,
         value_offset: i32,
         value_length: i32| {
            let mut context = ImportContext::try_from(caller)?;
            bind::bind_blob(
                &mut context,
                statement,
                index,
                value,
                value_offset,
                value_length,
            )
            .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_step",
        |caller: Caller<'_, Runtime>, statement: u64| {
            caller
                .data
                .sqlite()
                .step(statement)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_reset",
        |caller: Caller<'_, Runtime>, statement: u64| {
            caller
                .data
                .sqlite()
                .reset(statement)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_clear_bindings",
        |caller: Caller<'_, Runtime>, statement: u64| {
            caller
                .data
                .sqlite()
                .clear_bindings(statement)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_finalize",
        |caller: Caller<'_, Runtime>, statement: u64| {
            caller
                .data
                .sqlite()
                .finalize(statement)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_count",
        |caller: Caller<'_, Runtime>, statement: u64| {
            caller
                .data
                .sqlite()
                .column_count(statement)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_name16_length",
        |caller: Caller<'_, Runtime>, statement: u64, column: i32| {
            caller
                .data
                .sqlite()
                .column_name16_length(statement, column)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_name16",
        |caller: Caller<'_, Runtime>, statement: u64, column: i32, output: u32, capacity: u32| {
            let mut context = ImportContext::try_from(caller)?;
            column::column_name16(&mut context, statement, column, output, capacity)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_type",
        |caller: Caller<'_, Runtime>, statement: u64, column: i32| {
            caller
                .data
                .sqlite()
                .column_type(statement, column)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_int64",
        |caller: Caller<'_, Runtime>, statement: u64, column: i32| {
            caller
                .data
                .sqlite()
                .column_int64(statement, column)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_double",
        |caller: Caller<'_, Runtime>, statement: u64, column: i32| {
            caller
                .data
                .sqlite()
                .column_double(statement, column)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_text16_length",
        |caller: Caller<'_, Runtime>, statement: u64, column: i32| {
            caller
                .data
                .sqlite()
                .column_text16_length(statement, column)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_text16",
        |caller: Caller<'_, Runtime>, statement: u64, column: i32, output: u32, capacity: u32| {
            let mut context = ImportContext::try_from(caller)?;
            column::column_text16(&mut context, statement, column, output, capacity)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_blob_length",
        |caller: Caller<'_, Runtime>, statement: u64, column: i32| {
            caller
                .data
                .sqlite()
                .column_blob_length(statement, column)
                .map_err(SqliteError::from)
                .map_err(ImportError::from)
        },
    )?;
    registry.func_wrap(
        MOONBIT_SQLITE_MODULE,
        "sqlite3_column_blob",
        |caller: Caller<'_, Runtime>, statement: u64, column: i32, output: u32, capacity: u32| {
            let mut context = ImportContext::try_from(caller)?;
            column::column_blob(&mut context, statement, column, output, capacity)
                .map_err(ImportError::from)
        },
    )?;
    Ok(())
}
