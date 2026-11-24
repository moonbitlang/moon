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
pub(crate) use watch::{create, remove_file};

const EVENT_HEADER_SIZE: usize = std::mem::size_of::<libc::inotify_event>();
const NAME_MAX: usize = 255;

ported_fns! {
    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_buffer_size"
    )]
    pub(crate) fn event_buffer_size() -> u32 {
        let min_size = EVENT_HEADER_SIZE + NAME_MAX + 1;
        u32::try_from(4096usize.max(min_size)).expect("inotify event buffer size fits u32")
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_fetch_event"
    )]
    pub(crate) fn fetch_event(
        inotify: RawFile,
        buffer: &mut [u8],
        len: u32,
    ) -> AsyncHostResult<i32> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let buffer = buffer.get_mut(..len).ok_or(AsyncHostError::Fault)?;
        i32::try_from(watch::fetch_event(inotify, buffer)?).map_err(|_| AsyncHostError::Fault)
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_get_size"
    )]
    pub(crate) fn event_get_size(buffer: &[u8], offset: u32) -> AsyncHostResult<u32> {
        let event = event_at(buffer, offset)?;
        let size = EVENT_HEADER_SIZE
            .checked_add(event.len as usize)
            .ok_or(AsyncHostError::Fault)?;
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        if offset.checked_add(size).ok_or(AsyncHostError::Fault)? > buffer.len() {
            return Err(AsyncHostError::Fault);
        }
        u32::try_from(size).map_err(|_| AsyncHostError::Fault)
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_get_wd"
    )]
    pub(crate) fn event_get_wd(buffer: &[u8], offset: u32) -> AsyncHostResult<i32> {
        Ok(event_at(buffer, offset)?.wd)
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_has_relevant_event"
    )]
    pub(crate) fn event_has_relevant_event(
        buffer: &[u8],
        offset: u32,
    ) -> AsyncHostResult<bool> {
        let mask = event_at(buffer, offset)?.mask;
        Ok(mask
            & (libc::IN_CREATE
                | libc::IN_DELETE
                | libc::IN_MODIFY
                | libc::IN_MOVED_FROM
                | libc::IN_MOVED_TO)
            != 0)
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_has_overflow"
    )]
    pub(crate) fn event_has_overflow(buffer: &[u8], offset: u32) -> AsyncHostResult<bool> {
        Ok(event_at(buffer, offset)?.mask & libc::IN_Q_OVERFLOW != 0)
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_has_ignore"
    )]
    pub(crate) fn event_has_ignore(buffer: &[u8], offset: u32) -> AsyncHostResult<bool> {
        Ok(event_at(buffer, offset)?.mask & libc::IN_IGNORED != 0)
    }
}

fn event_at(buffer: &[u8], offset: u32) -> AsyncHostResult<libc::inotify_event> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let end = offset
        .checked_add(EVENT_HEADER_SIZE)
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
    fn watching_a_file_preserves_events_and_nonblocking_reads() {
        let watcher = unsafe { OwnedFd::from_raw_fd(create().unwrap()) };
        let fd = watcher.as_raw_fd();
        assert_ne!(
            unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        assert_ne!(
            unsafe { libc::fcntl(fd, libc::F_GETFL) } & libc::O_NONBLOCK,
            0
        );
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let wd = watch::add_file(fd, file.path().as_os_str(), false).unwrap();
        let mut buffer = vec![0; event_buffer_size() as usize];
        assert_eq!(fetch_event(fd, &mut buffer, event_buffer_size()), Ok(0));

        file.write_all(b"changed").unwrap();
        let count = fetch_event(fd, &mut buffer, event_buffer_size()).unwrap();
        assert!(count > 0);
        let events = &buffer[..count as usize];
        assert_eq!(event_get_wd(events, 0), Ok(wd));
        assert_eq!(event_has_relevant_event(events, 0), Ok(true));
        assert_eq!(event_get_size(events, 0), Ok(count as u32));
        assert_eq!(fetch_event(fd, &mut buffer, event_buffer_size()), Ok(0));

        remove_file(fd, wd).unwrap();
        let count = fetch_event(fd, &mut buffer, event_buffer_size()).unwrap();
        assert!(count > 0);
        assert_eq!(event_has_ignore(&buffer[..count as usize], 0), Ok(true));
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
