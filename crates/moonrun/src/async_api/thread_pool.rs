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

use std::ffi::OsString;

use crate::async_host::{AsyncHostError, AsyncHostResult, checked_range};
use crate::async_sys::internal::event_loop::thread_pool;

use super::context::{
    AsyncContext, ImportArgs, callback_context, throw_import_error, with_memory_mut,
};

pub(super) fn free_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.free_job(args.i32(0)?)
    })();
    if let Err(error) = result {
        throw_import_error(scope, "thread_pool/free_job", error);
        return;
    }
    ret.set_undefined();
}

pub(super) fn job_get_ret(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.job_get_ret(args.i32(0)?)
    })();
    match result {
        Ok(value) => ret.set_int32(value as i32),
        Err(error) => throw_import_error(scope, "thread_pool/job_get_ret", error),
    }
}

pub(super) fn job_get_err(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.job_get_err(args.i32(0)?)
    })();
    match result {
        Ok(value) => ret.set_int32(value),
        Err(error) => throw_import_error(scope, "thread_pool/job_get_err", error),
    }
}

pub(super) fn run_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = run_job_impl(scope, &args, context);
    if let Err(error) = result {
        throw_import_error(scope, "thread_pool/run_job", error);
        return;
    }
    ret.set_undefined();
}

pub(super) fn complete_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = complete_job_impl(scope, &args, context);
    if let Err(error) = result {
        throw_import_error(scope, "thread_pool/complete_job", error);
        return;
    }
    ret.set_undefined();
}

pub(super) fn spawn_worker(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.spawn_worker(args.i32(0)?, args.i32(1)?)
    })();
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/spawn_worker", error),
    }
}

pub(super) fn free_worker(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.free_worker(args.i32(0)?)
    })();
    if let Err(error) = result {
        throw_import_error(scope, "thread_pool/free_worker", error);
        return;
    }
    ret.set_undefined();
}

pub(super) fn wake_worker(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context
            .host
            .wake_worker(args.i32(0)?, args.i32(1)?, args.i32(2)?)
    })();
    if let Err(error) = result {
        throw_import_error(scope, "thread_pool/wake_worker", error);
        return;
    }
    ret.set_undefined();
}

pub(super) fn worker_enter_idle(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.worker_enter_idle(args.i32(0)?)
    })();
    if let Err(error) = result {
        throw_import_error(scope, "thread_pool/worker_enter_idle", error);
        return;
    }
    ret.set_undefined();
}

pub(super) fn cancel_worker(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.cancel_worker(args.i32(0)?)
    })();
    match result {
        Ok(status) => ret.set_int32(status),
        Err(error) => throw_import_error(scope, "thread_pool/cancel_worker", error),
    }
}

pub(super) fn fetch_completion(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = fetch_completion_impl(scope, &args, context);
    match result {
        Ok(bytes) => ret.set_int32(bytes),
        Err(error) => throw_import_error(scope, "thread_pool/fetch_completion", error),
    }
}

pub(super) fn make_sleep_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context
            .host
            .insert_job(thread_pool::make_sleep_job(args.i32(0)?))
    })();
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_sleep_job", error),
    }
}

pub(super) fn make_open_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_open_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_open_job", error),
    }
}

pub(super) fn make_read_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        let fd = args.i32(0)?;
        let ptr = args.i32(1)?;
        let offset = args.i32(2)?;
        let len = args.i32(3)?;
        let position = args.i64(4)?;
        context
            .host
            .insert_job(thread_pool::make_read_job(fd, ptr, offset, len, position))
    })();
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_read_job", error),
    }
}

pub(super) fn make_write_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_write_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_write_job", error),
    }
}

pub(super) fn make_file_kind_by_path_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_file_kind_by_path_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_file_kind_by_path_job", error),
    }
}

pub(super) fn make_file_size_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context
            .host
            .insert_job(thread_pool::make_file_size_job(args.i32(0)?))
    })();
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_file_size_job", error),
    }
}

pub(super) fn get_file_size_result(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.get_file_size_result(args.i32(0)?)
    })();
    match result {
        Ok(value) => ret.set(v8::BigInt::new_from_i64(scope, value).into()),
        Err(error) => throw_import_error(scope, "thread_pool/get_file_size_result", error),
    }
}

pub(super) fn make_file_time_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.insert_job(thread_pool::make_file_time_job(
            args.i32(0)?,
            args.i32(1)?,
            args.i32(2)?,
        ))
    })();
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_file_time_job", error),
    }
}

pub(super) fn make_file_time_by_path_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_file_time_by_path_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_file_time_by_path_job", error),
    }
}

pub(super) fn make_access_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_access_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_access_job", error),
    }
}

pub(super) fn make_chmod_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_chmod_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_chmod_job", error),
    }
}

pub(super) fn make_fsync_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        let fd = args.i32(0)?;
        let only_data = args.i32(1)? != 0;
        context
            .host
            .insert_job(thread_pool::make_fsync_job(fd, only_data))
    })();
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_fsync_job", error),
    }
}

pub(super) fn make_flock_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        let fd = args.i32(0)?;
        let exclusive = args.i32(1)? != 0;
        context
            .host
            .insert_job(thread_pool::make_flock_job(fd, exclusive))
    })();
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_flock_job", error),
    }
}

pub(super) fn make_remove_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_remove_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_remove_job", error),
    }
}

pub(super) fn make_rename_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_rename_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_rename_job", error),
    }
}

pub(super) fn make_symlink_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_symlink_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_symlink_job", error),
    }
}

pub(super) fn make_mkdir_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_mkdir_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_mkdir_job", error),
    }
}

pub(super) fn make_rmdir_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = make_rmdir_job_impl(scope, &args, context);
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_rmdir_job", error),
    }
}

pub(super) fn make_readdir_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        let dir = args.i32(0)?;
        let buf = args.i32(1)?;
        let len = args.i32(2)?;
        let restart = args.i32(3)? != 0;
        context
            .host
            .insert_job(thread_pool::make_readdir_job(dir, buf, len, restart))
    })();
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "thread_pool/make_readdir_job", error),
    }
}

pub(super) fn open_job_get_fd(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match open_job_i32(scope, &args, context, |handle| {
        context.host.open_job_get_fd(handle)
    }) {
        Ok(value) => ret.set_int32(value),
        Err(error) => throw_import_error(scope, "thread_pool/open_job_get_fd", error),
    }
}

pub(super) fn open_job_get_kind(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match open_job_i32(scope, &args, context, |handle| {
        context.host.open_job_get_kind(handle)
    }) {
        Ok(value) => ret.set_int32(value),
        Err(error) => throw_import_error(scope, "thread_pool/open_job_get_kind", error),
    }
}

pub(super) fn open_job_get_dev_id(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match open_job_u64(scope, &args, context, |handle| {
        context.host.open_job_get_dev_id(handle)
    }) {
        Ok(value) => ret.set(v8::BigInt::new_from_u64(scope, value).into()),
        Err(error) => throw_import_error(scope, "thread_pool/open_job_get_dev_id", error),
    }
}

pub(super) fn open_job_get_file_id(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match open_job_u64(scope, &args, context, |handle| {
        context.host.open_job_get_file_id(handle)
    }) {
        Ok(value) => ret.set(v8::BigInt::new_from_u64(scope, value).into()),
        Err(error) => throw_import_error(scope, "thread_pool/open_job_get_file_id", error),
    }
}

fn run_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<()> {
    let mut args = ImportArgs::new(scope, args);
    let job = args.i32(0)?;
    with_memory_mut(scope, context, |memory| context.host.run_job(memory, job))
}

fn complete_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<()> {
    let mut args = ImportArgs::new(scope, args);
    let job = args.i32(0)?;
    with_memory_mut(scope, context, |memory| {
        context.host.complete_job(memory, job)
    })
}

fn fetch_completion_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let dst = args.i32(0)?;
    let max_jobs = args.i32(1)?;
    with_memory_mut(scope, context, |memory| {
        context.host.fetch_completion(memory, dst, max_jobs)
    })
}

fn make_open_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let path_ptr = args.i32(0)?;
    let path_len = args.i32(1)?;
    let access = args.i32(2)?;
    let create_mode = args.i32(3)?;
    let append = args.i32(4)? != 0;
    let sync = args.i32(5)?;
    let mode = args.i32(6)?;

    let filename = read_guest_path(scope, context, path_ptr, path_len)?;

    context.host.insert_job(thread_pool::make_open_job(
        filename,
        access,
        create_mode,
        append,
        sync,
        mode,
    ))
}

fn make_write_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let fd = args.i32(0)?;
    let ptr = args.i32(1)?;
    let offset = args.i32(2)?;
    let len = args.i32(3)?;
    let position = args.i64(4)?;
    let offset_ptr = ptr.checked_add(offset).ok_or(AsyncHostError::Fault)?;
    let data = with_memory_mut(scope, context, |memory| {
        Ok(checked_range(memory, offset_ptr, len)?.to_vec())
    })?;

    context
        .host
        .insert_job(thread_pool::make_write_job(fd, data, position))
}

fn make_file_kind_by_path_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let parent = args.i32(0)?;
    let path_ptr = args.i32(1)?;
    let path_len = args.i32(2)?;
    let follow_symlink = args.i32(3)? != 0;
    let path = read_guest_path(scope, context, path_ptr, path_len)?;

    context
        .host
        .insert_job(thread_pool::make_file_kind_by_path_job(
            parent,
            path,
            follow_symlink,
        ))
}

fn make_file_time_by_path_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let path_ptr = args.i32(0)?;
    let path_len = args.i32(1)?;
    let out = args.i32(2)?;
    let out_len = args.i32(3)?;
    let follow_symlink = args.i32(4)? != 0;
    let path = read_guest_path(scope, context, path_ptr, path_len)?;

    context
        .host
        .insert_job(thread_pool::make_file_time_by_path_job(
            path,
            out,
            out_len,
            follow_symlink,
        ))
}

fn make_access_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let path_ptr = args.i32(0)?;
    let path_len = args.i32(1)?;
    let access = args.i32(2)?;
    let path = read_guest_path(scope, context, path_ptr, path_len)?;

    context
        .host
        .insert_job(thread_pool::make_access_job(path, access))
}

fn make_chmod_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let path_ptr = args.i32(0)?;
    let path_len = args.i32(1)?;
    let mode = args.i32(2)?;
    let path = read_guest_path(scope, context, path_ptr, path_len)?;

    context
        .host
        .insert_job(thread_pool::make_chmod_job(path, mode))
}

fn make_remove_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let path_ptr = args.i32(0)?;
    let path_len = args.i32(1)?;
    let path = read_guest_path(scope, context, path_ptr, path_len)?;

    context.host.insert_job(thread_pool::make_remove_job(path))
}

fn make_rename_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let old_path_ptr = args.i32(0)?;
    let old_path_len = args.i32(1)?;
    let new_path_ptr = args.i32(2)?;
    let new_path_len = args.i32(3)?;
    let replace = args.i32(4)? != 0;
    let old_path = read_guest_path(scope, context, old_path_ptr, old_path_len)?;
    let new_path = read_guest_path(scope, context, new_path_ptr, new_path_len)?;

    context
        .host
        .insert_job(thread_pool::make_rename_job(old_path, new_path, replace))
}

fn make_symlink_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let target_ptr = args.i32(0)?;
    let target_len = args.i32(1)?;
    let path_ptr = args.i32(2)?;
    let path_len = args.i32(3)?;
    let target = read_guest_path(scope, context, target_ptr, target_len)?;
    let path = read_guest_path(scope, context, path_ptr, path_len)?;

    context
        .host
        .insert_job(thread_pool::make_symlink_job(target, path))
}

fn make_mkdir_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let path_ptr = args.i32(0)?;
    let path_len = args.i32(1)?;
    let mode = args.i32(2)?;
    let path = read_guest_path(scope, context, path_ptr, path_len)?;

    context
        .host
        .insert_job(thread_pool::make_mkdir_job(path, mode))
}

fn make_rmdir_job_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let path_ptr = args.i32(0)?;
    let path_len = args.i32(1)?;
    let path = read_guest_path(scope, context, path_ptr, path_len)?;

    context.host.insert_job(thread_pool::make_rmdir_job(path))
}

fn read_guest_path(
    scope: &mut v8::HandleScope,
    context: &AsyncContext,
    ptr: i32,
    len: i32,
) -> AsyncHostResult<OsString> {
    with_memory_mut(scope, context, |memory| {
        let bytes = checked_range(memory, ptr, len)?;
        decode_guest_path(bytes)
    })
}

#[cfg(unix)]
fn decode_guest_path(bytes: &[u8]) -> AsyncHostResult<OsString> {
    use std::os::unix::ffi::OsStringExt;

    Ok(OsString::from_vec(bytes.to_vec()))
}

#[cfg(windows)]
fn decode_guest_path(bytes: &[u8]) -> AsyncHostResult<OsString> {
    // `thread_pool.wasm.mbt` builds job paths with `@utf8.encode(path.to_string())`.
    // Host result paths, such as `get_tmp_path`, use a separate OS-string encoding path.
    let path = std::str::from_utf8(bytes).map_err(|_| AsyncHostError::Inval)?;
    Ok(OsString::from(path))
}

fn open_job_i32(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    _context: &AsyncContext,
    f: impl FnOnce(i32) -> AsyncHostResult<i32>,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    f(args.i32(0)?)
}

fn open_job_u64(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    _context: &AsyncContext,
    f: impl FnOnce(i32) -> AsyncHostResult<u64>,
) -> AsyncHostResult<u64> {
    let mut args = ImportArgs::new(scope, args);
    f(args.i32(0)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn guest_path_decodes_as_raw_unix_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let path = decode_guest_path(b"async-\xff.txt").unwrap();

        assert_eq!(path.as_os_str().as_bytes(), b"async-\xff.txt");
    }

    #[cfg(windows)]
    #[test]
    fn guest_path_decodes_as_utf8_on_windows() {
        use std::os::windows::ffi::OsStrExt;

        let path = decode_guest_path("async-fs-smoke-\u{6587}.txt".as_bytes()).unwrap();

        assert_eq!(
            path.as_os_str().encode_wide().collect::<Vec<_>>(),
            "async-fs-smoke-\u{6587}.txt"
                .encode_utf16()
                .collect::<Vec<_>>()
        );
    }

    #[cfg(windows)]
    #[test]
    fn guest_path_accepts_odd_length_utf8_on_windows() {
        let path = decode_guest_path(b"a.txt").unwrap();

        assert_eq!(path, OsString::from("a.txt"));
    }

    #[cfg(windows)]
    #[test]
    fn guest_path_rejects_invalid_utf8_on_windows() {
        assert!(matches!(
            decode_guest_path(b"async-\xff.txt"),
            Err(AsyncHostError::Inval)
        ));
    }
}
