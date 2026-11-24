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

//! Linux file notification mechanisms. Callers retain the watcher descriptor.

use super::{RawFile, path_to_cstring};
use crate::async_sys::ported_fns;
use std::{ffi::OsStr, io};

ported_fns! {
    #[ported(source = "src/fs/watch_inotify.c", original = "moonbitlang_async_inotify_create")]
    pub(crate) fn create() -> io::Result<RawFile> {
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(fd)
        }
    }

    // The Job executor tracks inotify_add_watch_job_worker's provenance;
    // this native helper is not a separate guest-import implementation.
    pub(in crate::filesystem) fn add_file(
        inotify: RawFile,
        path: &OsStr,
        is_dir: bool,
    ) -> io::Result<i32> {
        let path = path_to_cstring(path)?;
        let flags = if is_dir {
            libc::IN_CREATE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO | libc::IN_DELETE
        } else {
            libc::IN_MODIFY
        };
        let watch = unsafe { libc::inotify_add_watch(inotify, path.as_ptr(), flags) };
        if watch < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(watch)
        }
    }

    #[ported(source = "src/fs/watch_inotify.c", original = "moonbitlang_async_inotify_remove_file")]
    pub(crate) fn remove_file(inotify: RawFile, wd: i32) -> io::Result<()> {
        if unsafe { libc::inotify_rm_watch(inotify, wd) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    #[ported(source = "src/fs/watch_inotify.c", original = "moonbitlang_async_inotify_fetch_event")]
    pub(in crate::filesystem) fn fetch_event(
        inotify: RawFile,
        buffer: &mut [u8],
    ) -> io::Result<usize> {
        match super::io::read(inotify, buffer) {
            Err(error) if error.raw_os_error() == Some(libc::EAGAIN) => Ok(0),
            result => result,
        }
    }
}
