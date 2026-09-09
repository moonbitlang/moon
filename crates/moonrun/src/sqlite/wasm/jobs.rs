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

//! SQLite job construction and result copying. Scheduling, completion, status
//! transport, and release use the existing moonbitlang/async thread-pool ABI.
use super::context::{ImportContext, SqliteError, SqliteResult};

pub(super) fn make_open_job(
    context: &mut ImportContext<'_>,
    filename: u32,
    length: i32,
    flags: i32,
) -> SqliteResult<u64> {
    let length = u32::try_from(length).map_err(|_| SqliteError::Fault)?;
    let filename = context.read_utf8_c_string(filename, length)?;
    Ok(context
        .resources
        .insert_job(context.host.make_open_job(filename, flags))?)
}

pub(super) fn make_prepare_job(
    context: &mut ImportContext<'_>,
    database: u64,
    sql: u32,
    offset: i32,
    length: i32,
) -> SqliteResult<u64> {
    let offset = u32::try_from(offset).map_err(|_| SqliteError::Fault)?;
    let length = u32::try_from(length).map_err(|_| SqliteError::Fault)?;
    let sql = context.read_utf16_view(sql, offset, length)?.to_vec();
    let job = context.host.make_prepare_job(database, sql)?;
    Ok(context.resources.insert_job(job)?)
}

pub(super) fn make_step_job(
    context: &mut ImportContext<'_>,
    database: u64,
    statement: u64,
) -> SqliteResult<u64> {
    let job = context.host.make_step_job(database, statement)?;
    Ok(context.resources.insert_job(job)?)
}

pub(super) fn make_finalize_job(
    context: &mut ImportContext<'_>,
    database: u64,
    statement: u64,
) -> SqliteResult<u64> {
    let job = context.host.make_finalize_job(database, statement)?;
    Ok(context.resources.insert_job(job)?)
}

pub(super) fn make_discard_job(context: &mut ImportContext<'_>, job: u64) -> SqliteResult<u64> {
    let cleanup = context
        .resources
        .with_job_mut(job, |job| -> SqliteResult<_> {
            Ok(context.host.make_discard_job(job.sqlite_mut()?)?)
        })??;
    match cleanup {
        Some(cleanup) => Ok(context.resources.insert_job(cleanup)?),
        None => Ok(crate::runtime::null_handle()),
    }
}

pub(super) fn job_result(
    context: &mut ImportContext<'_>,
    job: u64,
    output: u32,
) -> SqliteResult<()> {
    context.validate_write(output, 24)?;
    let mut result = [0; 24];
    context
        .resources
        .with_job(job, |job| -> SqliteResult<()> {
            Ok(job.sqlite()?.copy_result(&mut result)?)
        })??;
    context.write_exact(output, &result)
}

pub(super) fn take_job_handle(context: &mut ImportContext<'_>, job: u64) -> SqliteResult<u64> {
    context.resources.with_job_mut(job, |job| {
        Ok(context.host.take_job_handle(job.sqlite_mut()?)?)
    })?
}

pub(super) fn job_message16_length(context: &mut ImportContext<'_>, job: u64) -> SqliteResult<u32> {
    context
        .resources
        .with_job(job, |job| Ok(job.sqlite()?.message16_length()?))?
}

pub(super) fn job_message16(
    context: &mut ImportContext<'_>,
    job: u64,
    output: u32,
    capacity: u32,
) -> SqliteResult<u32> {
    let resources = context.resources;
    context.with_utf16_output(output, capacity, |_, output| {
        resources.with_job(job, |job| Ok(job.sqlite()?.copy_message16(output)?))?
    })
}
