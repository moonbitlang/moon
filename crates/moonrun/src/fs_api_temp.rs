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

use crate::filesystem::{FsOperationResults, HostFs};
use crate::policy::Policy;
use crate::util::get_ref;
use crate::v8_builder::{ArgsExt, ObjectExt, ScopeExt};

struct FsImports {
    filesystem: HostFs,
    // V8 invokes these imports synchronously on the isolate thread. Keep the
    // adapter's mutable protocol state local without imposing a threading
    // model on the engine-neutral FsOperationResults.
    operation_results: RefCell<FsOperationResults>,
}

impl FsImports {
    fn new(policy: Arc<Policy>) -> Self {
        Self {
            filesystem: HostFs::new(policy),
            operation_results: RefCell::new(FsOperationResults::default()),
        }
    }
}

fn fs_imports<'a>(args: &v8::FunctionCallbackArguments<'a>) -> &'a FsImports {
    // SAFETY: every callback using this helper is registered by `init_fs`
    // through `set_host_func` with a pointer to the same boxed `FsImports`.
    // `dtors` owns that box until after V8 can no longer invoke the callbacks.
    unsafe { get_ref::<FsImports>(args) }
}

/// `fn read_file_to_string(path: JSString) -> JSString`
fn read_file_to_string(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let contents = imports
        .filesystem
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
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let contents = args.string_lossy(scope, 1);
    imports
        .filesystem
        .write_string_to_file(&path, &contents)
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set_undefined();
}

fn write_bytes_to_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    imports
        .filesystem
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
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    imports
        .filesystem
        .create_dir(&path)
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set_undefined();
}

fn read_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let entries = imports
        .filesystem
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
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    ret.set_bool(imports.filesystem.is_file(&path));
}

fn is_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    ret.set_bool(imports.filesystem.is_dir(&path));
}

fn remove_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    imports
        .filesystem
        .remove_file(&path)
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set_undefined();
}

fn remove_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    imports
        .filesystem
        .remove_dir(&path)
        .unwrap_or_else(|error| panic!("{error}"));
    ret.set_undefined();
}

fn path_exists(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    ret.set_bool(imports.filesystem.path_exists(&path));
}

fn current_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    ret.set(scope.string(&imports.filesystem.current_dir()).into());
}

fn write_bytes_to_file_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let status = imports.filesystem.write_bytes_to_file_new(
        &mut imports.operation_results.borrow_mut(),
        &path,
        || match v8::Local::<v8::Uint8Array>::try_from(args.get(1)) {
            Ok(array) => Ok(copy_uint8_array(array)),
            Err(_) => Err("Failed to convert contents to Uint8Array".to_string()),
        },
    );
    ret.set_int32(status);
}

fn read_file_to_bytes_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let status = imports
        .filesystem
        .read_file_to_bytes_new(&mut imports.operation_results.borrow_mut(), &path);
    ret.set_int32(status);
}

fn get_file_content(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let contents = imports.operation_results.borrow().file_content().to_vec();
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
    let imports = fs_imports(&args);
    let files = imports.operation_results.borrow().dir_files().to_vec();
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
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let status = imports
        .filesystem
        .create_dir_new(&mut imports.operation_results.borrow_mut(), &path);
    ret.set_int32(status);
}

fn read_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let status = imports
        .filesystem
        .read_dir_new(&mut imports.operation_results.borrow_mut(), &path);
    ret.set_int32(status);
}

fn is_file_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let status = imports
        .filesystem
        .is_file_new(&mut imports.operation_results.borrow_mut(), &path);
    ret.set_int32(status);
}

fn is_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let status = imports
        .filesystem
        .is_dir_new(&mut imports.operation_results.borrow_mut(), &path);
    ret.set_int32(status);
}

fn remove_file_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let status = imports
        .filesystem
        .remove_file_new(&mut imports.operation_results.borrow_mut(), &path);
    ret.set_int32(status);
}

fn remove_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let path = args.string_lossy(scope, 0);
    let status = imports
        .filesystem
        .remove_dir_new(&mut imports.operation_results.borrow_mut(), &path);
    ret.set_int32(status);
}

fn get_error_message(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let imports = fs_imports(&args);
    let message = imports
        .operation_results
        .borrow()
        .error_message()
        .to_owned();
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
    policy: Arc<Policy>,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let imports = Box::new(FsImports::new(policy));
    let imports_ptr = &*imports as *const FsImports;
    dtors.push(imports);

    set_host_func(
        obj,
        scope,
        "read_file_to_string",
        read_file_to_string,
        imports_ptr,
    );
    set_host_func(
        obj,
        scope,
        "write_string_to_file",
        write_string_to_file,
        imports_ptr,
    );
    set_host_func(
        obj,
        scope,
        "write_bytes_to_file",
        write_bytes_to_file,
        imports_ptr,
    );
    set_host_func(obj, scope, "create_dir", create_dir, imports_ptr);
    set_host_func(obj, scope, "read_dir", read_dir, imports_ptr);
    set_host_func(obj, scope, "is_file", is_file, imports_ptr);
    set_host_func(obj, scope, "is_dir", is_dir, imports_ptr);
    set_host_func(obj, scope, "remove_file", remove_file, imports_ptr);
    set_host_func(obj, scope, "remove_dir", remove_dir, imports_ptr);
    set_host_func(obj, scope, "path_exists", path_exists, imports_ptr);
    set_host_func(obj, scope, "current_dir", current_dir, imports_ptr);
    set_host_func(
        obj,
        scope,
        "read_file_to_bytes_new",
        read_file_to_bytes_new,
        imports_ptr,
    );
    set_host_func(
        obj,
        scope,
        "write_bytes_to_file_new",
        write_bytes_to_file_new,
        imports_ptr,
    );
    set_host_func(
        obj,
        scope,
        "get_file_content",
        get_file_content,
        imports_ptr,
    );
    set_host_func(obj, scope, "get_dir_files", get_dir_files, imports_ptr);
    set_host_func(
        obj,
        scope,
        "get_error_message",
        get_error_message,
        imports_ptr,
    );
    set_host_func(obj, scope, "create_dir_new", create_dir_new, imports_ptr);
    set_host_func(obj, scope, "read_dir_new", read_dir_new, imports_ptr);
    set_host_func(obj, scope, "is_file_new", is_file_new, imports_ptr);
    set_host_func(obj, scope, "is_dir_new", is_dir_new, imports_ptr);
    set_host_func(obj, scope, "remove_file_new", remove_file_new, imports_ptr);
    set_host_func(obj, scope, "remove_dir_new", remove_dir_new, imports_ptr);
}

fn set_host_func<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    imports_ptr: *const FsImports,
) {
    let data = v8::External::new(scope, imports_ptr as *mut std::ffi::c_void);
    let function = v8::Function::builder(callback)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set_value(scope, name, function.into());
}
