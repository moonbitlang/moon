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

use crate::v8_builder::ScopeExt;
use rand::RngCore;
use std::any::Any;
use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

type WasiErrno = i32;
type WasiResult<T> = Result<T, WasiErrno>;

const WASI_ERRNO_SUCCESS: WasiErrno = 0;
const WASI_ERRNO_AGAIN: WasiErrno = 6;
const WASI_ERRNO_ACCES: WasiErrno = 2;
const WASI_ERRNO_BADF: WasiErrno = 8;
const WASI_ERRNO_EXIST: WasiErrno = 20;
const WASI_ERRNO_FAULT: WasiErrno = 21;
const WASI_ERRNO_INVAL: WasiErrno = 28;
const WASI_ERRNO_IO: WasiErrno = 29;
const WASI_ERRNO_ISDIR: WasiErrno = 31;
const WASI_ERRNO_NOENT: WasiErrno = 44;
const WASI_ERRNO_NAMETOOLONG: WasiErrno = 37;
const WASI_ERRNO_NOTDIR: WasiErrno = 54;
const WASI_ERRNO_NOTEMPTY: WasiErrno = 55;
const WASI_ERRNO_PIPE: WasiErrno = 64;
const WASI_ERRNO_NOTCAPABLE: WasiErrno = 76;

const WASI_FD_STDIN: i32 = 0;
const WASI_FD_STDOUT: i32 = 1;
const WASI_FD_STDERR: i32 = 2;
const WASI_FD_PREOPEN_DIR: i32 = 3;
const WASI_FD_DYNAMIC_START: i32 = 4;
const WASI_IOVEC_SIZE: usize = 8;
const WASI_DIRENT_SIZE: usize = 24;

const WASI_PREOPEN_TYPE_DIR: u8 = 0;
const WASI_FILETYPE_UNKNOWN: u8 = 0;
const WASI_FILETYPE_CHARACTER_DEVICE: u8 = 2;
const WASI_FILETYPE_DIRECTORY: u8 = 3;
const WASI_FILETYPE_REGULAR_FILE: u8 = 4;
const WASI_FILETYPE_SYMBOLIC_LINK: u8 = 7;

const WASI_FDFLAG_APPEND: i32 = 1;

const WASI_OFLAGS_CREAT: i32 = 1;
const WASI_OFLAGS_DIRECTORY: i32 = 2;
const WASI_OFLAGS_EXCL: i32 = 4;
const WASI_OFLAGS_TRUNC: i32 = 8;

const WASI_LOOKUPFLAG_SYMLINK_FOLLOW: i32 = 1;

const WASI_RIGHT_FD_READ: u64 = 1u64 << 1;
const WASI_RIGHT_FD_WRITE: u64 = 1u64 << 6;

struct DescriptorTable {
    next_fd: i32,
    entries: BTreeMap<i32, Arc<Descriptor>>,
}

impl DescriptorTable {
    fn new() -> Self {
        Self {
            next_fd: WASI_FD_DYNAMIC_START,
            entries: BTreeMap::new(),
        }
    }

    fn insert(&mut self, descriptor: Arc<Descriptor>) -> WasiResult<i32> {
        let fd = self.next_fd;
        self.next_fd = self.next_fd.checked_add(1).ok_or(WASI_ERRNO_FAULT)?;
        self.entries.insert(fd, descriptor);
        Ok(fd)
    }
}

enum Descriptor {
    File(Mutex<fs::File>),
    Directory(PathBuf),
}

#[repr(i32)]
#[derive(Clone, Copy)]
enum ClockId {
    Realtime = 0,
    Monotonic = 1,
    ProcessCpuTime = 2,
    ThreadCpuTime = 3,
}

impl TryFrom<i32> for ClockId {
    type Error = WasiErrno;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Realtime),
            1 => Ok(Self::Monotonic),
            2 => Ok(Self::ProcessCpuTime),
            3 => Ok(Self::ThreadCpuTime),
            _ => Err(WASI_ERRNO_INVAL),
        }
    }
}

struct WasiContext {
    argv: Vec<Vec<u8>>,
    monotonic_origin: Instant,
    preopen_dir_name: Vec<u8>,
    preopen_dir_host_path: PathBuf,
    descriptors: Mutex<DescriptorTable>,
    memory: OnceLock<v8::Global<v8::WasmMemoryObject>>,
}

struct DirectoryEntry {
    name: Vec<u8>,
    file_type: u8,
}

struct FileStatData {
    file_type: u8,
    nlink: u64,
    size: u64,
    atim: u64,
    mtim: u64,
    ctim: u64,
}

fn encode_c_string(value: impl Into<String>) -> Vec<u8> {
    let mut bytes = value.into().into_bytes();
    bytes.push(0);
    bytes
}

fn build_argv(wasm_file_name: &str, args: &[String]) -> Vec<Vec<u8>> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(encode_c_string(wasm_file_name));
    argv.extend(args.iter().map(|arg| encode_c_string(arg.as_str())));
    argv
}

fn collect_environ() -> Vec<Vec<u8>> {
    std::env::vars()
        .map(|(key, value)| encode_c_string(format!("{key}={value}")))
        .collect()
}

fn read_i32_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> WasiResult<i32> {
    args.get(index).int32_value(scope).ok_or(WASI_ERRNO_INVAL)
}

fn read_u64_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> WasiResult<u64> {
    let value = args.get(index);
    if value.is_big_int() {
        let bigint = v8::Local::<v8::BigInt>::try_from(value).map_err(|_| WASI_ERRNO_INVAL)?;
        let (result, lossless) = bigint.u64_value();
        if lossless {
            Ok(result)
        } else {
            Err(WASI_ERRNO_INVAL)
        }
    } else {
        let value = value.integer_value(scope).ok_or(WASI_ERRNO_INVAL)?;
        u64::try_from(value).map_err(|_| WASI_ERRNO_INVAL)
    }
}

fn ptr_to_offset(ptr: i32) -> WasiResult<usize> {
    usize::try_from(ptr).map_err(|_| WASI_ERRNO_FAULT)
}

fn checked_range(memory: &[u8], offset: usize, len: usize) -> WasiResult<&[u8]> {
    let end = offset.checked_add(len).ok_or(WASI_ERRNO_FAULT)?;
    memory.get(offset..end).ok_or(WASI_ERRNO_FAULT)
}

fn checked_mut_range(memory: &mut [u8], offset: usize, len: usize) -> WasiResult<&mut [u8]> {
    let end = offset.checked_add(len).ok_or(WASI_ERRNO_FAULT)?;
    memory.get_mut(offset..end).ok_or(WASI_ERRNO_FAULT)
}

fn write_u32_at(memory: &mut [u8], offset: usize, value: u32) -> WasiResult<()> {
    checked_mut_range(memory, offset, 4)?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32(memory: &mut [u8], ptr: i32, value: u32) -> WasiResult<()> {
    write_u32_at(memory, ptr_to_offset(ptr)?, value)
}

fn read_u32_at(memory: &[u8], offset: usize) -> WasiResult<u32> {
    let end = offset.checked_add(4).ok_or(WASI_ERRNO_FAULT)?;
    let bytes = memory.get(offset..end).ok_or(WASI_ERRNO_FAULT)?;
    Ok(u32::from_le_bytes(
        <[u8; 4]>::try_from(bytes).map_err(|_| WASI_ERRNO_FAULT)?,
    ))
}

fn write_u64_at(memory: &mut [u8], offset: usize, value: u64) -> WasiResult<()> {
    checked_mut_range(memory, offset, 8)?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u64(memory: &mut [u8], ptr: i32, value: u64) -> WasiResult<()> {
    let offset = ptr_to_offset(ptr)?;
    write_u64_at(memory, offset, value)
}

fn table_bytes_len(values: &[Vec<u8>]) -> WasiResult<u32> {
    let total = values.iter().try_fold(0usize, |acc, value| {
        acc.checked_add(value.len()).ok_or(WASI_ERRNO_FAULT)
    })?;
    u32::try_from(total).map_err(|_| WASI_ERRNO_FAULT)
}

fn write_c_string_table(
    memory: &mut [u8],
    values: &[Vec<u8>],
    pointers_ptr: i32,
    bytes_ptr: i32,
) -> WasiResult<()> {
    let pointers_base = ptr_to_offset(pointers_ptr)?;
    let mut cursor = ptr_to_offset(bytes_ptr)?;

    for (index, value) in values.iter().enumerate() {
        let pointer_slot = pointers_base
            .checked_add(index.checked_mul(4).ok_or(WASI_ERRNO_FAULT)?)
            .ok_or(WASI_ERRNO_FAULT)?;

        let cursor_u32 = u32::try_from(cursor).map_err(|_| WASI_ERRNO_FAULT)?;
        write_u32_at(memory, pointer_slot, cursor_u32)?;

        checked_mut_range(memory, cursor, value.len())?.copy_from_slice(value);
        cursor = cursor.checked_add(value.len()).ok_or(WASI_ERRNO_FAULT)?;
    }

    Ok(())
}

fn iovec(memory: &[u8], iovs_ptr: i32, index: u32) -> WasiResult<(usize, usize)> {
    let base = ptr_to_offset(iovs_ptr)?;
    let index_offset = usize::try_from(index).map_err(|_| WASI_ERRNO_FAULT)?;
    let iov_offset = base
        .checked_add(
            index_offset
                .checked_mul(WASI_IOVEC_SIZE)
                .ok_or(WASI_ERRNO_FAULT)?,
        )
        .ok_or(WASI_ERRNO_FAULT)?;

    let buf_ptr = read_u32_at(memory, iov_offset)?;
    let buf_len = read_u32_at(memory, iov_offset + 4)?;
    let buf_offset = usize::try_from(buf_ptr).map_err(|_| WASI_ERRNO_FAULT)?;
    let len = usize::try_from(buf_len).map_err(|_| WASI_ERRNO_FAULT)?;
    Ok((buf_offset, len))
}

fn preopen_name() -> Vec<u8> {
    b".".to_vec()
}

fn with_descriptor_table<T>(
    context: &WasiContext,
    f: impl FnOnce(&mut DescriptorTable) -> WasiResult<T>,
) -> WasiResult<T> {
    let mut table = context.descriptors.lock().map_err(|_| WASI_ERRNO_IO)?;
    f(&mut table)
}

fn descriptor_for_fd(context: &WasiContext, fd: i32) -> WasiResult<Arc<Descriptor>> {
    let table = context.descriptors.lock().map_err(|_| WASI_ERRNO_IO)?;
    table.entries.get(&fd).cloned().ok_or(WASI_ERRNO_BADF)
}

fn preopen_matches(context: &WasiContext, fd: i32) -> bool {
    fd == WASI_FD_PREOPEN_DIR && !context.preopen_dir_name.is_empty()
}

fn dir_path_for_fd(context: &WasiContext, fd: i32) -> WasiResult<PathBuf> {
    if preopen_matches(context, fd) {
        return Ok(context.preopen_dir_host_path.clone());
    }

    let descriptor = descriptor_for_fd(context, fd)?;
    match descriptor.as_ref() {
        Descriptor::Directory(path) => Ok(path.clone()),
        _ => Err(WASI_ERRNO_BADF),
    }
}

fn file_for_fd(context: &WasiContext, fd: i32) -> WasiResult<Arc<Descriptor>> {
    let descriptor = descriptor_for_fd(context, fd)?;
    if matches!(descriptor.as_ref(), Descriptor::File(_)) {
        Ok(descriptor)
    } else {
        Err(WASI_ERRNO_BADF)
    }
}

fn read_path_from_memory(memory: &[u8], path_ptr: i32, path_len: usize) -> WasiResult<String> {
    let path = checked_range(memory, ptr_to_offset(path_ptr)?, path_len)?;
    let path = std::str::from_utf8(path).map_err(|_| WASI_ERRNO_INVAL)?;
    if path.contains('\0') {
        return Err(WASI_ERRNO_INVAL);
    }
    Ok(path.to_string())
}

fn validate_guest_relative_path(path: &str) -> WasiResult<()> {
    let mut depth = 0i32;
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(_) => depth = depth.saturating_add(1),
            Component::ParentDir => {
                if depth == 0 {
                    return Err(WASI_ERRNO_NOTCAPABLE);
                }
                depth -= 1;
            }
            Component::RootDir | Component::Prefix(_) => return Err(WASI_ERRNO_NOTCAPABLE),
        }
    }
    Ok(())
}

fn resolve_path(context: &WasiContext, dirfd: i32, path: &str) -> WasiResult<PathBuf> {
    validate_guest_relative_path(path)?;
    let base = dir_path_for_fd(context, dirfd)?;
    let relative = if path.is_empty() { "." } else { path };
    Ok(base.join(relative))
}

fn io_error_to_errno(error: &std::io::Error) -> WasiErrno {
    match error.kind() {
        ErrorKind::NotFound => WASI_ERRNO_NOENT,
        ErrorKind::PermissionDenied => WASI_ERRNO_ACCES,
        ErrorKind::AlreadyExists => WASI_ERRNO_EXIST,
        ErrorKind::InvalidInput | ErrorKind::InvalidData => WASI_ERRNO_INVAL,
        ErrorKind::NotADirectory => WASI_ERRNO_NOTDIR,
        ErrorKind::IsADirectory => WASI_ERRNO_ISDIR,
        ErrorKind::DirectoryNotEmpty => WASI_ERRNO_NOTEMPTY,
        ErrorKind::WouldBlock => WASI_ERRNO_AGAIN,
        ErrorKind::BrokenPipe => WASI_ERRNO_PIPE,
        _ => WASI_ERRNO_IO,
    }
}

fn entry_file_type(file_type: fs::FileType) -> u8 {
    if file_type.is_dir() {
        WASI_FILETYPE_DIRECTORY
    } else if file_type.is_file() {
        WASI_FILETYPE_REGULAR_FILE
    } else if file_type.is_symlink() {
        WASI_FILETYPE_SYMBOLIC_LINK
    } else {
        WASI_FILETYPE_UNKNOWN
    }
}

fn collect_directory_entries(path: &Path) -> WasiResult<Vec<DirectoryEntry>> {
    let mut entries = vec![
        DirectoryEntry {
            name: b".".to_vec(),
            file_type: WASI_FILETYPE_DIRECTORY,
        },
        DirectoryEntry {
            name: b"..".to_vec(),
            file_type: WASI_FILETYPE_DIRECTORY,
        },
    ];

    let host_entries = fs::read_dir(path).map_err(|_| WASI_ERRNO_IO)?;
    for entry in host_entries {
        let entry = entry.map_err(|_| WASI_ERRNO_IO)?;
        let name = entry
            .file_name()
            .to_string_lossy()
            .into_owned()
            .into_bytes();
        let file_type = entry.file_type().map_err(|_| WASI_ERRNO_IO)?;
        entries.push(DirectoryEntry {
            name,
            file_type: entry_file_type(file_type),
        });
    }

    Ok(entries)
}

fn serialize_dirent(entry_index: usize, entry: &DirectoryEntry) -> WasiResult<Vec<u8>> {
    let next = u64::try_from(entry_index + 1).map_err(|_| WASI_ERRNO_FAULT)?;
    let name_len = u32::try_from(entry.name.len()).map_err(|_| WASI_ERRNO_FAULT)?;

    let mut bytes = Vec::with_capacity(WASI_DIRENT_SIZE + entry.name.len());
    bytes.extend_from_slice(&next.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&name_len.to_le_bytes());
    bytes.push(entry.file_type);
    bytes.extend_from_slice(&[0, 0, 0]);
    bytes.extend_from_slice(&entry.name);
    Ok(bytes)
}

fn system_time_to_ns(time: std::io::Result<SystemTime>) -> u64 {
    time.ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn stat_data_for_metadata(metadata: &fs::Metadata) -> FileStatData {
    FileStatData {
        file_type: entry_file_type(metadata.file_type()),
        nlink: 1,
        size: metadata.len(),
        atim: system_time_to_ns(metadata.accessed()),
        mtim: system_time_to_ns(metadata.modified()),
        ctim: system_time_to_ns(metadata.created()),
    }
}

fn write_filestat(memory: &mut [u8], buf_ptr: i32, stat: &FileStatData) -> WasiResult<()> {
    let offset = ptr_to_offset(buf_ptr)?;
    checked_mut_range(memory, offset, 64)?.fill(0);
    checked_mut_range(memory, offset + 16, 1)?[0] = stat.file_type;
    write_u64_at(memory, offset + 24, stat.nlink)?;
    write_u64_at(memory, offset + 32, stat.size)?;
    write_u64_at(memory, offset + 40, stat.atim)?;
    write_u64_at(memory, offset + 48, stat.mtim)?;
    write_u64_at(memory, offset + 56, stat.ctim)?;
    Ok(())
}

fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s WasiContext {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(ptr as *const WasiContext) }
}

fn cached_wasi_memory<'s>(
    scope: &mut v8::HandleScope<'s>,
    context: &WasiContext,
) -> WasiResult<v8::Local<'s, v8::WasmMemoryObject>> {
    context
        .memory
        .get()
        .map(|memory| v8::Local::new(scope, memory))
        .ok_or(WASI_ERRNO_FAULT)
}

fn with_wasi_memory_mut<T>(
    scope: &mut v8::HandleScope,
    context: &WasiContext,
    f: impl FnOnce(&mut [u8]) -> WasiResult<T>,
) -> WasiResult<T> {
    let memory_object = cached_wasi_memory(scope, context)?;
    let buffer = memory_object.buffer();
    let len = buffer.byte_length();

    let Some(ptr) = buffer.data() else {
        if len == 0 {
            let mut empty = [];
            return f(&mut empty);
        }
        return Err(WASI_ERRNO_FAULT);
    };

    let memory = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr() as *mut u8, len) };
    f(memory)
}

fn result_to_errno(result: WasiResult<()>) -> WasiErrno {
    match result {
        Ok(()) => WASI_ERRNO_SUCCESS,
        Err(errno) => errno,
    }
}

fn finish_with_result(ret: &mut v8::ReturnValue, result: WasiResult<()>) {
    ret.set_int32(result_to_errno(result));
}

fn set_memory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let context = callback_context(&args);
        let memory_value = args.get(0);
        let memory = v8::Local::<v8::WasmMemoryObject>::try_from(memory_value)
            .map_err(|_| WASI_ERRNO_INVAL)?;
        let _ = context.memory.set(v8::Global::new(scope, memory));
        Ok(())
    })();
    finish_with_result(&mut ret, result);
}

fn fd_filestat_data(context: &WasiContext, fd: i32) -> WasiResult<FileStatData> {
    match fd {
        WASI_FD_STDIN | WASI_FD_STDOUT | WASI_FD_STDERR => Ok(FileStatData {
            file_type: WASI_FILETYPE_CHARACTER_DEVICE,
            nlink: 1,
            size: 0,
            atim: 0,
            mtim: 0,
            ctim: 0,
        }),
        _ if preopen_matches(context, fd) => {
            let metadata = fs::metadata(&context.preopen_dir_host_path)
                .map_err(|error| io_error_to_errno(&error))?;
            Ok(stat_data_for_metadata(&metadata))
        }
        _ => {
            let descriptor = descriptor_for_fd(context, fd)?;
            match descriptor.as_ref() {
                Descriptor::Directory(path) => {
                    let metadata = fs::metadata(path).map_err(|error| io_error_to_errno(&error))?;
                    Ok(stat_data_for_metadata(&metadata))
                }
                Descriptor::File(file) => {
                    let file = file.lock().map_err(|_| WASI_ERRNO_IO)?;
                    let metadata = file.metadata().map_err(|error| io_error_to_errno(&error))?;
                    Ok(stat_data_for_metadata(&metadata))
                }
            }
        }
    }
}

fn fd_close(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let context = callback_context(&args);
        if fd <= WASI_FD_PREOPEN_DIR {
            return Err(WASI_ERRNO_BADF);
        }

        with_descriptor_table(context, |table| {
            if table.entries.remove(&fd).is_some() {
                Ok(())
            } else {
                Err(WASI_ERRNO_BADF)
            }
        })
    })();

    finish_with_result(&mut ret, result);
}

fn fd_filestat_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let stat_ptr = read_i32_arg(scope, &args, 1)?;
        let context = callback_context(&args);
        let stat = fd_filestat_data(context, fd)?;

        with_wasi_memory_mut(scope, context, |memory| {
            write_filestat(memory, stat_ptr, &stat)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn path_open(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let _dirflags = read_i32_arg(scope, &args, 1)?;
        let path_ptr = read_i32_arg(scope, &args, 2)?;
        let path_len =
            usize::try_from(read_i32_arg(scope, &args, 3)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let oflags = read_i32_arg(scope, &args, 4)?;
        let rights_base = read_u64_arg(scope, &args, 5)?;
        let _rights_inheriting = read_u64_arg(scope, &args, 6)?;
        let fdflags = read_i32_arg(scope, &args, 7)?;
        let opened_fd_ptr = read_i32_arg(scope, &args, 8)?;
        let context = callback_context(&args);

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;

        let descriptor = if (oflags & WASI_OFLAGS_DIRECTORY) != 0 {
            if (oflags & (WASI_OFLAGS_CREAT | WASI_OFLAGS_EXCL | WASI_OFLAGS_TRUNC)) != 0 {
                return Err(WASI_ERRNO_INVAL);
            }
            let metadata = fs::metadata(&full_path).map_err(|error| io_error_to_errno(&error))?;
            if !metadata.is_dir() {
                return Err(WASI_ERRNO_NOTDIR);
            }
            Arc::new(Descriptor::Directory(full_path))
        } else {
            let wants_read = (rights_base & WASI_RIGHT_FD_READ) != 0;
            let wants_write = (rights_base & WASI_RIGHT_FD_WRITE) != 0;
            let append = (fdflags & WASI_FDFLAG_APPEND) != 0;

            let mut options = fs::OpenOptions::new();
            options.read(wants_read || !wants_write);
            options.write(wants_write || append);
            options.append(append);
            options.create((oflags & WASI_OFLAGS_CREAT) != 0);
            options.create_new((oflags & WASI_OFLAGS_EXCL) != 0);
            options.truncate((oflags & WASI_OFLAGS_TRUNC) != 0);

            let file = options
                .open(full_path)
                .map_err(|error| io_error_to_errno(&error))?;
            Arc::new(Descriptor::File(Mutex::new(file)))
        };

        let opened_fd = with_descriptor_table(context, |table| table.insert(descriptor))?;
        let opened_fd = u32::try_from(opened_fd).map_err(|_| WASI_ERRNO_FAULT)?;

        with_wasi_memory_mut(scope, context, |memory| {
            write_u32(memory, opened_fd_ptr, opened_fd)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn path_readlink(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_i32_arg(scope, &args, 1)?;
        let path_len =
            usize::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let buf_ptr = read_i32_arg(scope, &args, 3)?;
        let buf_len =
            usize::try_from(read_i32_arg(scope, &args, 4)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let buf_used_ptr = read_i32_arg(scope, &args, 5)?;
        let context = callback_context(&args);

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        let link_target = fs::read_link(&full_path).map_err(|error| io_error_to_errno(&error))?;
        let link_bytes = link_target
            .as_os_str()
            .to_string_lossy()
            .into_owned()
            .into_bytes();

        with_wasi_memory_mut(scope, context, |memory| {
            let out = checked_mut_range(memory, ptr_to_offset(buf_ptr)?, buf_len)?;
            let used = out.len().min(link_bytes.len());
            out[..used].copy_from_slice(&link_bytes[..used]);
            let used_u32 = u32::try_from(used).map_err(|_| WASI_ERRNO_FAULT)?;
            write_u32(memory, buf_used_ptr, used_u32)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn path_create_directory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_i32_arg(scope, &args, 1)?;
        let path_len =
            usize::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let context = callback_context(&args);

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        fs::create_dir(full_path).map_err(|error| io_error_to_errno(&error))
    })();

    finish_with_result(&mut ret, result);
}

fn path_rename(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let old_fd = read_i32_arg(scope, &args, 0)?;
        let old_path_ptr = read_i32_arg(scope, &args, 1)?;
        let old_path_len =
            usize::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let new_fd = read_i32_arg(scope, &args, 3)?;
        let new_path_ptr = read_i32_arg(scope, &args, 4)?;
        let new_path_len =
            usize::try_from(read_i32_arg(scope, &args, 5)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let context = callback_context(&args);

        let old_path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, old_path_ptr, old_path_len)
        })?;
        let new_path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, new_path_ptr, new_path_len)
        })?;

        let old_host_path = resolve_path(context, old_fd, &old_path)?;
        let new_host_path = resolve_path(context, new_fd, &new_path)?;
        fs::rename(old_host_path, new_host_path).map_err(|error| io_error_to_errno(&error))
    })();

    finish_with_result(&mut ret, result);
}

fn path_filestat_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let flags = read_i32_arg(scope, &args, 1)?;
        let path_ptr = read_i32_arg(scope, &args, 2)?;
        let path_len =
            usize::try_from(read_i32_arg(scope, &args, 3)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let stat_ptr = read_i32_arg(scope, &args, 4)?;
        let context = callback_context(&args);

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        let metadata = if (flags & WASI_LOOKUPFLAG_SYMLINK_FOLLOW) != 0 {
            fs::metadata(&full_path).map_err(|error| io_error_to_errno(&error))?
        } else {
            fs::symlink_metadata(&full_path).map_err(|error| io_error_to_errno(&error))?
        };
        let stat = stat_data_for_metadata(&metadata);

        with_wasi_memory_mut(scope, context, |memory| {
            write_filestat(memory, stat_ptr, &stat)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn path_remove_directory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_i32_arg(scope, &args, 1)?;
        let path_len =
            usize::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let context = callback_context(&args);

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        fs::remove_dir(full_path).map_err(|error| io_error_to_errno(&error))
    })();

    finish_with_result(&mut ret, result);
}

fn path_unlink_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let dirfd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_i32_arg(scope, &args, 1)?;
        let path_len =
            usize::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let context = callback_context(&args);

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        fs::remove_file(full_path).map_err(|error| io_error_to_errno(&error))
    })();

    finish_with_result(&mut ret, result);
}

fn fd_prestat_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let prestat_ptr = read_i32_arg(scope, &args, 1)?;
        let context = callback_context(&args);
        if !preopen_matches(context, fd) {
            return Err(WASI_ERRNO_BADF);
        }

        let preopen_name_len =
            u32::try_from(context.preopen_dir_name.len()).map_err(|_| WASI_ERRNO_FAULT)?;
        with_wasi_memory_mut(scope, context, |memory| {
            let prestat = checked_mut_range(memory, ptr_to_offset(prestat_ptr)?, 8)?;
            prestat.fill(0);
            prestat[0] = WASI_PREOPEN_TYPE_DIR;
            prestat[4..8].copy_from_slice(&preopen_name_len.to_le_bytes());
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn fd_prestat_dir_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let path_ptr = read_i32_arg(scope, &args, 1)?;
        let path_len =
            usize::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let context = callback_context(&args);
        if !preopen_matches(context, fd) {
            return Err(WASI_ERRNO_BADF);
        }

        let name = &context.preopen_dir_name;
        if path_len < name.len() {
            return Err(WASI_ERRNO_NAMETOOLONG);
        }

        with_wasi_memory_mut(scope, context, |memory| {
            let path_buf = checked_mut_range(memory, ptr_to_offset(path_ptr)?, path_len)?;
            path_buf[..name.len()].copy_from_slice(name);
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn fd_readdir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let buf_ptr = read_i32_arg(scope, &args, 1)?;
        let buf_len =
            usize::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let cookie = read_u64_arg(scope, &args, 3)?;
        let buf_used_ptr = read_i32_arg(scope, &args, 4)?;
        let context = callback_context(&args);
        let dir_path = dir_path_for_fd(context, fd)?;
        let entries = collect_directory_entries(&dir_path)?;
        let start = usize::try_from(cookie).map_err(|_| WASI_ERRNO_INVAL)?;

        with_wasi_memory_mut(scope, context, |memory| {
            let buf = checked_mut_range(memory, ptr_to_offset(buf_ptr)?, buf_len)?;
            let mut used = 0usize;

            for (entry_index, entry) in entries.iter().enumerate().skip(start) {
                if used == buf_len {
                    break;
                }
                let serialized = serialize_dirent(entry_index, entry)?;
                let remaining = buf_len.checked_sub(used).ok_or(WASI_ERRNO_FAULT)?;
                let to_copy = remaining.min(serialized.len());
                let end = used.checked_add(to_copy).ok_or(WASI_ERRNO_FAULT)?;
                buf[used..end].copy_from_slice(&serialized[..to_copy]);
                used = end;
                if to_copy < serialized.len() {
                    break;
                }
            }

            let used_u32 = u32::try_from(used).map_err(|_| WASI_ERRNO_FAULT)?;
            write_u32(memory, buf_used_ptr, used_u32)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn args_sizes_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let argc_ptr = read_i32_arg(scope, &args, 0)?;
        let argv_buf_size_ptr = read_i32_arg(scope, &args, 1)?;

        let context = callback_context(&args);
        let argc = u32::try_from(context.argv.len()).map_err(|_| WASI_ERRNO_FAULT)?;
        let argv_buf_size = table_bytes_len(&context.argv)?;

        with_wasi_memory_mut(scope, context, |memory| {
            write_u32(memory, argc_ptr, argc)?;
            write_u32(memory, argv_buf_size_ptr, argv_buf_size)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn args_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let argv_ptr = read_i32_arg(scope, &args, 0)?;
        let argv_buf_ptr = read_i32_arg(scope, &args, 1)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            write_c_string_table(memory, &context.argv, argv_ptr, argv_buf_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn environ_sizes_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let environc_ptr = read_i32_arg(scope, &args, 0)?;
        let environ_buf_size_ptr = read_i32_arg(scope, &args, 1)?;

        let environ = collect_environ();
        let environc = u32::try_from(environ.len()).map_err(|_| WASI_ERRNO_FAULT)?;
        let environ_buf_size = table_bytes_len(&environ)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            write_u32(memory, environc_ptr, environc)?;
            write_u32(memory, environ_buf_size_ptr, environ_buf_size)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn environ_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let environ_ptr = read_i32_arg(scope, &args, 0)?;
        let environ_buf_ptr = read_i32_arg(scope, &args, 1)?;
        let context = callback_context(&args);

        let environ = collect_environ();
        with_wasi_memory_mut(scope, context, |memory| {
            write_c_string_table(memory, &environ, environ_ptr, environ_buf_ptr)
        })
    })();

    finish_with_result(&mut ret, result);
}

fn random_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let buf_ptr = read_i32_arg(scope, &args, 0)?;
        let buf_len =
            usize::try_from(read_i32_arg(scope, &args, 1)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            let buf = checked_mut_range(memory, ptr_to_offset(buf_ptr)?, buf_len)?;
            rand::thread_rng().fill_bytes(buf);
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn proc_exit(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _ret: v8::ReturnValue,
) {
    let code = args.get(0).uint32_value(scope).unwrap_or(1);
    std::process::exit(i32::try_from(code).unwrap_or(1));
}

fn fd_write(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let iovs_ptr = read_i32_arg(scope, &args, 1)?;
        let iovs_len =
            u32::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let nwritten_ptr = read_i32_arg(scope, &args, 3)?;
        let context = callback_context(&args);
        let descriptor = if fd > WASI_FD_STDERR {
            Some(file_for_fd(context, fd)?)
        } else {
            None
        };

        with_wasi_memory_mut(scope, context, |memory| {
            let mut total_written: usize = 0;

            match fd {
                WASI_FD_STDOUT => {
                    let mut stdout = std::io::stdout();
                    for index in 0..iovs_len {
                        let (buf_offset, len) = iovec(memory, iovs_ptr, index)?;
                        let bytes = checked_mut_range(memory, buf_offset, len)?;
                        stdout.write_all(bytes).map_err(|_| WASI_ERRNO_IO)?;
                        total_written = total_written.checked_add(len).ok_or(WASI_ERRNO_FAULT)?;
                    }
                    stdout.flush().map_err(|_| WASI_ERRNO_IO)?;
                }
                WASI_FD_STDERR => {
                    let mut stderr = std::io::stderr();
                    for index in 0..iovs_len {
                        let (buf_offset, len) = iovec(memory, iovs_ptr, index)?;
                        let bytes = checked_mut_range(memory, buf_offset, len)?;
                        stderr.write_all(bytes).map_err(|_| WASI_ERRNO_IO)?;
                        total_written = total_written.checked_add(len).ok_or(WASI_ERRNO_FAULT)?;
                    }
                    stderr.flush().map_err(|_| WASI_ERRNO_IO)?;
                }
                _ => {
                    let descriptor = descriptor.as_ref().ok_or(WASI_ERRNO_BADF)?;
                    let Descriptor::File(file) = descriptor.as_ref() else {
                        return Err(WASI_ERRNO_BADF);
                    };
                    let mut file = file.lock().map_err(|_| WASI_ERRNO_IO)?;
                    for index in 0..iovs_len {
                        let (buf_offset, len) = iovec(memory, iovs_ptr, index)?;
                        let bytes = checked_mut_range(memory, buf_offset, len)?;
                        file.write_all(bytes)
                            .map_err(|error| io_error_to_errno(&error))?;
                        total_written = total_written.checked_add(len).ok_or(WASI_ERRNO_FAULT)?;
                    }
                }
            }

            let nwritten = u32::try_from(total_written).map_err(|_| WASI_ERRNO_FAULT)?;
            write_u32(memory, nwritten_ptr, nwritten)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn fd_read(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let fd = read_i32_arg(scope, &args, 0)?;
        let iovs_ptr = read_i32_arg(scope, &args, 1)?;
        let iovs_len =
            u32::try_from(read_i32_arg(scope, &args, 2)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let nread_ptr = read_i32_arg(scope, &args, 3)?;
        let context = callback_context(&args);
        let descriptor = if fd > WASI_FD_STDERR {
            Some(file_for_fd(context, fd)?)
        } else {
            None
        };

        with_wasi_memory_mut(scope, context, |memory| {
            let mut total_read: usize = 0;
            match fd {
                WASI_FD_STDIN => {
                    let mut stdin = std::io::stdin().lock();
                    for index in 0..iovs_len {
                        let (buf_offset, len) = iovec(memory, iovs_ptr, index)?;
                        if len == 0 {
                            continue;
                        }
                        let buffer = checked_mut_range(memory, buf_offset, len)?;
                        let read_len = stdin.read(buffer).map_err(|_| WASI_ERRNO_IO)?;
                        total_read = total_read.checked_add(read_len).ok_or(WASI_ERRNO_FAULT)?;

                        if read_len < len {
                            break;
                        }
                    }
                }
                _ => {
                    let descriptor = descriptor.as_ref().ok_or(WASI_ERRNO_BADF)?;
                    let Descriptor::File(file) = descriptor.as_ref() else {
                        return Err(WASI_ERRNO_BADF);
                    };
                    let mut file = file.lock().map_err(|_| WASI_ERRNO_IO)?;
                    for index in 0..iovs_len {
                        let (buf_offset, len) = iovec(memory, iovs_ptr, index)?;
                        if len == 0 {
                            continue;
                        }
                        let buffer = checked_mut_range(memory, buf_offset, len)?;
                        let read_len = file
                            .read(buffer)
                            .map_err(|error| io_error_to_errno(&error))?;
                        total_read = total_read.checked_add(read_len).ok_or(WASI_ERRNO_FAULT)?;

                        if read_len < len {
                            break;
                        }
                    }
                }
            }

            let nread = u32::try_from(total_read).map_err(|_| WASI_ERRNO_FAULT)?;
            write_u32(memory, nread_ptr, nread)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn clock_now_ns(context: &WasiContext, clock_id: ClockId) -> WasiResult<u64> {
    match clock_id {
        ClockId::Realtime => {
            let duration = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| WASI_ERRNO_FAULT)?;
            Ok(duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        }
        ClockId::Monotonic | ClockId::ProcessCpuTime | ClockId::ThreadCpuTime => {
            let elapsed = context.monotonic_origin.elapsed();
            Ok(elapsed.as_nanos().min(u128::from(u64::MAX)) as u64)
        }
    }
}

fn clock_resolution_ns(clock_id: ClockId) -> WasiResult<u64> {
    match clock_id {
        ClockId::Realtime
        | ClockId::Monotonic
        | ClockId::ProcessCpuTime
        | ClockId::ThreadCpuTime => Ok(1),
    }
}

fn clock_res_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let clock_id = ClockId::try_from(read_i32_arg(scope, &args, 0)?)?;
        let resolution_ptr = read_i32_arg(scope, &args, 1)?;
        let resolution = clock_resolution_ns(clock_id)?;
        let context = callback_context(&args);

        with_wasi_memory_mut(scope, context, |memory| {
            write_u64(memory, resolution_ptr, resolution)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn clock_time_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let clock_id = ClockId::try_from(read_i32_arg(scope, &args, 0)?)?;
        let time_ptr = read_i32_arg(scope, &args, 2)?;

        let context = callback_context(&args);
        let now_ns = clock_now_ns(context, clock_id)?;

        with_wasi_memory_mut(scope, context, |memory| {
            write_u64(memory, time_ptr, now_ns)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn set_wasi_func_impl<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    context_ptr: *mut std::ffi::c_void,
) {
    let key = scope.string(name);
    let data = v8::External::new(scope, context_ptr);
    let function = v8::Function::builder(callback)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set(scope, key.into(), function.into());
}

macro_rules! set_wasi_func {
    ($obj:expr, $scope:expr, $context_ptr:expr, $callback:ident) => {
        set_wasi_func_impl($obj, $scope, stringify!($callback), $callback, $context_ptr);
    };
}

pub(crate) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    wasm_file_name: &str,
    args: &[String],
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let context = Box::new(WasiContext {
        argv: build_argv(wasm_file_name, args),
        monotonic_origin: Instant::now(),
        preopen_dir_name: preopen_name(),
        preopen_dir_host_path: PathBuf::from("."),
        descriptors: Mutex::new(DescriptorTable::new()),
        memory: OnceLock::new(),
    });
    let context_ptr = &*context as *const WasiContext as *mut std::ffi::c_void;

    set_wasi_func!(obj, scope, context_ptr, set_memory);
    set_wasi_func!(obj, scope, context_ptr, args_get);
    set_wasi_func!(obj, scope, context_ptr, args_sizes_get);
    set_wasi_func!(obj, scope, context_ptr, environ_get);
    set_wasi_func!(obj, scope, context_ptr, environ_sizes_get);
    set_wasi_func!(obj, scope, context_ptr, fd_read);
    set_wasi_func!(obj, scope, context_ptr, fd_write);
    set_wasi_func!(obj, scope, context_ptr, fd_close);
    set_wasi_func!(obj, scope, context_ptr, fd_filestat_get);
    set_wasi_func!(obj, scope, context_ptr, fd_prestat_get);
    set_wasi_func!(obj, scope, context_ptr, fd_prestat_dir_name);
    set_wasi_func!(obj, scope, context_ptr, fd_readdir);
    set_wasi_func!(obj, scope, context_ptr, path_open);
    set_wasi_func!(obj, scope, context_ptr, path_readlink);
    set_wasi_func!(obj, scope, context_ptr, path_rename);
    set_wasi_func!(obj, scope, context_ptr, path_create_directory);
    set_wasi_func!(obj, scope, context_ptr, path_filestat_get);
    set_wasi_func!(obj, scope, context_ptr, path_remove_directory);
    set_wasi_func!(obj, scope, context_ptr, path_unlink_file);
    set_wasi_func!(obj, scope, context_ptr, random_get);
    set_wasi_func!(obj, scope, context_ptr, proc_exit);
    set_wasi_func!(obj, scope, context_ptr, clock_res_get);
    set_wasi_func!(obj, scope, context_ptr, clock_time_get);

    dtors.push(context);
}
