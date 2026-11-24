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

use super::os::{RawFile, watch};
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::ported_fns;
pub(crate) use watch::create;

const EVENT_COUNT: usize = 1024;
const EVENT_SIZE: usize = std::mem::size_of::<libc::kevent>();

ported_fns! {
    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_buffer_size"
    )]
    pub(crate) fn buffer_size() -> u32 {
        u32::try_from(EVENT_COUNT * EVENT_SIZE).expect("kqueue watcher buffer size fits u32")
    }

    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_add_file"
    )]
    pub(crate) fn add_file(
        kqueue: RawFile,
        file: RawFile,
        _is_dir: bool,
        file_handle: u64,
    ) -> AsyncHostResult<()> {
        let token = usize::try_from(file_handle).map_err(|_| AsyncHostError::Fault)?;
        Ok(watch::add_file(kqueue, file, token)?)
    }

    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_fetch_event"
    )]
    pub(crate) fn fetch_event(
        kqueue: RawFile,
        buffer: &mut [u8],
        len: u32,
    ) -> AsyncHostResult<i32> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let buffer = buffer.get_mut(..len).ok_or(AsyncHostError::Fault)?;
        let event_count = buffer.len() / EVENT_SIZE;
        let event_count = i32::try_from(event_count).map_err(|_| AsyncHostError::Fault)?;
        // The checked slice holds event_count complete records. kevent writes
        // native records here; the guest accessors read them unaligned.
        Ok(unsafe { watch::fetch_event(kqueue, buffer.as_mut_ptr().cast(), event_count)? })
    }

    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_event_get_fd"
    )]
    pub(crate) fn event_get_fd(buffer: &[u8], index: u32) -> AsyncHostResult<u64> {
        let event = event_at(buffer, index)?;
        let handle = event.udata as usize;
        if handle == 0 {
            Ok(event.ident as u64)
        } else {
            Ok(handle as u64)
        }
    }

    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_event_has_modify"
    )]
    pub(crate) fn event_has_modify(buffer: &[u8], index: u32) -> AsyncHostResult<bool> {
        Ok(event_at(buffer, index)?.fflags & (libc::NOTE_WRITE | libc::NOTE_EXTEND) != 0)
    }
}

fn event_at(buffer: &[u8], index: u32) -> AsyncHostResult<libc::kevent> {
    let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
    let offset = index.checked_mul(EVENT_SIZE).ok_or(AsyncHostError::Fault)?;
    let end = offset
        .checked_add(EVENT_SIZE)
        .ok_or(AsyncHostError::Fault)?;
    if end > buffer.len() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(offset).cast()) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    #[test]
    fn extending_a_file_reports_only_the_write_event() {
        let kqueue = unsafe { OwnedFd::from_raw_fd(create().unwrap()) };
        let mut file = tempfile::tempfile().unwrap();
        add_file(kqueue.as_raw_fd(), file.as_raw_fd(), false, 7).unwrap();

        file.write_all(b"extended").unwrap();
        let mut buffer = vec![0; buffer_size() as usize];
        assert_eq!(
            fetch_event(kqueue.as_raw_fd(), &mut buffer, buffer_size()),
            Ok(1)
        );
        assert_eq!(event_get_fd(&buffer, 0), Ok(7));
        assert_eq!(event_has_modify(&buffer, 0), Ok(true));
        assert_eq!(event_at(&buffer, 0).unwrap().fflags, libc::NOTE_WRITE);

        assert_eq!(
            fetch_event(kqueue.as_raw_fd(), &mut buffer, buffer_size()),
            Ok(0)
        );
    }

    #[test]
    fn invalid_buffer_lengths_are_rejected_before_native_reads() {
        assert_eq!(fetch_event(-1, &mut [], 1), Err(AsyncHostError::Fault));
        assert_eq!(
            fetch_event(-1, &mut [], 0),
            Err(AsyncHostError::Native(libc::EBADF))
        );
    }
}
