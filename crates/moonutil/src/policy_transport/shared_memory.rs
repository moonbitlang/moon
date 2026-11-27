// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Darwin shared memory and Windows sections are mappable but do not support
//! ordinary file reads. Keep the byte count inside the object: an intermediary
//! only needs the original decimal handle token, and may outlive the publisher.

use std::{fs::File, io, ptr};

pub(super) use platform::is_non_file_read;

const HEADER: usize = size_of::<u64>();

pub(super) fn create(contents: &[u8]) -> io::Result<File> {
    let len = HEADER + contents.len();
    let file = platform::create(len)?;
    let view = View::map(&file, len, true)?;
    // SAFETY: this fresh writable mapping is private until we return the file.
    unsafe {
        view.addr
            .copy_from_nonoverlapping((contents.len() as u64).to_le_bytes().as_ptr(), HEADER);
        view.addr
            .add(HEADER)
            .copy_from_nonoverlapping(contents.as_ptr(), contents.len());
    }
    Ok(file)
}

pub(super) fn read(file: &File) -> io::Result<Vec<u8>> {
    let header = View::map(file, HEADER, false)?;
    // SAFETY: the view contains the full fixed-size header.
    let len = u64::from_le_bytes(unsafe { header.addr.cast::<[u8; HEADER]>().read_unaligned() });
    let len = usize::try_from(len)
        .ok()
        .and_then(|n| n.checked_add(HEADER))
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or(io::ErrorKind::InvalidData)?;
    let view = View::map(file, len, false)?;
    // SAFETY: map validates that the object covers the entire requested view.
    Ok(unsafe { std::slice::from_raw_parts(view.addr.add(HEADER), len - HEADER) }.to_vec())
}

struct View {
    addr: *mut u8,
    #[cfg(target_os = "macos")]
    len: usize,
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use rustix::{io::Errno, shm};
    use std::os::fd::AsRawFd;

    pub(super) fn create(len: usize) -> io::Result<File> {
        let file = loop {
            // Darwin limits shared memory names to 31 bytes. Exclusive creation
            // and mode 000 keep other openers out before we remove the name.
            let name = format!("/moonrun-{:016x}", rand::random::<u64>());
            match shm::open(
                name.as_str(),
                shm::OFlags::CREATE | shm::OFlags::EXCL | shm::OFlags::RDWR,
                shm::Mode::empty(),
            ) {
                Ok(fd) => {
                    shm::unlink(name.as_str())?;
                    break File::from(fd);
                }
                Err(Errno::EXIST) => continue,
                Err(err) => return Err(err.into()),
            }
        };
        file.set_len(len as u64)?;
        Ok(file)
    }

    pub(in super::super) fn is_non_file_read(err: &io::Error) -> bool {
        err.raw_os_error() == Some(libc::ENXIO)
    }

    impl View {
        pub(super) fn map(file: &File, len: usize, writable: bool) -> io::Result<Self> {
            // Darwin rounds the object size up to a page. mmap can succeed past
            // EOF, so check the backing size before touching any mapped bytes.
            if len as u64 > file.metadata()?.len() {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let prot = if writable {
                libc::PROT_READ | libc::PROT_WRITE
            } else {
                libc::PROT_READ
            };
            let addr = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    len,
                    prot,
                    libc::MAP_SHARED,
                    file.as_raw_fd(),
                    0,
                )
            };
            if addr == libc::MAP_FAILED {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                addr: addr.cast(),
                len,
            })
        }
    }

    impl Drop for View {
        fn drop(&mut self) {
            unsafe { libc::munmap(self.addr.cast(), self.len) };
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use windows_sys::Win32::Foundation::{ERROR_INVALID_HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS,
        MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
    };

    pub(super) fn create(len: usize) -> io::Result<File> {
        let size = len as u64;
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                ptr::null(),
                PAGE_READWRITE,
                (size >> 32) as u32,
                size as u32,
                ptr::null(),
            )
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the section is a newly owned CloseHandle-compatible handle.
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    pub(in super::super) fn is_non_file_read(err: &io::Error) -> bool {
        err.raw_os_error() == Some(ERROR_INVALID_HANDLE as i32)
    }

    impl View {
        pub(super) fn map(file: &File, len: usize, writable: bool) -> io::Result<Self> {
            let access = if writable {
                FILE_MAP_WRITE
            } else {
                FILE_MAP_READ
            };
            // MapViewOfFile rejects views extending past the section.
            let view = unsafe { MapViewOfFile(file.as_raw_handle(), access, 0, 0, len) };
            if view.Value.is_null() {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                addr: view.Value.cast(),
            })
        }
    }

    impl Drop for View {
        fn drop(&mut self) {
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.addr.cast(),
                })
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_transport::PolicyTransfer;

    #[test]
    fn mapped_transfer_preserves_exact_length_across_pages() {
        let mut bytes = vec![42; 256 * 1024];
        bytes.push(0);
        assert_eq!(
            PolicyTransfer::from_bytes(&bytes).unwrap().read().unwrap(),
            bytes
        );
    }

    #[test]
    fn mapped_transfer_rejects_length_larger_than_object() {
        let file = platform::create(HEADER).unwrap();
        let view = View::map(&file, HEADER, true).unwrap();
        // SAFETY: this writable view covers the entire header.
        unsafe {
            view.addr
                .cast::<[u8; HEADER]>()
                .write_unaligned((1_u64 << 30).to_le_bytes())
        };
        assert!(read(&file).is_err());
    }
}
