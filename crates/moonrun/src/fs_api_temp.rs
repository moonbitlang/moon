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
fn read_file_to_string(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    let contents =
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("Failed to read file: {}", path));
    let contents = v8::String::new(scope, &contents).unwrap();
    ret.set(contents.into());
}

fn read_file_to_bytes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    let contents = std::fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file: {}", path));
    let len = contents.len();
    let array_buffer = v8::ArrayBuffer::with_backing_store(
        scope,
        &v8::ArrayBuffer::new_backing_store_from_bytes(contents).make_shared(),
    );

    let uint8_array = v8::Uint8Array::new(scope, array_buffer, 0, len).unwrap();
    ret.set(uint8_array.into());
}

/// `fn write_string_to_file(path: JSString, contents: JSString) -> Unit`
fn write_string_to_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    let contents = args.get(1);
    let contents = contents.to_string(scope).unwrap();
    let contents = contents.to_rust_string_lossy(scope);

    std::fs::write(&path, contents).unwrap_or_else(|_| panic!("Failed to write file: {}", path));

    ret.set_undefined()
}

fn write_bytes_to_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    let contents = args.get(1);

    let uint8_array = v8::Local::<v8::Uint8Array>::try_from(contents).unwrap();
    let length = uint8_array.byte_length();
    let mut buffer = vec![0; length];
    uint8_array.copy_contents(&mut buffer);

    std::fs::write(&path, buffer).unwrap_or_else(|_| panic!("Failed to write file: {}", path));

    ret.set_undefined()
}

fn create_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    std::fs::create_dir_all(&path)
        .unwrap_or_else(|_| panic!("Failed to create directory: {}", path));

    ret.set_undefined()
}

#[allow(clippy::manual_flatten)]
fn read_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    let entries =
        std::fs::read_dir(&path).unwrap_or_else(|_| panic!("Failed to read directory: {}", path));

    let result = v8::Array::new(scope, 0);
    let mut index = 0;

    for entry in entries {
        if let Ok(entry) = entry {
            let rust_style_path = entry.path();
            let node_style_path = rust_style_path.strip_prefix(&path).unwrap();
            if let Some(path_str) = node_style_path.to_str() {
                let js_string = v8::String::new(scope, path_str).unwrap();
                result.set_index(scope, index, js_string.into()).unwrap();
                index += 1;
            }
        }
    }

    ret.set(result.into());
}

fn is_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    let is_file = std::path::Path::new(&path).is_file();
    ret.set_bool(is_file);
}

fn is_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    let is_dir = std::path::Path::new(&path).is_dir();
    ret.set_bool(is_dir);
}

fn remove_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    std::fs::remove_file(&path).unwrap_or_else(|_| panic!("Failed to remove file: {}", path));

    ret.set_undefined();
}

fn remove_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

    std::fs::remove_dir_all(&path)
        .unwrap_or_else(|_| panic!("Failed to remove directory: {}", path));

    ret.set_undefined();
}

fn path_exists(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);

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
    let current_dir = v8::String::new(scope, current_dir).unwrap();
    ret.set(current_dir.into());
}

// new ffi with error handling, use in moonbitlang/core

use once_cell::sync::Lazy;
use std::sync::Mutex;

static GLOBAL_STATE: Lazy<Mutex<GlobalState>> = Lazy::new(|| {
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

fn write_bytes_to_file_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

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
                format!("Failed to write file {}: {}", path, e);
            ret.set_int32(-1);
        }
    }
}

fn read_file_to_bytes_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    match std::fs::read(&path) {
        Ok(contents) => {
            GLOBAL_STATE.lock().unwrap().file_content = contents;
            ret.set_int32(0);
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to read file {}: {}", path, e);
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
        let js_string = v8::String::new(scope, file).unwrap();
        array
            .set_index(scope, index as u32, js_string.into())
            .unwrap();
    }
    ret.set(array.into());
}

fn create_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    match std::fs::create_dir_all(&path) {
        Ok(_) => {
            ret.set_int32(0);
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to create directory {}: {}", path, e);
            ret.set_int32(-1);
        }
    }
}

#[allow(clippy::manual_flatten)]
fn read_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    let entries = match std::fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to read directory {}: {}", path, e);
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

fn is_file_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    let is_file = match std::fs::metadata(&path) {
        Ok(metadata) => {
            if metadata.is_file() {
                1
            } else {
                0
            }
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message = format!("{}: {}", e, path);
            -1
        }
    };
    ret.set_int32(is_file);
}

fn is_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    let is_dir = match std::fs::metadata(&path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                1
            } else {
                0
            }
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message = format!("{}: {}", e, path);
            -1
        }
    };
    ret.set_int32(is_dir);
}

fn remove_file_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    match std::fs::remove_file(&path) {
        Ok(_) => {
            ret.set_int32(0);
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to remove file {}: {}", path, e);
            ret.set_int32(-1);
        }
    }
}

fn remove_dir_new(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    match std::fs::remove_dir_all(&path) {
        Ok(_) => {
            ret.set_int32(0);
        }
        Err(e) => {
            GLOBAL_STATE.lock().unwrap().error_message =
                format!("Failed to remove directory {}: {}", path, e);
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
    let error = v8::String::new(scope, &state.error_message).unwrap();
    ret.set(error.into());
}

pub fn init_fs<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Object> {
    let read_file_to_string = v8::FunctionTemplate::new(scope, read_file_to_string);
    let read_file_to_string = read_file_to_string.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "read_file_to_string").unwrap();
    obj.set(scope, ident.into(), read_file_to_string.into());

    let read_file_to_bytes = v8::FunctionTemplate::new(scope, read_file_to_bytes);
    let read_file_to_bytes = read_file_to_bytes.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "read_file_to_bytes").unwrap();
    obj.set(scope, ident.into(), read_file_to_bytes.into());

    let write_string_to_file = v8::FunctionTemplate::new(scope, write_string_to_file);
    let write_string_to_file = write_string_to_file.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "write_string_to_file").unwrap();
    obj.set(scope, ident.into(), write_string_to_file.into());

    let write_bytes_to_file = v8::FunctionTemplate::new(scope, write_bytes_to_file);
    let write_bytes_to_file = write_bytes_to_file.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "write_bytes_to_file").unwrap();
    obj.set(scope, ident.into(), write_bytes_to_file.into());

    let create_dir = v8::FunctionTemplate::new(scope, create_dir);
    let create_directory = create_dir.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "create_dir").unwrap();
    obj.set(scope, ident.into(), create_directory.into());

    let read_dir = v8::FunctionTemplate::new(scope, read_dir);
    let read_directory = read_dir.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "read_dir").unwrap();
    obj.set(scope, ident.into(), read_directory.into());

    let is_file = v8::FunctionTemplate::new(scope, is_file);
    let is_file = is_file.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "is_file").unwrap();
    obj.set(scope, ident.into(), is_file.into());

    let is_dir = v8::FunctionTemplate::new(scope, is_dir);
    let is_dir = is_dir.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "is_dir").unwrap();
    obj.set(scope, ident.into(), is_dir.into());

    let remove_file = v8::FunctionTemplate::new(scope, remove_file);
    let remove_file = remove_file.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "remove_file").unwrap();
    obj.set(scope, ident.into(), remove_file.into());

    let remove_dir = v8::FunctionTemplate::new(scope, remove_dir);
    let remove_dir = remove_dir.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "remove_dir").unwrap();
    obj.set(scope, ident.into(), remove_dir.into());

    let path_exists = v8::FunctionTemplate::new(scope, path_exists);
    let path_exists = path_exists.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "path_exists").unwrap();
    obj.set(scope, ident.into(), path_exists.into());

    let current_dir = v8::FunctionTemplate::new(scope, current_dir);
    let current_dir = current_dir.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "current_dir").unwrap();
    obj.set(scope, ident.into(), current_dir.into());

    {
        let read_file_to_bytes_new = v8::FunctionTemplate::new(scope, read_file_to_bytes_new);
        let read_file_to_bytes_new = read_file_to_bytes_new.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "read_file_to_bytes_new").unwrap();
        obj.set(scope, ident.into(), read_file_to_bytes_new.into());

        let write_bytes_to_file_new = v8::FunctionTemplate::new(scope, write_bytes_to_file_new);
        let write_bytes_to_file_new = write_bytes_to_file_new.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "write_bytes_to_file_new").unwrap();
        obj.set(scope, ident.into(), write_bytes_to_file_new.into());

        let get_file_content = v8::FunctionTemplate::new(scope, get_file_content);
        let get_file_content = get_file_content.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "get_file_content").unwrap();
        obj.set(scope, ident.into(), get_file_content.into());

        let get_dir_files = v8::FunctionTemplate::new(scope, get_dir_files);
        let get_dir_files = get_dir_files.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "get_dir_files").unwrap();
        obj.set(scope, ident.into(), get_dir_files.into());

        let get_error_message = v8::FunctionTemplate::new(scope, get_error_message);
        let get_error_message = get_error_message.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "get_error_message").unwrap();
        obj.set(scope, ident.into(), get_error_message.into());

        let create_dir_new = v8::FunctionTemplate::new(scope, create_dir_new);
        let create_dir_new = create_dir_new.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "create_dir_new").unwrap();
        obj.set(scope, ident.into(), create_dir_new.into());

        let read_dir_new = v8::FunctionTemplate::new(scope, read_dir_new);
        let read_dir_new = read_dir_new.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "read_dir_new").unwrap();
        obj.set(scope, ident.into(), read_dir_new.into());

        let is_file_new = v8::FunctionTemplate::new(scope, is_file_new);
        let is_file_new = is_file_new.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "is_file_new").unwrap();
        obj.set(scope, ident.into(), is_file_new.into());

        let is_dir_new = v8::FunctionTemplate::new(scope, is_dir_new);
        let is_dir_new = is_dir_new.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "is_dir_new").unwrap();
        obj.set(scope, ident.into(), is_dir_new.into());

        let remove_file_new = v8::FunctionTemplate::new(scope, remove_file_new);
        let remove_file_new = remove_file_new.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "remove_file_new").unwrap();
        obj.set(scope, ident.into(), remove_file_new.into());

        let remove_dir_new = v8::FunctionTemplate::new(scope, remove_dir_new);
        let remove_dir_new = remove_dir_new.get_function(scope).unwrap();
        let ident = v8::String::new(scope, "remove_dir_new").unwrap();
        obj.set(scope, ident.into(), remove_dir_new.into());
    }

    obj
}
