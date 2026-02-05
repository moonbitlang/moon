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

//! Temporary-use FS API. Only has whole-file read/write and no other features.

/// `fn read_file_to_string(path: JSString) -> JSString`
fn read_file_to_string<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    let contents =
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read file: {path}"));
    let contents = scope.string(&contents);
    ret.set(contents.into());
}

/// `fn write_string_to_file(path: JSString, contents: JSString) -> Unit`
fn write_string_to_file<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);
    let contents = args.string_lossy(scope, 1);

    std::fs::write(&path, contents).unwrap_or_else(|_| panic!("Failed to write file: {path}"));

    ret.set_undefined()
}

fn write_bytes_to_file<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);
    let contents = args.get(1);

    let uint8_array = v8::Local::<v8::Uint8Array>::try_from(contents).unwrap();
    let length = uint8_array.byte_length();
    let mut buffer = vec![0; length];
    uint8_array.copy_contents(&mut buffer);

    std::fs::write(&path, buffer).unwrap_or_else(|_| panic!("Failed to write file: {path}"));

    ret.set_undefined()
}

fn create_dir<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    std::fs::create_dir_all(&path).unwrap_or_else(|_| panic!("Failed to create directory: {path}"));

    ret.set_undefined()
}

#[allow(clippy::manual_flatten)]
fn read_dir<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    let entries =
        std::fs::read_dir(&path).unwrap_or_else(|_| panic!("Failed to read directory: {path}"));

    let result = v8::Array::new(scope, 0);
    let mut index = 0;

    for entry in entries {
        if let Ok(entry) = entry {
            let rust_style_path = entry.path();
            let node_style_path = rust_style_path.strip_prefix(&path).unwrap();
            if let Some(path_str) = node_style_path.to_str() {
                let js_string = scope.string(path_str);
                result.set_index(scope, index, js_string.into()).unwrap();
                index += 1;
            }
        }
    }

    ret.set(result.into());
}

fn is_file<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    let is_file = std::path::Path::new(&path).is_file();
    ret.set_bool(is_file);
}

fn is_dir<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    let is_dir = std::path::Path::new(&path).is_dir();
    ret.set_bool(is_dir);
}

fn remove_file<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    std::fs::remove_file(&path).unwrap_or_else(|_| panic!("Failed to remove file: {path}"));

    ret.set_undefined();
}

fn remove_dir<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    std::fs::remove_dir_all(&path).unwrap_or_else(|_| panic!("Failed to remove directory: {path}"));

    ret.set_undefined();
}

fn path_exists<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    let exists = std::path::Path::new(&path).exists();
    ret.set_bool(exists);
}

fn current_dir(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let current_dir = std::env::current_dir().unwrap_or_default();
    let current_dir = current_dir.to_str().unwrap();
    let current_dir = scope.string(current_dir);
    ret.set(current_dir.into());
}

// new ffi with error handling, use in moonbitlang/core

use std::sync::{LazyLock, Mutex};

use crate::v8_builder::{ArgsExt, ObjectExt, ScopeExt};

static GLOBAL_STATE: LazyLock<Mutex<GlobalState>> = LazyLock::new(|| {
    Mutex::new(GlobalState {
        file_content: Vec::new(),
        dir_files: Vec::new(),
        error_message: String::new(),
    })
});

struct GlobalState {
    file_content: Vec<u8>,
    dir_files: Vec<String>,
    error_message: String,
}

fn write_bytes_to_file_new<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    let contents = args.get(1);
    let uint8_array = match v8::Local::<v8::Uint8Array>::try_from(contents) {
        Ok(array) => array,
        Err(_) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                "Failed to convert contents to Uint8Array".to_string();
            ret.set_int32(-1);
            return;
        }
    };

    let length = uint8_array.byte_length();
    let mut buffer = vec![0; length];
    uint8_array.copy_contents(&mut buffer);

    match std::fs::write(&path, buffer) {
        Ok(_) => {
            ret.set_int32(0);
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to write file {path}: {e}");
            ret.set_int32(-1);
        }
    }
}

fn read_file_to_bytes_new<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    match std::fs::read(&path) {
        Ok(contents) => {
            GLOBAL_STATE.lock().unwrap().file_content = contents;
            ret.set_int32(0);
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message = format!("Failed to read file {path}: {e}");
            ret.set_int32(-1);
        }
    }
}

fn get_file_content(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let state = GLOBAL_STATE.lock().unwrap();
    let array_buffer = v8::ArrayBuffer::with_backing_store(
        scope,
        &v8::ArrayBuffer::new_backing_store_from_bytes(state.file_content.clone()).make_shared(),
    );
    let uint8_array =
        v8::Uint8Array::new(scope, array_buffer, 0, state.file_content.len()).unwrap();
    ret.set(uint8_array.into());
}

fn get_dir_files(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let state = GLOBAL_STATE.lock().unwrap();
    let array = v8::Array::new(scope, 0);
    for (index, file) in state.dir_files.iter().enumerate() {
        let js_string = scope.string(file);
        array
            .set_index(scope, index as u32, js_string.into())
            .unwrap();
    }
    ret.set(array.into());
}

fn create_dir_new<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    match std::fs::create_dir_all(&path) {
        Ok(_) => {
            ret.set_int32(0);
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to create directory {path}: {e}");
            ret.set_int32(-1);
        }
    }
}

#[allow(clippy::manual_flatten)]
fn read_dir_new<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    let entries = match std::fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to read directory {path}: {e}");
            ret.set_int32(-1);
            return;
        }
    };

    let mut dir_files = Vec::new();
    for entry in entries {
        if let Ok(entry) = entry {
            let rust_style_path = entry.path();
            let node_style_path = rust_style_path.strip_prefix(&path).unwrap();
            if let Some(path_str) = node_style_path.to_str() {
                dir_files.push(path_str.to_string());
            }
        }
    }

    GLOBAL_STATE.lock().unwrap().dir_files = dir_files;

    ret.set_int32(0);
}

fn is_file_new<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    let is_file = match std::fs::metadata(&path) {
        Ok(metadata) => {
            if metadata.is_file() {
                1
            } else {
                0
            }
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message = format!("{e}: {path}");
            -1
        }
    };
    ret.set_int32(is_file);
}

fn is_dir_new<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    let is_dir = match std::fs::metadata(&path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                1
            } else {
                0
            }
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message = format!("{e}: {path}");
            -1
        }
    };
    ret.set_int32(is_dir);
}

fn remove_file_new<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    match std::fs::remove_file(&path) {
        Ok(_) => {
            ret.set_int32(0);
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to remove file {path}: {e}");
            ret.set_int32(-1);
        }
    }
}

fn remove_dir_new<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut ret: v8::ReturnValue,
) {
    let path = args.string_lossy(scope, 0);

    match std::fs::remove_dir_all(&path) {
        Ok(_) => {
            ret.set_int32(0);
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to remove directory {path}: {e}");
            ret.set_int32(-1);
        }
    }
}

fn get_error_message(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let state = GLOBAL_STATE.lock().unwrap();
    let error = scope.string(&state.error_message);
    ret.set(error.into());
}

pub fn init_fs<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Object> {
    obj.set_func(scope, "read_file_to_string", read_file_to_string);
    obj.set_func(scope, "write_string_to_file", write_string_to_file);
    obj.set_func(scope, "write_bytes_to_file", write_bytes_to_file);
    obj.set_func(scope, "create_dir", create_dir);
    obj.set_func(scope, "read_dir", read_dir);
    obj.set_func(scope, "is_file", is_file);
    obj.set_func(scope, "is_dir", is_dir);
    obj.set_func(scope, "remove_file", remove_file);
    obj.set_func(scope, "remove_dir", remove_dir);
    obj.set_func(scope, "path_exists", path_exists);
    obj.set_func(scope, "current_dir", current_dir);

    obj.set_func(scope, "read_file_to_bytes_new", read_file_to_bytes_new);
    obj.set_func(scope, "write_bytes_to_file_new", write_bytes_to_file_new);
    obj.set_func(scope, "get_file_content", get_file_content);
    obj.set_func(scope, "get_dir_files", get_dir_files);
    obj.set_func(scope, "get_error_message", get_error_message);
    obj.set_func(scope, "create_dir_new", create_dir_new);
    obj.set_func(scope, "read_dir_new", read_dir_new);
    obj.set_func(scope, "is_file_new", is_file_new);
    obj.set_func(scope, "is_dir_new", is_dir_new);
    obj.set_func(scope, "remove_file_new", remove_file_new);
    obj.set_func(scope, "remove_dir_new", remove_dir_new);

    obj
}
