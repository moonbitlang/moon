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

use std::{
    collections::HashMap,
    ffi::CString,
    fs::{self, metadata, File, OpenOptions, Permissions},
    io::{IsTerminal, Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
    process::{Command, Stdio},
};

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

fn make_shell() -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c");
    cmd
}

// system : JSString -> Number
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
    let command = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match command.spawn() {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(mut child) => match child.wait() {
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
        },
    }
}

// log : JSString -> undefined
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

// is_file : JSString -> Number(1 | 0)
fn is_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let retcode = if path.exists() && path.is_file() {
        1
    } else {
        0
    };
    let retcode = v8::Number::new(scope, retcode.into());
    ret.set(retcode.into())
}

// is_directory : JSString -> Number(1 | 0)
fn is_directory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let retcode = if path.exists() && path.is_dir() { 1 } else { 0 };
    let retcode = v8::Number::new(scope, retcode.into());
    ret.set(retcode.into())
}

// file_exists : JSString -> Number(1 | 0)
// actually mains path_exists
fn file_exists(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let retcode = if path.exists() { 1 } else { 0 };
    let retcode = v8::Number::new(scope, retcode.into());
    ret.set(retcode.into())
}

// chmod : JSString, PermissionMode -> undefined
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

// truncate: JSString, u64 as Length -> undefined
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

// File Descriptor Table
// The file descriptor representation uses i32, keep consistent with the wasm_of_ocaml runtime
pub struct FdTable {
    map: HashMap<i32, File>,
    next_fd: i32,
}

impl FdTable {
    pub fn new() -> FdTable {
        FdTable {
            map: HashMap::new(),
            next_fd: 3,
        }
    }
    fn add(&mut self, file: File) -> i32 {
        let fd = self.next_fd;
        let removed = self.map.insert(fd, file);
        assert!(removed.is_none());
        self.next_fd += 1;
        fd
    }
    fn get(&self, fd: i32) -> Result<&File, String> {
        match self.map.get(&fd) {
            None => Err(format!("invalid file descriptor: {}", fd)),
            Some(fileref) => Ok(fileref),
        }
    }

    fn get_mut(&mut self, fd: i32) -> Result<&mut File, String> {
        match self.map.get_mut(&fd) {
            None => Err(format!("invalid file descriptor: {}", fd)),
            Some(fileref) => Ok(fileref),
        }
    }

    fn remove(&mut self, fd: i32) -> Option<File> {
        self.map.remove(&fd)
    }
}

// wasm_of_ocaml compile Unix.(stdin, stdout, stderr) to constants (0, 1, 2)
const STDIN: i32 = 0;
const STDOUT: i32 = 1;
const STDERR: i32 = 2;

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

// open : JSString as Path, Number as Flags, Number as PermissionMode -> FileDescriptor
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
            return;
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
            let context = scope.get_current_context();
            let fd_table = context.get_slot_mut::<FdTable>().unwrap();
            let fd = fd_table.add(file) as f64;
            let fd = v8::Number::new(scope, fd);
            ret.set(fd.into())
        }
    }
}

// close : FileDescriptor -> undefined
fn close(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let context = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    match fd_table.remove(fd) {
        None => {
            let err_msg = &format!("invalid file descriptor {}", fd);
            let message = v8::String::new(scope, err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Some(file) => match file.sync_all() {
            Ok(_) => {
                let undefined = v8::undefined(scope);
                ret.set(undefined.into())
            }
            Err(err) => {
                let message = v8::String::new(scope, &err.to_string()).unwrap();
                let exn = v8::Exception::error(scope, message);
                scope.throw_exception(exn);
            }
        },
    }
}

// access flags for wasm_of_ocaml
const R_OK: i32 = 8;
const W_OK: i32 = 4;
const X_OK: i32 = 2;
const F_OK: i32 = 1;

// access : JSString as Path, i32 as Mode -> undefined
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
            return;
        }
    }

    if mode & R_OK != 0 {
        if let Err(err) = File::open(path) {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
    }

    if mode & W_OK != 0 {
        if let Err(err) = OpenOptions::new().write(true).open(path) {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
    }

    if mode & X_OK != 0 {
        match metadata(path) {
            Err(err) => {
                let message = v8::String::new(scope, &err.to_string()).unwrap();
                let exn = v8::Exception::error(scope, message);
                scope.throw_exception(exn);
                return;
            }
            Ok(metadata) => {
                let mode = metadata.permissions().mode();
                if mode & 0o111 == 0 {
                    let message = v8::String::new(scope, "execute permission denied").unwrap();
                    let exn = v8::Exception::error(scope, message);
                    scope.throw_exception(exn);
                    return;
                }
            }
        }
    }

    let undefined = v8::undefined(scope);
    ret.set(undefined.into())
}

// write: i32 as FileDescriptor, UInt8Array as Buffer, i32 as Offset, i32 as Length, null as Position -> Number
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
    debug_assert!(pos.is_null());
    let buf_length = buf.byte_length();
    let raw_data = buf.data();
    let bytes: &[u8] = unsafe { std::slice::from_raw_parts(raw_data as *mut u8, buf_length) };

    if fd == STDOUT || fd == STDERR {
        if fd == STDOUT {
            match std::io::stdout().write_all(&bytes[offset..offset + length]) {
                Err(err) => {
                    let message = v8::String::new(scope, &err.to_string()).unwrap();
                    let exn = v8::Exception::error(scope, message);
                    scope.throw_exception(exn);
                }
                Ok(_) => {
                    let n = v8::Number::new(scope, length as f64);
                    ret.set(n.into())
                }
            };
            return;
        } else {
            // fd == STDERR
            match std::io::stderr().write_all(&bytes[offset..offset + length]) {
                Err(err) => {
                    let message = v8::String::new(scope, &err.to_string()).unwrap();
                    let exn = v8::Exception::error(scope, message);
                    scope.throw_exception(exn);
                }
                Ok(_) => {
                    let n = v8::Number::new(scope, length as f64);
                    ret.set(n.into())
                }
            };
            return;
        }
    }
    let context = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get_mut(fd) {
        Ok(file) => file,
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
    };
    match file.write(&bytes[offset..offset + length]) {
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

// read: i32 as FileDescriptor, UInt8Array as Buffer, i32 as Offset, i32 as Length, null as Position -> Number
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
    debug_assert!(pos.is_null());
    let context = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get_mut(fd) {
        Ok(fileref) => fileref,
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
    };
    let buf_length = buf.byte_length();
    let raw_data = buf.data();
    let bytes: &mut [u8] =
        unsafe { std::slice::from_raw_parts_mut(raw_data as *mut u8, buf_length) };
    match file.read(&mut bytes[offset..offset + length]) {
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

// fsync: i32 as FileDescriptor -> undefined
fn fsync(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let context = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get(fd) {
        Ok(file) => file,
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
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

// file_size: i32 as FileDescriptor -> BigInt
fn file_size(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let context = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get(fd) {
        Ok(file) => file,
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
    };
    let metadata = match file.metadata() {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
        Ok(metadata) => metadata,
    };
    let size = v8::BigInt::new_from_u64(scope, metadata.len());
    ret.set(size.into());
}

fn timeval_from_f64(t: f64) -> std::io::Result<libc::timeval> {
    if !t.is_finite() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Time value must be finite",
        ));
    }

    let total_usec = (t * 1_000_000.0).round() as i64;

    let sec = total_usec.div_euclid(1_000_000);
    let usec = total_usec.rem_euclid(1_000_000);

    Ok(libc::timeval {
        tv_sec: sec as libc::time_t,
        tv_usec: usec as libc::suseconds_t,
    })
}

fn __utimes(path: String, atime: f64, mtime: f64) -> std::io::Result<()> {
    let c_path = CString::new(path)?;

    let atime_tv = timeval_from_f64(atime)?;
    let mtime_tv = timeval_from_f64(mtime)?;

    let times = [atime_tv, mtime_tv];

    let result = unsafe { libc::utimes(c_path.as_ptr(), times.as_ptr()) };

    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

// utimes: JSString as Path, F64 as AccessTime, F64 as ModifyTime -> undefined
fn utimes(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let atime = args.get(1).to_number(scope).unwrap().value();
    let mtime = args.get(2).to_number(scope).unwrap().value();

    match __utimes(path, atime, mtime) {
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

// exit: i32 -> undefined
fn exit(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, _ret: v8::ReturnValue) {
    let code = args.get(0).to_int32(scope).unwrap();
    std::process::exit(code.value());
}

// isatty: FileDescriptor -> Number(1 | 0)
fn isatty(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let rescode = if fd == STDIN {
        if std::io::stdin().is_terminal() {
            1
        } else {
            0
        }
    } else if fd == STDOUT {
        if std::io::stdout().is_terminal() {
            1
        } else {
            0
        }
    } else if fd == STDERR {
        if std::io::stderr().is_terminal() {
            1
        } else {
            0
        }
    } else {
        let context = scope.get_current_context();
        let fd_table = context.get_slot_mut::<FdTable>().unwrap();
        match fd_table.get(fd) {
            Ok(file) => unsafe { libc::isatty(file.as_raw_fd()) },
            Err(_) => 0,
        }
    };

    let rescode = v8::Number::new(scope, rescode as f64);
    ret.set(rescode.into());
}

// getcwd: () -> JSString
fn getcwd(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    match std::env::current_dir() {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(pathbuf) => {
            let path = pathbuf.display().to_string();
            let path = v8::String::new(scope, &path).unwrap();
            ret.set(path.into())
        }
    }
}

// chdir: JSString -> undefined
fn chdir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    match std::env::set_current_dir(path) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(_) => {
            let undefined = v8::undefined(scope);
            ret.set(undefined.into());
        }
    }
}

// mkdir: JSString as Path, i32 as Mode -> undefined
fn mkdir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let mode = args.get(1);
    let mode = mode.to_number(scope).unwrap().value() as i32;
    let path = Path::new(&path);
    if let Err(err) = fs::create_dir(path) {
        let message = v8::String::new(scope, &err.to_string()).unwrap();
        let exn = v8::Exception::error(scope, message);
        scope.throw_exception(exn);
        return;
    }
    let permissions = fs::Permissions::from_mode(mode as u32);
    match fs::set_permissions(path, permissions) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(_) => {
            let undefined = v8::undefined(scope);
            ret.set(undefined.into());
        }
    }
}

// rmdir: JSString as Path -> undefined
fn rmdir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    match fs::remove_dir(path) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(_) => {
            let undefined = v8::undefined(scope);
            ret.set(undefined.into());
        }
    }
}

// JSString as Path, JSString as NewPath -> undefined
// fn link(
//     scope: &mut v8::HandleScope,
//     args: v8::FunctionCallbackArguments,
//     mut ret: v8::ReturnValue,
// ) {
//     let path = args.get(0);
//     let path = path.to_string(scope).unwrap();
//     let path = path.to_rust_string_lossy(scope);
//     let path = Path::new(&path);
//     let newpath = args.get(1);
//     let newpath = newpath.to_string(scope).unwrap();
//     let newpath = newpath.to_rust_string_lossy(scope);
//     let newpath = Path::new(&newpath);
//     match fs::hard_link(path, newpath) {
//         Err(err) => {
//             let message = v8::String::new(scope, &err.to_string()).unwrap();
//             let exn = v8::Exception::error(scope, message);
//             scope.throw_exception(exn);
//         }
//         Ok(_) => {
//             let undefined = v8::undefined(scope);
//             ret.set(undefined.into());
//         }
//     }
// }

// JSString as Path, JSString as Path, Number(0 | 1 | 2) as Kind
// Kind: 0 -> "null", 1 -> "file", 2 -> "dir"
// fn symlink(
//     scope: &mut v8::HandleScope,
//     args: v8::FunctionCallbackArguments,
//     mut ret: v8::ReturnValue,
// ) {
//     let path = args.get(0);
//     let path = path.to_string(scope).unwrap();
//     let path = path.to_rust_string_lossy(scope);
//     let path = Path::new(&path);
//     let newpath = args.get(1);
//     let newpath = newpath.to_string(scope).unwrap();
//     let newpath = newpath.to_rust_string_lossy(scope);
//     let newpath = Path::new(&newpath);
//     match unix::fs::symlink(path, newpath) {
//         Err(err) => {
//             let message = v8::String::new(scope, &err.to_string()).unwrap();
//             let exn = v8::Exception::error(scope, message);
//             scope.throw_exception(exn);
//         }
//         Ok(_) => {
//             let undefined = v8::undefined(scope);
//             ret.set(undefined.into());
//         }
//     }
// }

// JSString as Path -> JSString as Path
// fn readlink(
//     scope: &mut v8::HandleScope,
//     args: v8::FunctionCallbackArguments,
//     mut ret: v8::ReturnValue,
// ) {
//     let path = args.get(0);
//     let path = path.to_string(scope).unwrap();
//     let path = path.to_rust_string_lossy(scope);
//     let path = Path::new(&path);
//     match fs::read_link(path) {
//         Err(err) => {
//             let message = v8::String::new(scope, &err.to_string()).unwrap();
//             let exn = v8::Exception::error(scope, message);
//             scope.throw_exception(exn);
//         }
//         Ok(path) => {
//             let path = path.display().to_string();
//             let path = v8::String::new(scope, &path).unwrap();
//             ret.set(path.into());
//         }
//     }
// }

// unlink: JSString as Path -> undefined
fn unlink(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    match fs::remove_file(path) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(_) => {
            let undefined = v8::undefined(scope);
            ret.set(undefined.into());
        }
    }
}

// readdir: JSString as Path -> Array<String>
fn read_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    match fs::read_dir(path) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(entries) => {
            let mut names = Vec::new();
            for entry in entries {
                match entry {
                    Err(err) => {
                        let message = v8::String::new(scope, &err.to_string()).unwrap();
                        let exn = v8::Exception::error(scope, message);
                        scope.throw_exception(exn);
                        return;
                    }
                    Ok(dir_entry) => match dir_entry.file_name().into_string() {
                        Err(_) => {
                            let message = v8::String::new(
                                scope,
                                &format!(
                                    "read_dir failed to read one item under {}",
                                    path.to_str().unwrap()
                                ),
                            )
                            .unwrap();
                            let exn = v8::Exception::error(scope, message);
                            scope.throw_exception(exn);
                            return;
                        }
                        Ok(name) => {
                            names.push(name);
                        }
                    },
                }
            }
            let strarray = v8::Array::new(scope, names.len() as i32);
            for (i, name) in names.iter().enumerate() {
                let name = v8::String::new(scope, name).unwrap();
                strarray.set_index(scope, i as u32, name.into());
            }
            ret.set(strarray.into())
        }
    }
}

// pub struct DirTable {
//     map: HashMap<i32, ReadDir>,
//     next_d: i32,
// }
//
// impl DirTable {
//     pub fn new() -> DirTable {
//         DirTable { map: HashMap::new(), next_d: 0 }
//     }
//     fn add(&mut self, rd: ReadDir) -> i32 {
//         let d = self.next_d;
//         let removed = self.map.insert(d, rd);
//         assert!(removed.is_none());
//         self.next_d += 1;
//         d
//     }
//     fn get(&mut self, d: i32) -> Result<&mut ReadDir, String> {
//         match self.map.get_mut(&d) {
//             None => Err(format!("invalid dir object: {}", d)),
//             Some(dirref) => Ok(dirref),
//         }
//     }
//     fn remove(&mut self, d: i32) -> Option<ReadDir> {
//         self.map.remove(&d)
//     }
// }
//
// // JSString -> Dir
// fn opendir(
//     scope: &mut v8::HandleScope,
//     args: v8::FunctionCallbackArguments,
//     mut ret: v8::ReturnValue,
// ) {
//     let path = args.get(0);
//     let path = path.to_string(scope).unwrap();
//     let path = path.to_rust_string_lossy(scope);
//     let path = Path::new(&path);
//     let context = scope.get_current_context();
//     let dir_table = context.get_slot_mut::<DirTable>().unwrap();
//     match fs::read_dir(path) {
//         Err(err) => {
//             let message = v8::String::new(scope, &err.to_string()).unwrap();
//             let exn = v8::Exception::error(scope, message);
//             scope.throw_exception(exn);
//         }
//         Ok(readdir) => {
//             let d = dir_table.add(readdir);
//             let d = v8::Number::new(scope, d as f64);
//             ret.set(d.into())
//         }
//     }
// }
//
// fn readdir(
//     scope: &mut v8::HandleScope,
//     args: v8::FunctionCallbackArguments,
//     mut ret: v8::ReturnValue,
// ) {
//     let d = args.get(0);
//     let d = d.to_number(scope).unwrap().value() as i32;
//     let context = scope.get_current_context();
//     let dir_table = context.get_slot_mut::<DirTable>().unwrap();
//     match dir_table.get(d) {
//         Err(err_msg) => {
//             let message = v8::String::new(scope, &err_msg).unwrap();
//             let exn = v8::Exception::error(scope, message);
//             scope.throw_exception(exn);
//         }
//         Ok(readdir) => match readdir.next() {
//             None => {
//                 let null = v8::null(scope);
//                 ret.set(null.into());
//             }
//             Some(result) => match result {
//                 Err(err) => {
//                     let message = v8::String::new(scope, &err.to_string()).unwrap();
//                     let exn = v8::Exception::error(scope, message);
//                     scope.throw_exception(exn);
//                 }
//                 Ok(dir_entry) => match dir_entry.file_name().into_string() {
//                     Err(_) => {
//                         let message =
//                             v8::String::new(scope, "readdir failed to read item").unwrap();
//                         let exn = v8::Exception::error(scope, message);
//                         scope.throw_exception(exn);
//                     }
//                     Ok(name) => {
//                         let name = v8::String::new(scope, &name).unwrap();
//                         ret.set(name.into())
//                     }
//                 },
//             },
//         },
//     }
// }
//
// fn closedir(
//     scope: &mut v8::HandleScope,
//     args: v8::FunctionCallbackArguments,
//     mut ret: v8::ReturnValue,
// ) {
//     let d = args.get(0);
//     let d = d.to_number(scope).unwrap().value() as i32;
//     let context = scope.get_current_context();
//     let dir_table = context.get_slot_mut::<DirTable>().unwrap();
//     match dir_table.remove(d) {
//         None => {
//             let err_msg = &format!("unable to remove invalid dir object {}", d);
//             let message = v8::String::new(scope, err_msg).unwrap();
//             let exn = v8::Exception::error(scope, message);
//             scope.throw_exception(exn);
//         }
//         Some(_) => {
//             let undefined = v8::undefined(scope);
//             ret.set(undefined.into());
//         }
//     }
// }

// stat : JSString as Path -> Object
fn stat(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let metadata = match fs::metadata(path) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
        Ok(metadata) => metadata,
    };
    let filetype = metadata.file_type();
    let kind = if filetype.is_file() {
        0
    } else if filetype.is_dir() {
        1
    } else if filetype.is_char_device() {
        2
    } else if filetype.is_block_device() {
        3
    } else if filetype.is_symlink() {
        4
    } else if filetype.is_fifo() {
        5
    } else if filetype.is_socket() {
        6
    } else {
        panic!()
    };

    let stat = v8::Object::new(scope);

    let id = v8::String::new(scope, "kind").unwrap();
    let kind = v8::Number::new(scope, kind as f64);
    stat.set(scope, id.into(), kind.into());

    let id = v8::String::new(scope, "dev").unwrap();
    let dev = v8::Number::new(scope, metadata.dev() as f64);
    stat.set(scope, id.into(), dev.into());

    let id = v8::String::new(scope, "ino").unwrap();
    let ino = v8::Number::new(scope, metadata.ino() as f64);
    stat.set(scope, id.into(), ino.into());

    let id = v8::String::new(scope, "mode").unwrap();
    let mode = v8::Number::new(scope, metadata.mode() as f64);
    stat.set(scope, id.into(), mode.into());

    let id = v8::String::new(scope, "nlink").unwrap();
    let nlink = v8::Number::new(scope, metadata.nlink() as f64);
    stat.set(scope, id.into(), nlink.into());

    let id = v8::String::new(scope, "uid").unwrap();
    let uid = v8::Number::new(scope, metadata.uid() as f64);
    stat.set(scope, id.into(), uid.into());

    let id = v8::String::new(scope, "gid").unwrap();
    let gid = v8::Number::new(scope, metadata.gid() as f64);
    stat.set(scope, id.into(), gid.into());

    let id = v8::String::new(scope, "rdev").unwrap();
    let rdev = v8::Number::new(scope, metadata.rdev() as f64);
    stat.set(scope, id.into(), rdev.into());

    let id = v8::String::new(scope, "size").unwrap();
    let size = v8::Number::new(scope, metadata.size() as f64);
    stat.set(scope, id.into(), size.into());

    let id = v8::String::new(scope, "atime").unwrap();
    let atime = v8::Number::new(scope, metadata.atime() as f64);
    stat.set(scope, id.into(), atime.into());

    let id = v8::String::new(scope, "mtime").unwrap();
    let mtime = v8::Number::new(scope, metadata.mtime() as f64);
    stat.set(scope, id.into(), mtime.into());

    let id = v8::String::new(scope, "ctime").unwrap();
    let ctime = v8::Number::new(scope, metadata.ctime() as f64);
    stat.set(scope, id.into(), ctime.into());

    ret.set(stat.into());
}

// lstat : JSString as Path -> Object
fn lstat(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let path = args.get(0);
    let path = path.to_string(scope).unwrap();
    let path = path.to_rust_string_lossy(scope);
    let path = Path::new(&path);
    let metadata = match fs::symlink_metadata(path) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
        Ok(metadata) => metadata,
    };
    let filetype = metadata.file_type();
    let kind = if filetype.is_file() {
        0
    } else if filetype.is_dir() {
        1
    } else if filetype.is_char_device() {
        2
    } else if filetype.is_block_device() {
        3
    } else if filetype.is_symlink() {
        4
    } else if filetype.is_fifo() {
        5
    } else if filetype.is_socket() {
        6
    } else {
        panic!()
    };

    let stat = v8::Object::new(scope);

    let id = v8::String::new(scope, "kind").unwrap();
    let kind = v8::Number::new(scope, kind as f64);
    stat.set(scope, id.into(), kind.into());

    let id = v8::String::new(scope, "dev").unwrap();
    let dev = v8::Number::new(scope, metadata.dev() as f64);
    stat.set(scope, id.into(), dev.into());

    let id = v8::String::new(scope, "ino").unwrap();
    let ino = v8::Number::new(scope, metadata.ino() as f64);
    stat.set(scope, id.into(), ino.into());

    let id = v8::String::new(scope, "mode").unwrap();
    let mode = v8::Number::new(scope, metadata.mode() as f64);
    stat.set(scope, id.into(), mode.into());

    let id = v8::String::new(scope, "nlink").unwrap();
    let nlink = v8::Number::new(scope, metadata.nlink() as f64);
    stat.set(scope, id.into(), nlink.into());

    let id = v8::String::new(scope, "uid").unwrap();
    let uid = v8::Number::new(scope, metadata.uid() as f64);
    stat.set(scope, id.into(), uid.into());

    let id = v8::String::new(scope, "gid").unwrap();
    let gid = v8::Number::new(scope, metadata.gid() as f64);
    stat.set(scope, id.into(), gid.into());

    let id = v8::String::new(scope, "rdev").unwrap();
    let rdev = v8::Number::new(scope, metadata.rdev() as f64);
    stat.set(scope, id.into(), rdev.into());

    let id = v8::String::new(scope, "size").unwrap();
    let size = v8::Number::new(scope, metadata.size() as f64);
    stat.set(scope, id.into(), size.into());

    let id = v8::String::new(scope, "atime").unwrap();
    let atime = v8::Number::new(scope, metadata.atime() as f64);
    stat.set(scope, id.into(), atime.into());

    let id = v8::String::new(scope, "mtime").unwrap();
    let mtime = v8::Number::new(scope, metadata.mtime() as f64);
    stat.set(scope, id.into(), mtime.into());

    let id = v8::String::new(scope, "ctime").unwrap();
    let ctime = v8::Number::new(scope, metadata.ctime() as f64);
    stat.set(scope, id.into(), ctime.into());

    ret.set(stat.into());
}

// fstat: i32 as FileDescriptor -> Object
fn fstat(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let context = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get(fd) {
        Ok(file) => file,
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
    };
    let metadata = match file.metadata() {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
        Ok(metadata) => metadata,
    };
    let filetype = metadata.file_type();
    let kind = if filetype.is_file() {
        0
    } else if filetype.is_dir() {
        1
    } else if filetype.is_char_device() {
        2
    } else if filetype.is_block_device() {
        3
    } else if filetype.is_symlink() {
        4
    } else if filetype.is_fifo() {
        5
    } else if filetype.is_socket() {
        6
    } else {
        panic!()
    };

    let stat = v8::Object::new(scope);

    let id = v8::String::new(scope, "kind").unwrap();
    let kind = v8::Number::new(scope, kind as f64);
    stat.set(scope, id.into(), kind.into());

    let id = v8::String::new(scope, "dev").unwrap();
    let dev = v8::Number::new(scope, metadata.dev() as f64);
    stat.set(scope, id.into(), dev.into());

    let id = v8::String::new(scope, "ino").unwrap();
    let ino = v8::Number::new(scope, metadata.ino() as f64);
    stat.set(scope, id.into(), ino.into());

    let id = v8::String::new(scope, "mode").unwrap();
    let mode = v8::Number::new(scope, metadata.mode() as f64);
    stat.set(scope, id.into(), mode.into());

    let id = v8::String::new(scope, "nlink").unwrap();
    let nlink = v8::Number::new(scope, metadata.nlink() as f64);
    stat.set(scope, id.into(), nlink.into());

    let id = v8::String::new(scope, "uid").unwrap();
    let uid = v8::Number::new(scope, metadata.uid() as f64);
    stat.set(scope, id.into(), uid.into());

    let id = v8::String::new(scope, "gid").unwrap();
    let gid = v8::Number::new(scope, metadata.gid() as f64);
    stat.set(scope, id.into(), gid.into());

    let id = v8::String::new(scope, "rdev").unwrap();
    let rdev = v8::Number::new(scope, metadata.rdev() as f64);
    stat.set(scope, id.into(), rdev.into());

    let id = v8::String::new(scope, "size").unwrap();
    let size = v8::Number::new(scope, metadata.size() as f64);
    stat.set(scope, id.into(), size.into());

    let id = v8::String::new(scope, "atime").unwrap();
    let atime = v8::Number::new(scope, metadata.atime() as f64);
    stat.set(scope, id.into(), atime.into());

    let id = v8::String::new(scope, "mtime").unwrap();
    let mtime = v8::Number::new(scope, metadata.mtime() as f64);
    stat.set(scope, id.into(), mtime.into());

    let id = v8::String::new(scope, "ctime").unwrap();
    let ctime = v8::Number::new(scope, metadata.ctime() as f64);
    stat.set(scope, id.into(), ctime.into());

    ret.set(stat.into());
}

// fchmod: i32 as FileDescriptor, i32 as Mode -> undefined
fn fchmod(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let mode = args.get(1);
    let mode = mode.to_number(scope).unwrap().value() as u32;
    let permission = Permissions::from_mode(mode);
    let context = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get(fd) {
        Ok(file) => file,
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
    };
    match file.set_permissions(permission) {
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

// ftruncate: i32 as FileDescriptor, u64 as Length -> undefined
fn ftruncate(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let fd = args.get(0);
    let fd = fd.to_number(scope).unwrap().value() as i32;
    let context = scope.get_current_context();
    let fd_table = context.get_slot_mut::<FdTable>().unwrap();
    let file = match fd_table.get(fd) {
        Ok(file) => file,
        Err(err_msg) => {
            let message = v8::String::new(scope, &err_msg).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
            return;
        }
    };
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

// rename: JSString as OldPath, JSString as NewPath -> undefined
fn rename(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let oldpath = args.get(0);
    let oldpath = oldpath.to_string(scope).unwrap();
    let oldpath = oldpath.to_rust_string_lossy(scope);
    let newpath = args.get(1);
    let newpath = newpath.to_string(scope).unwrap();
    let newpath = newpath.to_rust_string_lossy(scope);
    match fs::rename(oldpath, newpath) {
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

// decode_utf8: Uint8Array -> String
fn decode_utf8(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let jsbytes = args.get(0);
    let jsbytes = jsbytes.try_cast::<v8::Uint8Array>().unwrap();
    let size = jsbytes.length();
    let mut bytes = vec![0u8; size];
    let copied = jsbytes.copy_contents(&mut bytes);
    debug_assert!(copied == size);
    match String::from_utf8(bytes) {
        Err(err) => {
            let message = v8::String::new(scope, &err.to_string()).unwrap();
            let exn = v8::Exception::error(scope, message);
            scope.throw_exception(exn);
        }
        Ok(str) => {
            let str = v8::String::new(scope, &str).unwrap();
            ret.set(str.into());
        }
    }
}

fn encode_scalar(scalar: u32, index: usize, limit: usize, bytes: &mut [u8]) -> usize {
    match scalar {
        0x0000..=0x007F => {
            if index < limit {
                bytes[index] = scalar as u8;
            }
            1
        }
        0x0080..=0x07FF => {
            if index + 1 < limit {
                bytes[index] = 0xC0 | ((scalar >> 6) as u8);
                bytes[index + 1] = 0x80 | (scalar as u8 & 0x3F);
            }
            2
        }
        0x0800..=0xFFFF => {
            if index + 2 < limit {
                bytes[index] = 0xE0 | ((scalar >> 12) as u8);
                bytes[index + 1] = 0x80 | ((scalar >> 6) as u8 & 0x3F);
                bytes[index + 2] = 0x80 | (scalar as u8 & 0x3F);
            }
            3
        }
        0x10000..=0x10FFFF => {
            if index + 3 < limit {
                bytes[index] = 0xF0 | ((scalar >> 18) as u8);
                bytes[index + 1] = 0x80 | ((scalar >> 12) as u8 & 0x3F);
                bytes[index + 2] = 0x80 | ((scalar >> 6) as u8 & 0x3F);
                bytes[index + 3] = 0x80 | (scalar as u8 & 0x3F);
            }
            4
        }
        _ => unreachable!(),
    }
}

// encode_into: JSString, Uint8Array -> Object { read, written }
fn encode_into(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let jsstr = args.get(0);
    let jsstr = jsstr.to_string(scope).unwrap();
    let mut str_content = vec![0u16; jsstr.length()];
    jsstr.write(scope, &mut str_content, 0, v8::WriteOptions::empty());
    let buf = args.get(1);
    let buf = buf.try_cast::<v8::Uint8Array>().unwrap();
    let buf_length = buf.byte_length();
    let raw_data = buf.data();
    let buf: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(raw_data as *mut u8, buf_length) };
    // utf16le -> utf8
    let mut src_index = 0;
    let mut dst_index = 0;
    while dst_index < buf_length && src_index < jsstr.length() {
        let code_unit = str_content[src_index].to_le();
        src_index += 1;
        if (0xD800..=0xDBFF).contains(&code_unit) {
            if src_index < jsstr.length() {
                let next = str_content[src_index].to_le();
                if (0xDC00..=0xDFFF).contains(&next) {
                    // 计算 Unicode 标量值
                    let high = (code_unit - 0xD800) as u32;
                    let low = (next - 0xDC00) as u32;
                    let scalar = 0x10000 + (high << 10 | low);
                    src_index += 1;
                    let offset = encode_scalar(scalar, dst_index, buf_length, buf);
                    if dst_index + offset <= buf_length {
                        dst_index += offset
                    } else {
                        break;
                    }
                } else {
                    unreachable!()
                }
            } else {
                unreachable!()
            }
        } else {
            let offset = encode_scalar(code_unit as u32, dst_index, buf_length, buf);
            if dst_index + offset <= buf_length {
                dst_index += offset
            } else {
                break;
            }
        }
    }
    let retobj = v8::Object::new(scope);
    let read = v8::Number::new(scope, src_index as f64);
    let written = v8::Number::new(scope, dst_index as f64);
    let id = v8::String::new(scope, "read").unwrap();
    retobj.set(scope, id.into(), read.into());
    let id = v8::String::new(scope, "written").unwrap();
    retobj.set(scope, id.into(), written.into());
    ret.set(retobj.into());
}

pub fn init_wasmoo<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
) -> v8::Local<'s, v8::Object> {
    let getenv = v8::FunctionTemplate::new(scope, getenv);
    let getenv = getenv.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "getenv").unwrap();
    obj.set(scope, ident.into(), getenv.into());

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

    let getcwd = v8::FunctionTemplate::new(scope, getcwd);
    let getcwd = getcwd.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "getcwd").unwrap();
    obj.set(scope, ident.into(), getcwd.into());

    let chdir = v8::FunctionTemplate::new(scope, chdir);
    let chdir = chdir.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "chdir").unwrap();
    obj.set(scope, ident.into(), chdir.into());

    let mkdir = v8::FunctionTemplate::new(scope, mkdir);
    let mkdir = mkdir.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "mkdir").unwrap();
    obj.set(scope, ident.into(), mkdir.into());

    let rmdir = v8::FunctionTemplate::new(scope, rmdir);
    let rmdir = rmdir.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "rmdir").unwrap();
    obj.set(scope, ident.into(), rmdir.into());

    // let link = v8::FunctionTemplate::new(scope, link);
    // let link = link.get_function(scope).unwrap();
    // let ident = v8::String::new(scope, "link").unwrap();
    // obj.set(scope, ident.into(), link.into());

    // let symlink = v8::FunctionTemplate::new(scope, symlink);
    // let symlink = symlink.get_function(scope).unwrap();
    // let ident = v8::String::new(scope, "symlink").unwrap();
    // obj.set(scope, ident.into(), symlink.into());

    // let readlink = v8::FunctionTemplate::new(scope, readlink);
    // let readlink = readlink.get_function(scope).unwrap();
    // let ident = v8::String::new(scope, "readlink").unwrap();
    // obj.set(scope, ident.into(), readlink.into());

    let unlink = v8::FunctionTemplate::new(scope, unlink);
    let unlink = unlink.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "unlink").unwrap();
    obj.set(scope, ident.into(), unlink.into());

    let read_dir = v8::FunctionTemplate::new(scope, read_dir);
    let read_dir = read_dir.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "read_dir").unwrap();
    obj.set(scope, ident.into(), read_dir.into());

    // let opendir = v8::FunctionTemplate::new(scope, opendir);
    // let opendir = opendir.get_function(scope).unwrap();
    // let ident = v8::String::new(scope, "opendir").unwrap();
    // obj.set(scope, ident.into(), opendir.into());

    // let readdir = v8::FunctionTemplate::new(scope, readdir);
    // let readdir = readdir.get_function(scope).unwrap();
    // let ident = v8::String::new(scope, "readdir").unwrap();
    // obj.set(scope, ident.into(), readdir.into());

    // let closedir = v8::FunctionTemplate::new(scope, closedir);
    // let closedir = closedir.get_function(scope).unwrap();
    // let ident = v8::String::new(scope, "closedir").unwrap();
    // obj.set(scope, ident.into(), closedir.into());

    let stat = v8::FunctionTemplate::new(scope, stat);
    let stat = stat.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "stat").unwrap();
    obj.set(scope, ident.into(), stat.into());

    let fstat = v8::FunctionTemplate::new(scope, fstat);
    let fstat = fstat.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "fstat").unwrap();
    obj.set(scope, ident.into(), fstat.into());

    let fchmod = v8::FunctionTemplate::new(scope, fchmod);
    let fchmod = fchmod.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "fchmod").unwrap();
    obj.set(scope, ident.into(), fchmod.into());

    let isatty = v8::FunctionTemplate::new(scope, isatty);
    let isatty = isatty.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "isatty").unwrap();
    obj.set(scope, ident.into(), isatty.into());

    let ftruncate = v8::FunctionTemplate::new(scope, ftruncate);
    let ftruncate = ftruncate.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "ftruncate").unwrap();
    obj.set(scope, ident.into(), ftruncate.into());

    let rename = v8::FunctionTemplate::new(scope, rename);
    let rename = rename.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "rename").unwrap();
    obj.set(scope, ident.into(), rename.into());

    let utimes = v8::FunctionTemplate::new(scope, utimes);
    let utimes = utimes.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "utimes").unwrap();
    obj.set(scope, ident.into(), utimes.into());

    let exit = v8::FunctionTemplate::new(scope, exit);
    let exit = exit.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "exit").unwrap();
    obj.set(scope, ident.into(), exit.into());

    let lstat = v8::FunctionTemplate::new(scope, lstat);
    let lstat = lstat.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "lstat").unwrap();
    obj.set(scope, ident.into(), lstat.into());

    let read_file_to_bytes = v8::FunctionTemplate::new(scope, read_file_to_bytes);
    let read_file_to_bytes = read_file_to_bytes.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "read_file_to_bytes").unwrap();
    obj.set(scope, ident.into(), read_file_to_bytes.into());

    let decode_utf8 = v8::FunctionTemplate::new(scope, decode_utf8);
    let decode_utf8 = decode_utf8.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "decode_utf8").unwrap();
    obj.set(scope, ident.into(), decode_utf8.into());

    let encode_into = v8::FunctionTemplate::new(scope, encode_into);
    let encode_into = encode_into.get_function(scope).unwrap();
    let ident = v8::String::new(scope, "encode_into").unwrap();
    obj.set(scope, ident.into(), encode_into.into());

    obj
}

#[allow(dead_code)]
fn test_wasmoo_extern(script: &str) -> String {
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    let isolate = &mut v8::Isolate::new(Default::default());
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope, Default::default());
    // setup file descriptor table
    context.set_slot(FdTable {
        map: HashMap::new(),
        next_fd: 3,
    });
    // setup directory table
    // context.set_slot(DirTable {
    //     map: HashMap::new(),
    //     next_d: 0,
    // });
    let scope = &mut v8::ContextScope::new(scope, context);
    let global_proxy = scope.get_current_context().global(scope);
    init_wasmoo(global_proxy, scope);
    let code = v8::String::new(scope, script).unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    match script.run(scope) {
        None => String::from("ffffailed!"),
        Some(val) => val.to_string(scope).unwrap().to_rust_string_lossy(scope),
    }
}
