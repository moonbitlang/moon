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

use std::{collections::HashMap, fs::{self, metadata, File, OpenOptions, Permissions}, os::unix::fs::{FileExt, OpenOptionsExt, PermissionsExt}, path::Path, process::{Command, Stdio}};

use v8::Handle;

// getenv : JSString -> JSString
fn getenv(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let var = args.get(0);
    let var = var.to_string(scope).unwrap();
    let var = var.to_rust_string_lossy(scope);
    match std::env::var(var) {
        Ok(val) => {
            let val = v8::String::new(scope, &val).unwrap();
            ret.set(val.into())
        }
        Err(_) => {
            let val = v8::undefined(scope);
            ret.set(val.into())
        }
    }
}

#[cfg(target_os = "windows")]
const on_windows : &str = "const on_windows = true";

#[cfg(not(target_os = "windows"))]
const on_windows : &str = "const on_windows = false";


fn make_shell() -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c");
    cmd
}

// system : JSSTring -> Number
fn system(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let mut shell = make_shell();
    let command = args.get(0);
    let command = command.to_string(scope).unwrap();
    let command = command.to_rust_string_lossy(scope);
    let command = shell.arg(&command);
    let command = 
      command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match command.spawn() {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(mut child) => {
            match child.wait() {
                Err(err) => {
                    let message = v8::String::new(scope, &err.to_string()).unwrap();
                    let exn = v8::Exception::error(scope, message);
                    scope.throw_exception(exn);
                }
                Ok(status) => {
                    let retcode = status.code().unwrap_or(255);
                    let retcode = v8::Number::new(scope, retcode.into());
                    ret.set(retcode.into())
                }
            }
        }
    }
}

// log : JSstring -> undefined
fn console_log(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let msg = args.get(0);
    let msg = msg.to_string(scope).unwrap();
    let msg = msg.to_rust_string_lossy(scope);
    println!("{}", &msg);
    let undefined = v8::undefined(scope);
    ret.set(undefined.into())
}

// is_file : JJString -> Number(1 | 0)
fn is_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let retcode =
      if path.exists() && path.is_file() {
        1
      } else {
        0
      };
    let retcode = v8::Number::new(scope, retcode.into());
    ret.set(retcode.into())
}

// is_directory : JJString -> Number(1 | 0)
fn is_directory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let retcode =
      if path.exists() && path.is_dir() {
        1
      } else {
        0
      };
    let retcode = v8::Number::new(scope, retcode.into());
    ret.set(retcode.into())
}

// file_exists : JJString -> Number(1 | 0)
// actually path_exists
fn file_exists(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let retcode =
      if path.exists() {
        1
      } else {
        0
      };
    let retcode = v8::Number::new(scope, retcode.into());
    ret.set(retcode.into())
}

// chmod : JSString, PermissionMode -> undefined
// only support unix
fn chmod(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let mode = args.get(1);
    let mode = mode.to_number(scope).unwrap().value() as u32;
    let permission = Permissions::from_mode(mode);
    match fs::set_permissions(path, permission) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(_) => {
            let undefined = v8::undefined(scope);
            ret.set(undefined.into())
        }
    }
}

// truncate: JSString, Length -> undefined
fn truncate(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let file = OpenOptions::new().read(true).write(true).open(path);
    match file {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(file) => {
            let len = args.get(1);
            let len = len.to_number(scope).unwrap().value() as u64;
            match file.set_len(len) {
                Err(err) => {
                    let message = v8::String::new(scope, &err.to_string()).unwrap();
                    let exn = v8::Exception::error(scope, message);
                    scope.throw_exception(exn);
                }
                Ok(_) => {
                    let undefined = v8::undefined(scope);
                    ret.set(undefined.into())
                }
            }
        }
    }
}

// File Descripter Table
// The file descriptor representation uses i32, keep consistent with the wasm_of_ocaml runtime
struct FdTable {
    map : HashMap<i32, File>,
    next_fd : i32
}

impl FdTable {
    fn add(&mut self, file : File) -> i32 {
        let fd = self.next_fd;
        let removed = self.map.insert(fd, file);
        assert!(removed.is_none());
        self.next_fd += 1;
        fd
    }
    fn get(&self, fd : i32) -> Result<&File, String> {
        match self.map.get(&fd) {
            None => {
                Err(format!("invalid file descripter: {}", fd))
            }
            Some(fileref) => {
                Ok(fileref)
            }
        }
    }
    fn remove(&mut self, fd : i32) -> Option<File> {
        self.map.remove(&fd)
    }
}

// wasm_of_ocaml compile Unix.(stdin, stdout, stderr) to constants (0, 1, 2)
const STDIN : i32 = 0;
const STDOUT : i32 = 1;
const STDERR : i32 = 2;

// open flags for wasm_of_ocaml
const O_RDONLY: i32 = 1;
const O_WRONLY: i32 = 2;
const O_RDWR: i32 = 4;
const O_APPEND: i32 = 8;
const O_CREAT: i32 = 16;
const O_TRUNC: i32 = 32;
const O_EXCL: i32 = 64;
const O_NONBLOCK: i32 = 128;
const O_NOCTTY: i32 = 256;
const O_DSYNC: i32 = 512;
const O_SYNC: i32 = 1024;

// open : JSString as Path, i32 as Flags, Number as PermissionMode -> FileDescripter
fn open(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let flags = args.get(1);
    let flags = flags.to_number(scope).unwrap().value() as i32;
    let mode = args.get(2);
    let mode = mode.to_number(scope).unwrap().value() as i32;

    let access_mode = flags & (O_RDONLY | O_WRONLY | O_RDWR);
    let (read, write) = match access_mode {
        O_RDONLY => (true, false),
        O_WRONLY => (false, true),
        O_RDWR => (true, true),
        _ => {
            let err_msg = "Invalid Flags: Must specify O_RDONLY, O_WRONLY or O_RDWR";
            let message = v8::String::new(scope, err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return ()
        }
    };

    let mut opts = OpenOptions::new();
    opts.read(read)
        .write(write)
        .append((flags & O_APPEND) != 0)
        .truncate((flags & O_TRUNC) != 0);

    let has_creat = (flags & O_CREAT) != 0;
    let has_excl = (flags & O_EXCL) != 0;
    if has_creat && has_excl {
        opts.create_new(true);
    } else if has_creat {
        opts.create(true);
    }
    let mut custom_flags = 0;
    if (flags & O_NONBLOCK) != 0 {
        custom_flags |= libc::O_NONBLOCK;
    }
    if (flags & O_NOCTTY) != 0 {
        custom_flags |= libc::O_NOCTTY;
    }
    if (flags & O_DSYNC) != 0 {
        custom_flags |= libc::O_DSYNC;
    }
    if (flags & O_SYNC) != 0 {
        custom_flags |= libc::O_SYNC;
    }
    opts.custom_flags(custom_flags);
    opts.mode((mode & 0o777) as u32); // assure permission is legal
    match opts.open(path) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(file) => {
            let context  = scope.get_current_context();
            let fd_table = context.get_slot_mut::<FdTable>().unwrap();
            let fd = fd_table.add(file) as f64;
            let fd = v8::Number::new(scope, fd);
            ret.set(fd.into())
        }
    }
}

fn close(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let context  = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    match fd_table.remove(fd) {
        None => {
            let err_msg = &format!("invalid file descripter {}", fd);
            let message = v8::String::new(scope, err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Some(file) => {
            match file.sync_all() {
                Ok(_) => {
                    let undefined = v8::undefined(scope);
                    ret.set(undefined.into())
                }
                Err(err) => {
                    let message = v8::String::new(scope, &err.to_string()).unwrap();
                    let exn = v8::Exception::error(scope, message);
                    scope.throw_exception(exn);
                }
            }

        }
    }
}

// access flags for wasm_of_ocaml
const R_OK : i32 = 8;
const W_OK : i32 = 4;
const X_OK : i32 = 2;
const F_OK : i32 = 1;

fn access(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let mode = args.get(1);
    let mode = mode.to_number(scope).unwrap().value() as i32;
    if mode & F_OK != 0 {
        if let Err(err) = metadata(path) {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return
        }
    }

    if mode & R_OK != 0 {
        if let Err(err) = File::open(path) {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return
        }
    }

    if mode & W_OK != 0 {
        if let Err(err) = OpenOptions::new().write(true).open(path) {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return
        }
    }

    if mode & X_OK != 0 {
        match metadata(path) {
            Err(err) => {
                let message = v8::String::new(scope, &err.to_string()).unwrap();
                let exn = v8::Exception::error(scope, message);
                scope.throw_exception(exn);
                return
            }
            Ok(metadata) => {
                let mode = metadata.permissions().mode();
                if mode & 0o111 == 0 {
                    let message = v8::String::new(scope, "execute permission denied").unwrap();
                    let exn = v8::Exception::error(scope, message);
                    scope.throw_exception(exn);
                    return
                }
            }
        }
    }

    let undefined = v8::undefined(scope);
    ret.set(undefined.into())
}

// i32 as FileDescripter, UInt8Array as Buffer, i32 as Offset, i32 as Length, i32 | null as Position -> Number
fn write(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let buf = args.get(1);
    let buf = buf.try_cast::<v8::Uint8Array>().unwrap();
    let offset = args.get(2);
    let offset = offset.to_number(scope).unwrap().value() as usize;
    let length = args.get(3);
    let length = length.to_number(scope).unwrap().value() as usize;
    let pos = args.get(4);
    let pos = pos.to_number(scope).unwrap().value() as u64;
    let context  = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get(fd) {
        Ok(file) => {
            file
        }
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return
        }
    };
    let buf_length = buf.byte_length();
    let mut bytes = vec![0u8; buf_length];
    let copyed = buf.copy_contents(&mut bytes);
    assert!(copyed == buf_length);
    match file.write_at(&bytes[offset..offset + length], pos) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(n) => {
            let n = v8::Number::new(scope, n as f64);
            ret.set(n.into())
        }
    }
}

fn read(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let buf = args.get(1);
    let buf = buf.try_cast::<v8::Uint8Array>().unwrap();
    let offset = args.get(2);
    let offset = offset.to_number(scope).unwrap().value() as usize;
    let length = args.get(3);
    let length = length.to_number(scope).unwrap().value() as usize;
    let pos = args.get(4);
    let pos = pos.to_number(scope).unwrap().value() as u64;
    let context  = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get(fd) {
        Ok(file) => {
            file
        }
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return
        }
    };
    let buf_length = buf.byte_length();
    let raw_data = buf.data();
    let bytes : &mut [u8] = unsafe {
        std::slice::from_raw_parts_mut(raw_data as *mut u8, buf_length)
    };
    match file.read_at(&mut bytes[offset..offset + length], pos) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(n) => {
            let n = v8::Number::new(scope, n as f64);
            ret.set(n.into())
        }
    }
}

fn fsync(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let context  = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get(fd) {
        Ok(file) => {
            file
        }
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return
        }
    };
    match file.sync_all() {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(_) => {
            let undefined = v8::undefined(scope);
            ret.set(undefined.into())
        }
    }
}

// File Descripter -> BigInt
fn file_size(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let context  = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get(fd) {
        Ok(file) => {
            file
        }
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return
        }
    };
    let metadata = match file.metadata() {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return
        }
        Ok(metadata) => metadata
    };
    let size = v8::BigInt::new_from_u64(scope, metadata.len());
    ret.set(size.into());
}



fn init_wasmoo<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Object> {
    let getenv = v8::FunctionTemplate::new(scope, getenv);
    let getenv = getenv.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "getenv").unwrap();
    obj.set(scope, ident.into(), getenv.into());

    let on_windows_ = v8::String::new(scope, on_windows).unwrap();
    let ident = v8::String::new(scope, "on_windows").unwrap();
    obj.set(scope, ident.into(), on_windows_.into());

    let system = v8::FunctionTemplate::new(scope, system);
    let system = system.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "system").unwrap();
    obj.set(scope, ident.into(), system.into());

    let is_file = v8::FunctionTemplate::new(scope, is_file);
    let is_file = is_file.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "is_file").unwrap();
    obj.set(scope, ident.into(), is_file.into());

    let is_directory = v8::FunctionTemplate::new(scope, is_directory);
    let is_directory = is_directory.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "is_directory").unwrap();
    obj.set(scope, ident.into(), is_directory.into());

    let file_exists = v8::FunctionTemplate::new(scope, file_exists);
    let file_exists = file_exists.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "file_exists").unwrap();
    obj.set(scope, ident.into(), file_exists.into());

    let console_log = v8::FunctionTemplate::new(scope, console_log);
    let console_log = console_log.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "console_log").unwrap();
    obj.set(scope, ident.into(), console_log.into());

    let chmod = v8::FunctionTemplate::new(scope, chmod);
    let chmod = chmod.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "chmod").unwrap();
    obj.set(scope, ident.into(), chmod.into());

    let truncate = v8::FunctionTemplate::new(scope, truncate);
    let truncate = truncate.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "truncate").unwrap();
    obj.set(scope, ident.into(), truncate.into());

    let open = v8::FunctionTemplate::new(scope, open);
    let open = open.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "open").unwrap();
    obj.set(scope, ident.into(), open.into());

    let close = v8::FunctionTemplate::new(scope, close);
    let close = close.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "close").unwrap();
    obj.set(scope, ident.into(), close.into());

    let access = v8::FunctionTemplate::new(scope, access);
    let access = access.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "access").unwrap();
    obj.set(scope, ident.into(), access.into());

    let write = v8::FunctionTemplate::new(scope, write);
    let write = write.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "write").unwrap();
    obj.set(scope, ident.into(), write.into());

    let read = v8::FunctionTemplate::new(scope, read);
    let read = read.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "read").unwrap();
    obj.set(scope, ident.into(), read.into());

    let fsync = v8::FunctionTemplate::new(scope, fsync);
    let fsync = fsync.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "fsync").unwrap();
    obj.set(scope, ident.into(), fsync.into());

    let file_size = v8::FunctionTemplate::new(scope, file_size);
    let file_size = file_size.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "file_size").unwrap();
    obj.set(scope, ident.into(), file_size.into());

    obj
}

fn test_wasmoo_extern(script : &str) -> String {
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    let mut isolate = v8::Isolate::new(Default::default());
    // setup file descripter table
    isolate.set_slot(FdTable { map : HashMap::new(), next_fd : 3 });
    let isolate = &mut isolate;
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);
    let global_proxy = scope.get_current_context().global(scope);
    init_wasmoo(global_proxy, scope);
    let code = v8::String::new(scope, script).unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    match script.run(scope) {
        None => {
            String::from("ffffailed!")
        }
        Some(val) => {
            val.to_string(scope).unwrap().to_rust_string_lossy(scope)
        }
    }
    
}

#[cfg(test)]
mod test_wasmoo_extern {
    use super::*;

    #[test]
    fn test_getenv() {
        println!("{}", test_wasmoo_extern("getenv('PATH')"))
    }

    #[test]
    fn test_system() {
        println!("{}",test_wasmoo_extern("system('cat Cargo.toml')"))
    }

    #[test]
    fn test_is_file() {
        println!("{}", test_wasmoo_extern("is_file('README.md')"))
    }

    #[test]
    fn test_console_log() {
        assert_eq!("undefined", test_wasmoo_extern("console_log('foo')"))
    }

    #[test]
    fn test_is_directory() {
        println!("{}", test_wasmoo_extern("is_directory('src')"))
    }

    #[test]
    fn test_file_exists() {
        println!("{}", test_wasmoo_extern("file_exists('src')"));
    }
    
}