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

//! V8 adapter for the temporary whole-file filesystem imports.

use std::any::Any;
use std::cell::RefCell;
use std::sync::Arc;

use crate::async_policy::AsyncPolicy;
use crate::host_fs::{HostFs, HostFsState};
use crate::util::get_ref;
use crate::v8_builder::{ArgsExt, ObjectExt, ScopeExt};

struct FsImports {
    host: HostFs,
    // V8 invokes these imports synchronously on the isolate thread. Keep the
    // adapter's mutable protocol state local without imposing a threading
    // model on the engine-neutral HostFsState.
    state: RefCell<HostFsState>,
}

impl FsImports {
    fn new(policy: Arc<AsyncPolicy>) -> Self {
        Self {
            host: HostFs::new(policy),
            state: RefCell::new(HostFsState::default()),
        }
    }
}

/// `fn read_file_to_string(path: JSString) -> JSString`
fn read_file_to_string(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let contents = fs
        .host
        .read_file_to_string(&path)
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set(scope.string(&contents).into());
}

/// `fn write_string_to_file(path: JSString, contents: JSString) -> Unit`
fn write_string_to_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let contents = args.string_lossy(scope, 1);
    fs.host
        .write_string_to_file(&path, &contents)
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set_undefined();
}

fn write_bytes_to_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    fs.host
        .write_bytes_to_file(&path, || {
            let array = v8::Local::<v8::Uint8Array>::try_from(args.get(1)).unwrap();
            copy_uint8_array(array)
        })
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set_undefined();
}

fn create_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    fs.host
        .create_dir(&path)
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set_undefined();
}

fn read_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let entries = fs
        .host
        .read_dir(&path)
        .unwrap_or_else(|error| panic!("{error}"));
    let result = v8::Array::new(scope, 0);
    for (index, entry) in entries.iter().enumerate() {
        let entry = scope.string(entry);
        result.set_index(scope, index as u32, entry.into()).unwrap();
    }
    ret.set(result.into());
}

fn is_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    ret.set_bool(fs.host.is_file(&path));
}

fn is_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    ret.set_bool(fs.host.is_dir(&path));
}

fn remove_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    fs.host
        .remove_file(&path)
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set_undefined();
}

fn remove_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    fs.host
        .remove_dir(&path)
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set_undefined();
}

fn path_exists(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    ret.set_bool(fs.host.path_exists(&path));
}

fn current_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    ret.set(scope.string(&fs.host.current_dir()).into());
}

fn write_bytes_to_file_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let status = fs
        .host
        .write_bytes_to_file_new(&mut fs.state.borrow_mut(), &path, || {
            match v8::Local::<v8::Uint8Array>::try_from(args.get(1)) {
                Ok(array) => Ok(copy_uint8_array(array)),
                Err(_) => Err("Failed to convert contents to Uint8Array".to_string()),
            }
        });
    ret.set_int32(status);
}

fn read_file_to_bytes_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let status = fs
        .host
        .read_file_to_bytes_new(&mut fs.state.borrow_mut(), &path);
    ret.set_int32(status);
}

fn get_file_content(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let contents = fs.state.borrow().file_content().to_vec();
    let length = contents.len();
    let backing_store = v8::ArrayBuffer::new_backing_store_from_bytes(contents).make_shared();
    let array_buffer = v8::ArrayBuffer::with_backing_store(scope, &backing_store);
    let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, length).unwrap();
    ret.set(uint8_array.into());
}

fn get_dir_files(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let files = fs.state.borrow().dir_files().to_vec();
    let array = v8::Array::new(scope, 0);
    for (index, file) in files.iter().enumerate() {
        let file = scope.string(file);
        array.set_index(scope, index as u32, file.into()).unwrap();
    }
    ret.set(array.into());
}

fn create_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let status = fs.host.create_dir_new(&mut fs.state.borrow_mut(), &path);
    ret.set_int32(status);
}

fn read_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let status = fs.host.read_dir_new(&mut fs.state.borrow_mut(), &path);
    ret.set_int32(status);
}

fn is_file_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let status = fs.host.is_file_new(&mut fs.state.borrow_mut(), &path);
    ret.set_int32(status);
}

fn is_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let status = fs.host.is_dir_new(&mut fs.state.borrow_mut(), &path);
    ret.set_int32(status);
}

fn remove_file_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let status = fs.host.remove_file_new(&mut fs.state.borrow_mut(), &path);
    ret.set_int32(status);
}

fn remove_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let path = args.string_lossy(scope, 0);
    let status = fs.host.remove_dir_new(&mut fs.state.borrow_mut(), &path);
    ret.set_int32(status);
}

fn get_error_message(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fs = unsafe { get_ref::<FsImports>(&args) };
    let message = fs.state.borrow().error_message().to_owned();
    ret.set(scope.string(&message).into());
}

fn copy_uint8_array(array: v8::Local<'_, v8::Uint8Array>) -> Vec<u8> {
    let mut buffer = vec![0; array.byte_length()];
    array.copy_contents(&mut buffer);
    buffer
}

pub(crate) fn init_fs<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    policy: Arc<AsyncPolicy>,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let fs = Box::new(FsImports::new(policy));
    let fs_ptr = &*fs as *const FsImports;
    dtors.push(fs);

    set_host_func(
        obj,
        scope,
        "read_file_to_string",
        read_file_to_string,
        fs_ptr,
    );
    set_host_func(
        obj,
        scope,
        "write_string_to_file",
        write_string_to_file,
        fs_ptr,
    );
    set_host_func(
        obj,
        scope,
        "write_bytes_to_file",
        write_bytes_to_file,
        fs_ptr,
    );
    set_host_func(obj, scope, "create_dir", create_dir, fs_ptr);
    set_host_func(obj, scope, "read_dir", read_dir, fs_ptr);
    set_host_func(obj, scope, "is_file", is_file, fs_ptr);
    set_host_func(obj, scope, "is_dir", is_dir, fs_ptr);
    set_host_func(obj, scope, "remove_file", remove_file, fs_ptr);
    set_host_func(obj, scope, "remove_dir", remove_dir, fs_ptr);
    set_host_func(obj, scope, "path_exists", path_exists, fs_ptr);
    set_host_func(obj, scope, "current_dir", current_dir, fs_ptr);
    set_host_func(
        obj,
        scope,
        "read_file_to_bytes_new",
        read_file_to_bytes_new,
        fs_ptr,
    );
    set_host_func(
        obj,
        scope,
        "write_bytes_to_file_new",
        write_bytes_to_file_new,
        fs_ptr,
    );
    set_host_func(obj, scope, "get_file_content", get_file_content, fs_ptr);
    set_host_func(obj, scope, "get_dir_files", get_dir_files, fs_ptr);
    set_host_func(obj, scope, "get_error_message", get_error_message, fs_ptr);
    set_host_func(obj, scope, "create_dir_new", create_dir_new, fs_ptr);
    set_host_func(obj, scope, "read_dir_new", read_dir_new, fs_ptr);
    set_host_func(obj, scope, "is_file_new", is_file_new, fs_ptr);
    set_host_func(obj, scope, "is_dir_new", is_dir_new, fs_ptr);
    set_host_func(obj, scope, "remove_file_new", remove_file_new, fs_ptr);
    set_host_func(obj, scope, "remove_dir_new", remove_dir_new, fs_ptr);
}

fn set_host_func<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    fs_ptr: *const FsImports,
) {
    let data = v8::External::new(scope, fs_ptr as *mut std::ffi::c_void);
    let function = v8::Function::builder(callback)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set_value(scope, name, function.into());
}
