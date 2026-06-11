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

use crate::async_host::{AsyncHostError, AsyncHostResult, GuestMemory, GuestRange};

use super::context::{
    AsyncContext, ImportArgs, callback_context, throw_import_error, with_memory_mut,
};

pub(super) fn spawn_process(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match spawn_process_impl(scope, &args, context) {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

pub(super) fn make_wait_for_process_job(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.make_wait_for_process_job(args.i32(0)?)
    })();
    match result {
        Ok(handle) => ret.set_int32(handle),
        Err(error) => throw_import_error(scope, "make_wait_for_process_job", error),
    }
}

fn spawn_process_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let argv_ptr = args.i32(0)?;
    let argv_len = args.i32(1)?;
    let argc = args.i32(2)?;
    let stdin = args.i32(3)?;
    let stdout = args.i32(4)?;
    let stderr = args.i32(5)?;

    let (command, argv) = with_memory_mut(scope, context, |memory| {
        let mut argv = read_packed_argv(memory, argv_ptr, argv_len, argc)?;
        if argv.is_empty() {
            return Err(AsyncHostError::Inval);
        }
        let command = argv.remove(0);
        Ok((command, argv))
    })?;

    context
        .host
        .spawn_process(command, argv, stdin, stdout, stderr)
}

fn read_packed_argv(
    memory: &(impl GuestMemory + ?Sized),
    ptr: i32,
    len: i32,
    argc: i32,
) -> AsyncHostResult<Vec<String>> {
    let bytes = memory.read(GuestRange::new(ptr, len)?)?;
    let argc = usize::try_from(argc).map_err(|_| AsyncHostError::Fault)?;
    let mut argv = Vec::with_capacity(argc);
    let mut offset = 0usize;
    for _ in 0..argc {
        let end = offset.checked_add(4).ok_or(AsyncHostError::Fault)?;
        let len_bytes = bytes.get(offset..end).ok_or(AsyncHostError::Fault)?;
        let arg_len =
            u32::from_le_bytes(len_bytes.try_into().map_err(|_| AsyncHostError::Fault)?) as usize;
        offset = end;
        let end = offset.checked_add(arg_len).ok_or(AsyncHostError::Fault)?;
        let arg_bytes = bytes.get(offset..end).ok_or(AsyncHostError::Fault)?;
        let arg = std::str::from_utf8(arg_bytes).map_err(|_| AsyncHostError::Inval)?;
        argv.push(arg.to_owned());
        offset = end;
    }
    if offset != bytes.len() {
        return Err(AsyncHostError::Inval);
    }
    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::read_packed_argv;

    #[test]
    fn read_packed_argv_decodes_len_prefixed_utf8_args() {
        let mut bytes = Vec::new();
        for arg in ["moon", "build", "test_programs/lock_file"] {
            bytes.extend_from_slice(&(arg.len() as u32).to_le_bytes());
            bytes.extend_from_slice(arg.as_bytes());
        }
        let argv = read_packed_argv(bytes.as_slice(), 0, bytes.len() as i32, 3).unwrap();
        assert_eq!(argv, ["moon", "build", "test_programs/lock_file"]);
    }
}
