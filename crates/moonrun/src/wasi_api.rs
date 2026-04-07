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
use std::io::{ErrorKind, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

type WasiErrno = i32;
type WasiResult<T> = Result<T, WasiErrno>;

const WASI_ERRNO_SUCCESS: WasiErrno = 0;
const WASI_ERRNO_AGAIN: WasiErrno = 6;
const WASI_ERRNO_ACCESS: WasiErrno = 2;
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
const WASI_ERRNO_NOTSUP: WasiErrno = 58;
const WASI_ERRNO_PIPE: WasiErrno = 64;
const WASI_ERRNO_NOTCAPABLE: WasiErrno = 76;

const WASI_FD_STDIN: i32 = 0;
const WASI_FD_STDOUT: i32 = 1;
const WASI_FD_STDERR: i32 = 2;
const WASI_FD_PREOPEN_DIR: i32 = 3;
const WASI_FD_DYNAMIC_START: i32 = 4;
const WASI_IOVEC_SIZE: usize = 8;
const WASI_SUBSCRIPTION_SIZE: usize = 48;
const WASI_EVENT_SIZE: usize = 32;
const WASI_DIRENT_SIZE: usize = 24;

const WASI_PREOPEN_TYPE_DIR: u8 = 0;
const WASI_FILETYPE_UNKNOWN: u8 = 0;
const WASI_FILETYPE_CHARACTER_DEVICE: u8 = 2;
const WASI_FILETYPE_DIRECTORY: u8 = 3;
const WASI_FILETYPE_REGULAR_FILE: u8 = 4;
const WASI_FILETYPE_SYMBOLIC_LINK: u8 = 7;

const WASI_FDFLAG_APPEND: i32 = 1;
const WASI_FDFLAG_DSYNC: i32 = 2;
const WASI_FDFLAG_NONBLOCK: i32 = 4;
const WASI_FDFLAG_RSYNC: i32 = 8;
const WASI_FDFLAG_SYNC: i32 = 16;
const WASI_KNOWN_FDFLAGS_MASK: i32 = WASI_FDFLAG_APPEND
    | WASI_FDFLAG_DSYNC
    | WASI_FDFLAG_NONBLOCK
    | WASI_FDFLAG_RSYNC
    | WASI_FDFLAG_SYNC;
const WASI_SUPPORTED_FDFLAGS_MASK: i32 = WASI_FDFLAG_APPEND;

const WASI_OFLAGS_CREAT: i32 = 1;
const WASI_OFLAGS_DIRECTORY: i32 = 2;
const WASI_OFLAGS_EXCL: i32 = 4;
const WASI_OFLAGS_TRUNC: i32 = 8;

const WASI_LOOKUPFLAG_SYMLINK_FOLLOW: i32 = 1;
const WASI_KNOWN_LOOKUPFLAGS_MASK: i32 = WASI_LOOKUPFLAG_SYMLINK_FOLLOW;
const WASI_KNOWN_OFLAGS_MASK: i32 =
    WASI_OFLAGS_CREAT | WASI_OFLAGS_DIRECTORY | WASI_OFLAGS_EXCL | WASI_OFLAGS_TRUNC;

const WASI_RIGHT_FD_READ: u64 = 1u64 << 1;
const WASI_RIGHT_FD_WRITE: u64 = 1u64 << 6;
const WASI_RIGHT_PATH_CREATE_DIRECTORY: u64 = 1u64 << 9;
const WASI_RIGHT_PATH_OPEN: u64 = 1u64 << 13;
const WASI_RIGHT_FD_READDIR: u64 = 1u64 << 14;
const WASI_RIGHT_PATH_READLINK: u64 = 1u64 << 15;
const WASI_RIGHT_PATH_RENAME_SOURCE: u64 = 1u64 << 16;
const WASI_RIGHT_PATH_RENAME_TARGET: u64 = 1u64 << 17;
const WASI_RIGHT_PATH_FILESTAT_GET: u64 = 1u64 << 18;
const WASI_RIGHT_FD_FILESTAT_GET: u64 = 1u64 << 21;
const WASI_RIGHT_PATH_REMOVE_DIRECTORY: u64 = 1u64 << 25;
const WASI_RIGHT_PATH_UNLINK_FILE: u64 = 1u64 << 26;
const WASI_KNOWN_RIGHTS_MASK: u64 = (1u64 << 30) - 1;

const WASI_EVENTTYPE_CLOCK: u16 = 0;
const WASI_EVENTTYPE_FD_READ: u16 = 1;
const WASI_EVENTTYPE_FD_WRITE: u16 = 2;

const WASI_SUBSCRIPTION_TAG_CLOCK: u32 = 0;
const WASI_SUBSCRIPTION_TAG_FD_READ: u32 = 1;
const WASI_SUBSCRIPTION_TAG_FD_WRITE: u32 = 2;
const WASI_SUBCLOCKFLAG_ABSTIME: u32 = 1;

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

enum DescriptorKind {
    File(Mutex<fs::File>),
    Directory(PathBuf),
}

struct Descriptor {
    kind: DescriptorKind,
    rights_base: u64,
    rights_inheriting: u64,
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
    preopen_dir_real_path: PathBuf,
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

enum SubscriptionData {
    Clock {
        id: ClockId,
        timeout_ns: u64,
        flags: u32,
    },
    FdRead {
        fd: i32,
    },
    FdWrite {
        fd: i32,
    },
}

struct PollSubscription {
    userdata: u64,
    data: SubscriptionData,
}

struct PollEvent {
    userdata: u64,
    error: WasiErrno,
    event_type: u16,
    nbytes: u64,
    flags: u32,
}

enum FdReadPollState {
    Ready(u64),
    Pending,
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

fn write_u16_at(memory: &mut [u8], offset: usize, value: u16) -> WasiResult<()> {
    checked_mut_range(memory, offset, 2)?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn read_u16_at(memory: &[u8], offset: usize) -> WasiResult<u16> {
    let end = offset.checked_add(2).ok_or(WASI_ERRNO_FAULT)?;
    let bytes = memory.get(offset..end).ok_or(WASI_ERRNO_FAULT)?;
    Ok(u16::from_le_bytes(
        <[u8; 2]>::try_from(bytes).map_err(|_| WASI_ERRNO_FAULT)?,
    ))
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

fn read_u64_at(memory: &[u8], offset: usize) -> WasiResult<u64> {
    let end = offset.checked_add(8).ok_or(WASI_ERRNO_FAULT)?;
    let bytes = memory.get(offset..end).ok_or(WASI_ERRNO_FAULT)?;
    Ok(u64::from_le_bytes(
        <[u8; 8]>::try_from(bytes).map_err(|_| WASI_ERRNO_FAULT)?,
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
    let path = if preopen_matches(context, fd) {
        context.preopen_dir_host_path.clone()
    } else {
        let descriptor = descriptor_for_fd(context, fd)?;
        match &descriptor.kind {
            DescriptorKind::Directory(path) => path.clone(),
            _ => return Err(WASI_ERRNO_BADF),
        }
    };

    enforce_preopen_boundary(
        context,
        &path,
        PathBoundaryMode::FollowFinal {
            allow_missing_final: false,
        },
    )?;
    Ok(path)
}

fn file_for_fd(context: &WasiContext, fd: i32) -> WasiResult<Arc<Descriptor>> {
    let descriptor = descriptor_for_fd(context, fd)?;
    if matches!(&descriptor.kind, DescriptorKind::File(_)) {
        Ok(descriptor)
    } else {
        Err(WASI_ERRNO_BADF)
    }
}

fn validate_known_flag_bits(value: i32, mask: i32) -> WasiResult<()> {
    if (value & !mask) != 0 {
        Err(WASI_ERRNO_INVAL)
    } else {
        Ok(())
    }
}

fn validate_supported_flag_bits(value: i32, supported: i32) -> WasiResult<()> {
    if (value & !supported) != 0 {
        Err(WASI_ERRNO_NOTSUP)
    } else {
        Ok(())
    }
}

fn validate_known_rights(rights: u64) -> WasiResult<()> {
    if (rights & !WASI_KNOWN_RIGHTS_MASK) != 0 {
        Err(WASI_ERRNO_INVAL)
    } else {
        Ok(())
    }
}

fn preopen_rights_base() -> u64 {
    WASI_RIGHT_PATH_OPEN
        | WASI_RIGHT_FD_READDIR
        | WASI_RIGHT_PATH_READLINK
        | WASI_RIGHT_PATH_RENAME_SOURCE
        | WASI_RIGHT_PATH_RENAME_TARGET
        | WASI_RIGHT_PATH_FILESTAT_GET
        | WASI_RIGHT_PATH_CREATE_DIRECTORY
        | WASI_RIGHT_PATH_REMOVE_DIRECTORY
        | WASI_RIGHT_PATH_UNLINK_FILE
        | WASI_RIGHT_FD_FILESTAT_GET
}

fn rights_base_for_fd(context: &WasiContext, fd: i32) -> WasiResult<u64> {
    match fd {
        WASI_FD_STDIN => Ok(WASI_RIGHT_FD_READ | WASI_RIGHT_FD_FILESTAT_GET),
        WASI_FD_STDOUT | WASI_FD_STDERR => Ok(WASI_RIGHT_FD_WRITE | WASI_RIGHT_FD_FILESTAT_GET),
        _ if preopen_matches(context, fd) => Ok(preopen_rights_base()),
        _ => Ok(descriptor_for_fd(context, fd)?.rights_base),
    }
}

fn rights_inheriting_for_fd(context: &WasiContext, fd: i32) -> WasiResult<u64> {
    match fd {
        WASI_FD_STDIN | WASI_FD_STDOUT | WASI_FD_STDERR => Ok(0),
        _ if preopen_matches(context, fd) => Ok(WASI_KNOWN_RIGHTS_MASK),
        _ => Ok(descriptor_for_fd(context, fd)?.rights_inheriting),
    }
}

fn require_fd_right(context: &WasiContext, fd: i32, right: u64) -> WasiResult<()> {
    let rights = rights_base_for_fd(context, fd)?;
    if (rights & right) == right {
        Ok(())
    } else {
        Err(WASI_ERRNO_NOTCAPABLE)
    }
}

#[cfg(unix)]
fn guest_path_from_bytes(path: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(std::ffi::OsString::from_vec(path.to_vec()))
}

#[cfg(not(unix))]
fn guest_path_from_bytes(path: &[u8]) -> WasiResult<PathBuf> {
    let path = std::str::from_utf8(path).map_err(|_| WASI_ERRNO_INVAL)?;
    Ok(PathBuf::from(path))
}

fn read_path_from_memory(memory: &[u8], path_ptr: i32, path_len: usize) -> WasiResult<PathBuf> {
    let path = checked_range(memory, ptr_to_offset(path_ptr)?, path_len)?;
    if path.contains(&0) {
        return Err(WASI_ERRNO_INVAL);
    }
    #[cfg(unix)]
    {
        Ok(guest_path_from_bytes(path))
    }
    #[cfg(not(unix))]
    {
        guest_path_from_bytes(path)
    }
}

fn validate_guest_relative_path(path: &Path) -> WasiResult<()> {
    let mut depth = 0i32;
    for component in path.components() {
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

fn resolve_path(context: &WasiContext, dirfd: i32, path: &Path) -> WasiResult<PathBuf> {
    validate_guest_relative_path(path)?;
    let base = dir_path_for_fd(context, dirfd)?;
    let relative = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };
    Ok(base.join(relative))
}

fn io_error_to_errno(error: &std::io::Error) -> WasiErrno {
    match error.kind() {
        ErrorKind::NotFound => WASI_ERRNO_NOENT,
        ErrorKind::PermissionDenied => WASI_ERRNO_ACCESS,
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

enum PathBoundaryMode {
    FollowFinal { allow_missing_final: bool },
    ParentOnly,
}

fn canonical_parent_path(path: &Path) -> WasiResult<PathBuf> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::canonicalize(parent).map_err(|error| io_error_to_errno(&error))
}

fn ensure_within_preopen_root(context: &WasiContext, path: &Path) -> WasiResult<()> {
    if path.starts_with(&context.preopen_dir_real_path) {
        Ok(())
    } else {
        Err(WASI_ERRNO_NOTCAPABLE)
    }
}

fn enforce_preopen_boundary(
    context: &WasiContext,
    path: &Path,
    mode: PathBoundaryMode,
) -> WasiResult<()> {
    match mode {
        PathBoundaryMode::FollowFinal {
            allow_missing_final,
        } => match fs::canonicalize(path) {
            Ok(real_path) => ensure_within_preopen_root(context, &real_path),
            Err(error) if error.kind() == ErrorKind::NotFound && allow_missing_final => {
                match fs::symlink_metadata(path) {
                    Ok(metadata) if metadata.file_type().is_symlink() => Err(WASI_ERRNO_NOTCAPABLE),
                    Ok(_) => Err(WASI_ERRNO_NOENT),
                    Err(metadata_error) if metadata_error.kind() == ErrorKind::NotFound => {
                        let parent_path = canonical_parent_path(path)?;
                        ensure_within_preopen_root(context, &parent_path)
                    }
                    Err(metadata_error) => Err(io_error_to_errno(&metadata_error)),
                }
            }
            Err(error) => Err(io_error_to_errno(&error)),
        },
        PathBoundaryMode::ParentOnly => {
            let parent_path = canonical_parent_path(path)?;
            ensure_within_preopen_root(context, &parent_path)
        }
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
        #[cfg(unix)]
        let name = {
            use std::os::unix::ffi::OsStrExt;
            entry.file_name().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
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
            let dir_path = dir_path_for_fd(context, fd)?;
            let metadata = fs::metadata(&dir_path).map_err(|error| io_error_to_errno(&error))?;
            Ok(stat_data_for_metadata(&metadata))
        }
        _ => {
            let descriptor = descriptor_for_fd(context, fd)?;
            match &descriptor.kind {
                DescriptorKind::Directory(_) => {
                    let dir_path = dir_path_for_fd(context, fd)?;
                    let metadata =
                        fs::metadata(&dir_path).map_err(|error| io_error_to_errno(&error))?;
                    Ok(stat_data_for_metadata(&metadata))
                }
                DescriptorKind::File(file) => {
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
        require_fd_right(context, fd, WASI_RIGHT_FD_FILESTAT_GET)?;
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
        let dirflags = read_i32_arg(scope, &args, 1)?;
        let path_ptr = read_i32_arg(scope, &args, 2)?;
        let path_len =
            usize::try_from(read_i32_arg(scope, &args, 3)?).map_err(|_| WASI_ERRNO_INVAL)?;
        let oflags = read_i32_arg(scope, &args, 4)?;
        let rights_base = read_u64_arg(scope, &args, 5)?;
        let rights_inheriting = read_u64_arg(scope, &args, 6)?;
        let fdflags = read_i32_arg(scope, &args, 7)?;
        let opened_fd_ptr = read_i32_arg(scope, &args, 8)?;
        let context = callback_context(&args);

        validate_known_flag_bits(dirflags, WASI_KNOWN_LOOKUPFLAGS_MASK)?;
        validate_known_flag_bits(oflags, WASI_KNOWN_OFLAGS_MASK)?;
        validate_known_flag_bits(fdflags, WASI_KNOWN_FDFLAGS_MASK)?;
        validate_supported_flag_bits(fdflags, WASI_SUPPORTED_FDFLAGS_MASK)?;
        validate_known_rights(rights_base)?;
        validate_known_rights(rights_inheriting)?;

        require_fd_right(context, dirfd, WASI_RIGHT_PATH_OPEN)?;
        let parent_inheriting_rights = rights_inheriting_for_fd(context, dirfd)?;
        if (rights_base & !parent_inheriting_rights) != 0 {
            return Err(WASI_ERRNO_NOTCAPABLE);
        }
        if (rights_inheriting & !parent_inheriting_rights) != 0 {
            return Err(WASI_ERRNO_NOTCAPABLE);
        }

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        let create_requested = (oflags & WASI_OFLAGS_CREAT) != 0;
        enforce_preopen_boundary(
            context,
            &full_path,
            PathBoundaryMode::FollowFinal {
                allow_missing_final: create_requested,
            },
        )?;

        let descriptor_kind = if (oflags & WASI_OFLAGS_DIRECTORY) != 0 {
            if (oflags & (WASI_OFLAGS_CREAT | WASI_OFLAGS_EXCL | WASI_OFLAGS_TRUNC)) != 0 {
                return Err(WASI_ERRNO_INVAL);
            }
            let metadata = fs::metadata(&full_path).map_err(|error| io_error_to_errno(&error))?;
            if !metadata.is_dir() {
                return Err(WASI_ERRNO_NOTDIR);
            }
            DescriptorKind::Directory(full_path)
        } else {
            let wants_read = (rights_base & WASI_RIGHT_FD_READ) != 0;
            let wants_write = (rights_base & WASI_RIGHT_FD_WRITE) != 0;
            let append = (fdflags & WASI_FDFLAG_APPEND) != 0;

            let mut options = fs::OpenOptions::new();
            options.read(wants_read || !wants_write);
            // Rust requires write/append when O_CREAT is set. Keep rights enforcement at WASI level.
            options.write(wants_write || append || create_requested);
            options.append(append);
            options.create(create_requested);
            options.create_new((oflags & WASI_OFLAGS_EXCL) != 0);
            options.truncate((oflags & WASI_OFLAGS_TRUNC) != 0);

            let file = options
                .open(full_path)
                .map_err(|error| io_error_to_errno(&error))?;
            DescriptorKind::File(Mutex::new(file))
        };

        let descriptor = Arc::new(Descriptor {
            kind: descriptor_kind,
            rights_base,
            rights_inheriting,
        });

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
        require_fd_right(context, dirfd, WASI_RIGHT_PATH_READLINK)?;

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        enforce_preopen_boundary(context, &full_path, PathBoundaryMode::ParentOnly)?;
        let link_target = fs::read_link(&full_path).map_err(|error| io_error_to_errno(&error))?;
        #[cfg(unix)]
        let link_bytes = {
            use std::os::unix::ffi::OsStrExt;
            link_target.as_os_str().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
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
        require_fd_right(context, dirfd, WASI_RIGHT_PATH_CREATE_DIRECTORY)?;

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        enforce_preopen_boundary(context, &full_path, PathBoundaryMode::ParentOnly)?;
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
        require_fd_right(context, old_fd, WASI_RIGHT_PATH_RENAME_SOURCE)?;
        require_fd_right(context, new_fd, WASI_RIGHT_PATH_RENAME_TARGET)?;

        let old_path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, old_path_ptr, old_path_len)
        })?;
        let new_path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, new_path_ptr, new_path_len)
        })?;

        let old_host_path = resolve_path(context, old_fd, &old_path)?;
        let new_host_path = resolve_path(context, new_fd, &new_path)?;
        enforce_preopen_boundary(context, &old_host_path, PathBoundaryMode::ParentOnly)?;
        enforce_preopen_boundary(context, &new_host_path, PathBoundaryMode::ParentOnly)?;
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
        validate_known_flag_bits(flags, WASI_KNOWN_LOOKUPFLAGS_MASK)?;
        require_fd_right(context, dirfd, WASI_RIGHT_PATH_FILESTAT_GET)?;

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        let boundary_mode = if (flags & WASI_LOOKUPFLAG_SYMLINK_FOLLOW) != 0 {
            PathBoundaryMode::FollowFinal {
                allow_missing_final: false,
            }
        } else {
            PathBoundaryMode::ParentOnly
        };
        enforce_preopen_boundary(context, &full_path, boundary_mode)?;
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
        require_fd_right(context, dirfd, WASI_RIGHT_PATH_REMOVE_DIRECTORY)?;

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        enforce_preopen_boundary(context, &full_path, PathBoundaryMode::ParentOnly)?;
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
        require_fd_right(context, dirfd, WASI_RIGHT_PATH_UNLINK_FILE)?;

        let path = with_wasi_memory_mut(scope, context, |memory| {
            read_path_from_memory(memory, path_ptr, path_len)
        })?;
        let full_path = resolve_path(context, dirfd, &path)?;
        enforce_preopen_boundary(context, &full_path, PathBoundaryMode::ParentOnly)?;
        fs::remove_file(full_path).map_err(|error| io_error_to_errno(&error))
    })();

    finish_with_result(&mut ret, result);
}

fn parse_poll_subscriptions(
    memory: &[u8],
    in_ptr: i32,
    nsubscriptions: usize,
) -> WasiResult<Vec<PollSubscription>> {
    let base = ptr_to_offset(in_ptr)?;
    let mut subscriptions = Vec::with_capacity(nsubscriptions);

    for index in 0..nsubscriptions {
        let offset = base
            .checked_add(
                index
                    .checked_mul(WASI_SUBSCRIPTION_SIZE)
                    .ok_or(WASI_ERRNO_FAULT)?,
            )
            .ok_or(WASI_ERRNO_FAULT)?;
        let userdata = read_u64_at(memory, offset)?;
        let tag = u32::from(checked_range(memory, offset + 8, 1)?[0]);

        let data = match tag {
            WASI_SUBSCRIPTION_TAG_CLOCK => {
                let clock_id_raw = read_u32_at(memory, offset + 16)?;
                let clock_id =
                    ClockId::try_from(i32::try_from(clock_id_raw).map_err(|_| WASI_ERRNO_INVAL)?)?;
                let timeout_ns = read_u64_at(memory, offset + 24)?;
                let _precision = read_u64_at(memory, offset + 32)?;
                let flags = u32::from(read_u16_at(memory, offset + 40)?);
                if (flags & !WASI_SUBCLOCKFLAG_ABSTIME) != 0 {
                    return Err(WASI_ERRNO_INVAL);
                }
                SubscriptionData::Clock {
                    id: clock_id,
                    timeout_ns,
                    flags,
                }
            }
            WASI_SUBSCRIPTION_TAG_FD_READ => {
                let fd_raw = read_u32_at(memory, offset + 16)?;
                let fd = i32::try_from(fd_raw).map_err(|_| WASI_ERRNO_INVAL)?;
                SubscriptionData::FdRead { fd }
            }
            WASI_SUBSCRIPTION_TAG_FD_WRITE => {
                let fd_raw = read_u32_at(memory, offset + 16)?;
                let fd = i32::try_from(fd_raw).map_err(|_| WASI_ERRNO_INVAL)?;
                SubscriptionData::FdWrite { fd }
            }
            _ => return Err(WASI_ERRNO_INVAL),
        };

        subscriptions.push(PollSubscription { userdata, data });
    }

    Ok(subscriptions)
}

fn normalize_poll_subscriptions(
    context: &WasiContext,
    subscriptions: &mut [PollSubscription],
) -> WasiResult<()> {
    for subscription in subscriptions {
        if let SubscriptionData::Clock {
            id,
            timeout_ns,
            flags,
        } = &mut subscription.data
            && (*flags & WASI_SUBCLOCKFLAG_ABSTIME) == 0
        {
            let now = clock_now_ns(context, *id)?;
            *timeout_ns = now.saturating_add(*timeout_ns);
            *flags |= WASI_SUBCLOCKFLAG_ABSTIME;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn poll_stdin_with_timeout(timeout: Option<Duration>) -> WasiResult<bool> {
    let timeout_ms = match timeout {
        Some(duration) => {
            let millis = duration.as_millis();
            i32::try_from(millis.min(i32::MAX as u128)).map_err(|_| WASI_ERRNO_INVAL)?
        }
        None => -1,
    };

    let mut pollfd = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        // SAFETY: `pollfd` points to a valid single-element array for the duration of the call.
        let ready_count = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
        if ready_count >= 0 {
            return Ok(ready_count > 0
                && (pollfd.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR)) != 0);
        }

        let error = std::io::Error::last_os_error();
        if error.kind() == ErrorKind::Interrupted {
            continue;
        }
        if let Some(code) = error.raw_os_error()
            && code == libc::EPERM
        {
            return Ok(true);
        }
        return Err(io_error_to_errno(&error));
    }
}

#[cfg(windows)]
fn poll_stdin_with_timeout(timeout: Option<Duration>) -> WasiResult<bool> {
    use windows_sys::Win32::Foundation::{
        INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_INPUT_HANDLE};
    use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};

    let timeout_ms = match timeout {
        Some(duration) => {
            let millis = duration.as_millis();
            u32::try_from(millis.min(u32::MAX as u128)).map_err(|_| WASI_ERRNO_INVAL)?
        }
        None => INFINITE,
    };

    // SAFETY: We only query the process standard-input handle; no ownership is transferred.
    let stdin_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if stdin_handle.is_null() || stdin_handle == INVALID_HANDLE_VALUE {
        return Err(WASI_ERRNO_IO);
    }

    // SAFETY: `stdin_handle` is a handle value obtained from `GetStdHandle`.
    let wait_result = unsafe { WaitForSingleObject(stdin_handle, timeout_ms) };
    match wait_result {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => {
            let error = std::io::Error::last_os_error();
            Err(io_error_to_errno(&error))
        }
        _ => Ok(true),
    }
}

#[cfg(not(any(unix, windows)))]
fn poll_stdin_with_timeout(timeout: Option<Duration>) -> WasiResult<bool> {
    if let Some(duration) = timeout
        && duration > Duration::ZERO
    {
        thread::sleep(duration);
    }
    Ok(true)
}

fn poll_fd_read_state(context: &WasiContext, fd: i32) -> WasiResult<FdReadPollState> {
    match fd {
        WASI_FD_STDIN => {
            if poll_stdin_with_timeout(Some(Duration::ZERO))? {
                Ok(FdReadPollState::Ready(1))
            } else {
                Ok(FdReadPollState::Pending)
            }
        }
        WASI_FD_STDOUT | WASI_FD_STDERR => Err(WASI_ERRNO_BADF),
        _ => {
            let descriptor = file_for_fd(context, fd)?;
            let DescriptorKind::File(file) = &descriptor.kind else {
                return Err(WASI_ERRNO_BADF);
            };
            let mut file = file.lock().map_err(|_| WASI_ERRNO_IO)?;
            let metadata = file.metadata().map_err(|error| io_error_to_errno(&error))?;
            if metadata.is_file() {
                let position = file
                    .stream_position()
                    .map_err(|error| io_error_to_errno(&error))?;
                let remaining = metadata.len().saturating_sub(position);
                Ok(FdReadPollState::Ready(remaining))
            } else {
                Ok(FdReadPollState::Ready(metadata.len().max(1)))
            }
        }
    }
}

fn poll_fd_write_nbytes(context: &WasiContext, fd: i32) -> WasiResult<u64> {
    match fd {
        WASI_FD_STDIN => Err(WASI_ERRNO_BADF),
        WASI_FD_STDOUT | WASI_FD_STDERR => Ok(64 * 1024),
        _ => {
            let descriptor = file_for_fd(context, fd)?;
            let DescriptorKind::File(_) = &descriptor.kind else {
                return Err(WASI_ERRNO_BADF);
            };
            Ok(64 * 1024)
        }
    }
}

fn collect_poll_events(
    context: &WasiContext,
    subscriptions: &[PollSubscription],
) -> WasiResult<(Vec<PollEvent>, Option<u64>, bool)> {
    let mut events = Vec::new();
    let mut min_remaining_ns: Option<u64> = None;
    let mut pending_stdin_read = false;

    for subscription in subscriptions {
        match subscription.data {
            SubscriptionData::Clock {
                id,
                timeout_ns,
                flags,
            } => {
                let now = clock_now_ns(context, id)?;
                let deadline = if (flags & WASI_SUBCLOCKFLAG_ABSTIME) != 0 {
                    timeout_ns
                } else {
                    now.saturating_add(timeout_ns)
                };

                if now >= deadline {
                    events.push(PollEvent {
                        userdata: subscription.userdata,
                        error: WASI_ERRNO_SUCCESS,
                        event_type: WASI_EVENTTYPE_CLOCK,
                        nbytes: 0,
                        flags: 0,
                    });
                } else {
                    let remaining = deadline - now;
                    min_remaining_ns = Some(match min_remaining_ns {
                        Some(current) => current.min(remaining),
                        None => remaining,
                    });
                }
            }
            SubscriptionData::FdRead { fd } => {
                let read_result = (|| -> WasiResult<FdReadPollState> {
                    require_fd_right(context, fd, WASI_RIGHT_FD_READ)?;
                    poll_fd_read_state(context, fd)
                })();
                match read_result {
                    Ok(FdReadPollState::Ready(nbytes)) => {
                        events.push(PollEvent {
                            userdata: subscription.userdata,
                            error: WASI_ERRNO_SUCCESS,
                            event_type: WASI_EVENTTYPE_FD_READ,
                            nbytes,
                            flags: 0,
                        });
                    }
                    Ok(FdReadPollState::Pending) => {
                        pending_stdin_read = true;
                    }
                    Err(error) => {
                        events.push(PollEvent {
                            userdata: subscription.userdata,
                            error,
                            event_type: WASI_EVENTTYPE_FD_READ,
                            nbytes: 0,
                            flags: 0,
                        });
                    }
                }
            }
            SubscriptionData::FdWrite { fd } => {
                let write_result = (|| -> WasiResult<u64> {
                    require_fd_right(context, fd, WASI_RIGHT_FD_WRITE)?;
                    poll_fd_write_nbytes(context, fd)
                })();
                match write_result {
                    Ok(nbytes) => {
                        events.push(PollEvent {
                            userdata: subscription.userdata,
                            error: WASI_ERRNO_SUCCESS,
                            event_type: WASI_EVENTTYPE_FD_WRITE,
                            nbytes,
                            flags: 0,
                        });
                    }
                    Err(error) => {
                        events.push(PollEvent {
                            userdata: subscription.userdata,
                            error,
                            event_type: WASI_EVENTTYPE_FD_WRITE,
                            nbytes: 0,
                            flags: 0,
                        });
                    }
                }
            }
        }
    }

    Ok((events, min_remaining_ns, pending_stdin_read))
}

#[allow(dead_code)]
fn poll_oneoff_impl(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let result = (|| -> WasiResult<()> {
        let in_ptr = read_i32_arg(scope, &args, 0)?;
        let out_ptr = read_i32_arg(scope, &args, 1)?;
        let nsubscriptions_i32 = read_i32_arg(scope, &args, 2)?;
        let nevents_ptr = read_i32_arg(scope, &args, 3)?;
        if nsubscriptions_i32 <= 0 {
            return Err(WASI_ERRNO_INVAL);
        }
        let nsubscriptions = usize::try_from(nsubscriptions_i32).map_err(|_| WASI_ERRNO_INVAL)?;
        let context = callback_context(&args);

        let mut subscriptions = with_wasi_memory_mut(scope, context, |memory| {
            parse_poll_subscriptions(memory, in_ptr, nsubscriptions)
        })?;
        normalize_poll_subscriptions(context, &mut subscriptions)?;

        let (mut events, mut min_remaining_ns, mut pending_stdin_read) =
            collect_poll_events(context, &subscriptions)?;
        while events.is_empty() {
            if pending_stdin_read {
                let timeout = min_remaining_ns.map(Duration::from_nanos);
                let _ = poll_stdin_with_timeout(timeout)?;
            } else if let Some(wait_ns) = min_remaining_ns {
                if wait_ns > 0 {
                    thread::sleep(Duration::from_nanos(wait_ns));
                }
            } else {
                break;
            }

            (events, min_remaining_ns, pending_stdin_read) =
                collect_poll_events(context, &subscriptions)?;
        }

        with_wasi_memory_mut(scope, context, |memory| {
            let out_base = ptr_to_offset(out_ptr)?;
            for (index, event) in events.iter().enumerate() {
                let offset = out_base
                    .checked_add(index.checked_mul(WASI_EVENT_SIZE).ok_or(WASI_ERRNO_FAULT)?)
                    .ok_or(WASI_ERRNO_FAULT)?;
                checked_mut_range(memory, offset, WASI_EVENT_SIZE)?.fill(0);
                write_u64_at(memory, offset, event.userdata)?;
                let error_code = u16::try_from(event.error).map_err(|_| WASI_ERRNO_FAULT)?;
                write_u16_at(memory, offset + 8, error_code)?;
                write_u16_at(memory, offset + 10, event.event_type)?;
                write_u64_at(memory, offset + 16, event.nbytes)?;
                write_u32_at(memory, offset + 24, event.flags)?;
            }
            let nevents = u32::try_from(events.len()).map_err(|_| WASI_ERRNO_FAULT)?;
            write_u32(memory, nevents_ptr, nevents)?;
            Ok(())
        })
    })();

    finish_with_result(&mut ret, result);
}

fn poll_oneoff(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    finish_with_result(&mut ret, Err(WASI_ERRNO_NOTSUP));
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
        require_fd_right(context, fd, WASI_RIGHT_FD_READDIR)?;
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
        require_fd_right(context, fd, WASI_RIGHT_FD_WRITE)?;
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
                    let DescriptorKind::File(file) = &descriptor.kind else {
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
        require_fd_right(context, fd, WASI_RIGHT_FD_READ)?;
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
                    let DescriptorKind::File(file) = &descriptor.kind else {
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
    // We intentionally expose only wall-clock and monotonic clocks for now.
    // WASI CPU-time clocks are accepted as IDs but currently return EINVAL.
    match clock_id {
        ClockId::Realtime => {
            let duration = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| WASI_ERRNO_FAULT)?;
            Ok(duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        }
        ClockId::Monotonic => {
            let elapsed = context.monotonic_origin.elapsed();
            Ok(elapsed.as_nanos().min(u128::from(u64::MAX)) as u64)
        }
        ClockId::ProcessCpuTime | ClockId::ThreadCpuTime => Err(WASI_ERRNO_INVAL),
    }
}

fn clock_resolution_ns(clock_id: ClockId) -> WasiResult<u64> {
    match clock_id {
        ClockId::Realtime | ClockId::Monotonic => Ok(1),
        ClockId::ProcessCpuTime | ClockId::ThreadCpuTime => Err(WASI_ERRNO_INVAL),
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
    let preopen_dir_host_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let preopen_dir_real_path =
        fs::canonicalize(&preopen_dir_host_path).unwrap_or_else(|_| preopen_dir_host_path.clone());
    let context = Box::new(WasiContext {
        argv: build_argv(wasm_file_name, args),
        monotonic_origin: Instant::now(),
        preopen_dir_name: preopen_name(),
        preopen_dir_host_path,
        preopen_dir_real_path,
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
    set_wasi_func!(obj, scope, context_ptr, poll_oneoff);
    set_wasi_func!(obj, scope, context_ptr, random_get);
    set_wasi_func!(obj, scope, context_ptr, proc_exit);
    set_wasi_func!(obj, scope, context_ptr, clock_res_get);
    set_wasi_func!(obj, scope, context_ptr, clock_time_get);

    dtors.push(context);
}
