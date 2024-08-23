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

    let contents = std::fs::read_to_string(path).expect("Failed to read file");
    let contents = v8::String::new(scope, &contents).unwrap();
    ret.set(contents.into());
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

    std::fs::write(path, contents).expect("Failed to write file");

    ret.set_undefined()
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

pub fn init_fs<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Object> {
    let read_file_to_string = v8::FunctionTemplate::new(scope, read_file_to_string);
    let read_file_to_string = read_file_to_string.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "read_file_to_string").unwrap();
    obj.set(scope, ident.into(), read_file_to_string.into());

    let write_string_to_file = v8::FunctionTemplate::new(scope, write_string_to_file);
    let write_string_to_file = write_string_to_file.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "write_string_to_file").unwrap();
    obj.set(scope, ident.into(), write_string_to_file.into());

    let path_exists = v8::FunctionTemplate::new(scope, path_exists);
    let path_exists = path_exists.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "path_exists").unwrap();
    obj.set(scope, ident.into(), path_exists.into());

    obj
}
