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

use super::*;
use crate::async_host::tests::{default_host, default_host_with_keys};

#[test]
fn io_result_status_rejects_wrong_fd_without_clearing_pending() {
    let mut result = HostIoResult::for_file_read(0, 0).unwrap();
    let pending_resource = Arc::new(Resource::invalid());
    let other_file = Arc::new(Resource::invalid());
    let pending_fd = pending_resource.raw_identity();

    result.mark_pending(pending_resource).unwrap();

    assert_eq!(
        result.validate_pending_resource(&other_file),
        Err(AsyncHostError::Badf)
    );
    assert_eq!(result.pending_resource_identity(), Some(pending_fd));
}

#[test]
fn io_result_creation_keeps_only_host_buffer_capacity() {
    let result = HostIoResult::for_file_read(3, 0).unwrap();

    assert_eq!(result.buffer, vec![0; 3]);
    assert_eq!(result.event, IO_RESULT_READ_EVENT);
    assert_eq!(result.pending_resource_identity(), None);
}

#[test]
fn read_dir_changes_io_result_leases_buffer_until_free() {
    let host = default_host();
    let buffer = host.insert_windows_watcher_buffer();
    let result = host.make_read_dir_changes_io_result(buffer).unwrap();

    assert_eq!(
        host.make_read_dir_changes_io_result(buffer),
        Err(AsyncHostError::Badf)
    );
    assert_eq!(
        host.with_windows_watcher_buffer(buffer, |_| Ok(())),
        Err(AsyncHostError::Badf)
    );

    host.free_io_result(result).unwrap();
    assert_eq!(
        host.with_windows_watcher_buffer(buffer, |buffer| Ok(buffer.capacity())),
        Ok(usize::try_from(crate::filesystem::watch::event_buffer_size()).unwrap())
    );
    host.free_windows_watcher_buffer(buffer).unwrap();
}

#[test]
fn freed_watcher_buffer_is_not_restored_by_its_io_result() {
    let host = default_host();
    let buffer = host.insert_windows_watcher_buffer();
    let result = host.make_read_dir_changes_io_result(buffer).unwrap();
    host.free_windows_watcher_buffer(buffer).unwrap();

    let replacement = host.insert_windows_watcher_buffer();
    assert_ne!(buffer, replacement);
    let replacement_result = host.make_read_dir_changes_io_result(replacement).unwrap();

    // The old result must not return its bytes into the new buffer's lease.
    host.free_io_result(result).unwrap();
    for handle in [buffer, replacement] {
        assert_eq!(
            host.with_windows_watcher_buffer(handle, |_| Ok(())),
            Err(AsyncHostError::Badf)
        );
    }
    assert_eq!(
        host.free_windows_watcher_buffer(buffer),
        Err(AsyncHostError::Badf)
    );

    host.free_io_result(replacement_result).unwrap();
    assert_eq!(
        host.with_windows_watcher_buffer(replacement, |buffer| Ok(buffer.capacity())),
        Ok(usize::try_from(crate::filesystem::watch::event_buffer_size()).unwrap())
    );
    host.free_windows_watcher_buffer(replacement).unwrap();
    assert!(host.leak_summary().is_none());
}

#[test]
fn dropping_host_retires_available_and_leased_watcher_buffers() {
    let keys = Rc::new(RefCell::new(HostKeys::default()));
    let host = default_host_with_keys(keys.clone());
    let available = host.insert_windows_watcher_buffer();
    let leased = host.insert_windows_watcher_buffer();
    host.make_read_dir_changes_io_result(leased).unwrap();
    for handle in [available, leased] {
        assert_eq!(
            keys.borrow().kind(key_from_handle(handle)),
            Some(HandleKind::WindowsWatcherBuffer)
        );
    }

    drop(host);

    for handle in [available, leased] {
        assert_eq!(keys.borrow().kind(key_from_handle(handle)), None);
    }
}

#[test]
fn read_dir_changes_rejects_a_generic_c_buffer() {
    let host = default_host();
    let buffer = host.insert_c_buffer(vec![1, 2, 3].into_boxed_slice());

    assert_eq!(
        host.free_windows_watcher_buffer(buffer),
        Err(AsyncHostError::Badf)
    );
    assert_eq!(
        host.make_read_dir_changes_io_result(buffer),
        Err(AsyncHostError::Badf)
    );
    assert_eq!(
        host.with_c_buffer(buffer, |bytes| Ok(bytes.to_vec())),
        Ok(vec![1, 2, 3])
    );
    host.free_c_buffer(buffer).unwrap();
}

#[test]
fn io_result_read_copy_uses_current_guest_destination() {
    let mut result = HostIoResult::for_file_read(3, 0).unwrap();
    result.buffer.copy_from_slice(b"abc");
    let mut memory = vec![0; 16];

    result
        .copy_read_result(memory.as_mut_slice(), 8, 1, 3)
        .unwrap();

    assert_eq!(&memory[9..12], b"abc");
}

#[test]
fn io_result_read_with_addr_copy_uses_current_guest_buffers() {
    let mut result = HostIoResult::for_socket_with_addr_read(3, 0, b"addr".to_vec()).unwrap();
    result.buffer.copy_from_slice(b"abc");
    result.addr_len = 4;
    let mut memory = vec![0; 16];

    result
        .copy_read_result_with_addr(memory.as_mut_slice(), 8, 1, 3, 0, 4)
        .unwrap();

    assert_eq!(&memory[0..4], b"addr");
    assert_eq!(&memory[9..12], b"abc");
    assert_eq!(
        result.copy_read_result(memory.as_mut_slice(), 8, 1, 3),
        Err(AsyncHostError::Inval)
    );
}

#[test]
fn io_result_socket_addr_creation_copies_guest_source() {
    let result = HostIoResult::for_socket_with_addr_read(3, 0, b"addr".to_vec()).unwrap();

    assert_eq!(result.addr_buffer, b"addr");
    assert_eq!(result.addr_len, 4);
    assert_eq!(result.event, IO_RESULT_READ_EVENT);
}

#[test]
fn io_result_write_creation_copies_guest_source() {
    let host = default_host();
    let mut memory = b"zzzabc".to_vec();

    let result = host
        .make_file_write_io_result(memory.as_mut_slice(), 3, 0, 3, 0)
        .unwrap();
    memory[3..6].copy_from_slice(b"xxx");

    let io_results = host.io_results.borrow();
    let result = io_results.io_results.get(key_from_handle(result)).unwrap();
    assert_eq!(result.buffer, b"abc");
    assert_eq!(result.event, IO_RESULT_WRITE_EVENT);
}

#[test]
fn cancel_io_result_rejects_wrong_fd_without_clearing_pending() {
    let host = default_host();
    let [read, write] = host.pipe(true, true).unwrap();
    let result = host.make_file_read_io_result(0, 0).unwrap();
    let raw_read = {
        let read_file = host.acquire_resource(read).unwrap();
        let raw_read = read_file.raw_identity();
        host.io_results
            .borrow_mut()
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap()
            .mark_pending(read_file)
            .unwrap();
        raw_read
    };

    assert_eq!(
        host.cancel_io_result(result, write),
        Err(AsyncHostError::Badf)
    );
    {
        let mut io_results = host.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap();
        assert_eq!(result.pending_resource_identity(), Some(raw_read));
        result.clear_pending();
    }

    host.free_io_result(result).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
}

#[test]
fn cancel_io_result_clears_pending_result_when_no_wait_is_needed() {
    let host = default_host();
    let [read, write] = host.pipe(true, true).unwrap();
    let result = host.make_file_read_io_result(0, 0).unwrap();
    {
        let read_file = host.acquire_resource(read).unwrap();
        host.io_results
            .borrow_mut()
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap()
            .mark_pending(read_file)
            .unwrap();
    }

    assert_eq!(host.cancel_io_result(result, read), Ok(0));
    {
        let io_results = host.io_results.borrow();
        let result = io_results.io_results.get(key_from_handle(result)).unwrap();
        assert_eq!(result.pending_resource_identity(), None);
    }

    host.free_io_result(result).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
}

#[test]
fn cancel_io_result_keeps_pending_result_when_wait_is_needed() {
    let host = default_host();
    let [read, write] = host.pipe(true, true).unwrap();
    let result = host.make_file_read_io_result(0, 0).unwrap();
    let raw_read = {
        let read_file = host.acquire_resource(read).unwrap();
        let raw_read = read_file.raw_identity();
        let mut io_results = host.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap();
        result.overlapped.Internal = windows_sys::Win32::Foundation::STATUS_PENDING as usize;
        result.mark_pending(read_file).unwrap();
        raw_read
    };

    assert_eq!(host.cancel_io_result(result, read), Ok(1));
    assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
    assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
    {
        let mut io_results = host.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap();
        assert_eq!(result.pending_resource_identity(), Some(raw_read));
        result.clear_pending();
    }

    host.free_io_result(result).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
}

#[test]
fn close_fd_rejects_pending_io_result() {
    let host = default_host();
    let [read, write] = host.pipe(true, true).unwrap();
    let result = host.make_file_read_io_result(0, 0).unwrap();
    let raw_read = {
        let read_file = host.acquire_resource(read).unwrap();
        let raw_read = read_file.raw_identity();
        host.io_results
            .borrow_mut()
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap()
            .mark_pending(read_file)
            .unwrap();
        raw_read
    };

    assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
    {
        assert!(host.acquire_resource(read).is_ok());
        let mut io_results = host.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap();
        assert_eq!(result.pending_resource_identity(), Some(raw_read));
        result.clear_pending();
    }

    host.free_io_result(result).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
}

#[test]
fn close_fd_rejects_extra_pending_close_guard() {
    let host = default_host();
    let [read, write] = host.pipe(true, true).unwrap();
    let result = host.make_file_read_io_result(0, 0).unwrap();
    let (raw_read, raw_write) = {
        let read_file = host.acquire_resource(read).unwrap();
        let write_file = host.acquire_resource(write).unwrap();
        let raw_read = read_file.raw_identity();
        let raw_write = write_file.raw_identity();
        host.io_results
            .borrow_mut()
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap()
            .mark_pending_with_close_guard(read_file, write_file)
            .unwrap();
        (raw_read, raw_write)
    };

    assert_eq!(host.close_fd(write), Err(AsyncHostError::Inval));
    assert_eq!(
        host.cancel_io_result(result, write),
        Err(AsyncHostError::Badf)
    );
    assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
    {
        assert!(host.acquire_resource(read).is_ok());
        assert!(host.acquire_resource(write).is_ok());
        let mut io_results = host.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap();
        assert_eq!(result.pending_resource_identity(), Some(raw_read));
        assert!(result.protects_pending_resource(&host.acquire_resource(write).unwrap()));
        assert_eq!(
            result
                .pending
                .as_ref()
                .unwrap()
                .close_guard
                .as_ref()
                .map(|file| file.raw_identity()),
            Some(raw_write)
        );
        result.clear_pending();
    }

    host.free_io_result(result).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
}

#[test]
fn free_io_result_rejects_pending_result() {
    let host = default_host();
    let [read, write] = host.pipe(true, true).unwrap();
    let result = host.make_file_read_io_result(0, 0).unwrap();
    {
        let read_file = host.acquire_resource(read).unwrap();
        host.io_results
            .borrow_mut()
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap()
            .mark_pending(read_file)
            .unwrap();
    }

    assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
    assert!(
        host.io_results
            .borrow()
            .io_results
            .get(key_from_handle(result))
            .is_some()
    );
    host.io_results
        .borrow_mut()
        .io_results
        .get_mut(key_from_handle(result))
        .unwrap()
        .clear_pending();
    host.free_io_result(result).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
}

#[test]
fn poll_event_io_result_marks_pending_result_delivered() {
    use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

    let host = default_host();
    let poll = host.poll_create().unwrap();
    let completion_port = {
        let polls = host.polls.borrow();
        let poll = polls.polls.get(key_from_handle(poll)).unwrap();
        poll.instance.raw_fd()
    };
    let result = host.make_file_read_io_result(0, 0).unwrap();
    let [read, write] = host.pipe(true, true).unwrap();
    let read_file = host.acquire_resource(read).unwrap();
    let raw_fd = read_file.raw_identity();
    let overlapped = {
        let mut io_results = host.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap();
        result.mark_pending(read_file).unwrap();
        result.overlapped_ptr()
    };
    let posted =
        unsafe { PostQueuedCompletionStatus(completion_port, 0, raw_fd as usize, overlapped) };
    assert_ne!(posted, 0);

    assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
    let event = host.poll_get_event(poll, 0).unwrap();

    assert_eq!(host.poll_event_io_result(event).unwrap(), result);
    assert_eq!(
        host.io_results
            .borrow()
            .io_results
            .get(key_from_handle(result))
            .unwrap()
            .pending_resource_identity(),
        None
    );
    host.free_io_result(result).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
}

#[test]
fn reading_an_old_completion_does_not_complete_a_resubmitted_io_result() {
    use windows_sys::Win32::Foundation::ERROR_IO_PENDING;
    use windows_sys::Win32::Storage::FileSystem::WriteFile;

    let host = default_host();
    let poll = host.poll_create().unwrap();
    let [read, write] = host.pipe(true, false).unwrap();
    host.poll_register(poll, read, true).unwrap();
    let writer = host.acquire_resource(write).unwrap();
    let result = host.make_file_read_io_result(1, 0).unwrap();

    assert_eq!(
        host.read_io_result(read, result),
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    );
    let mut written = 0;
    assert_ne!(
        unsafe {
            WriteFile(
                writer.as_file().unwrap().as_raw_handle(),
                b"a".as_ptr(),
                1,
                &mut written,
                std::ptr::null_mut(),
            )
        },
        0
    );
    assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
    let old_event = host.poll_get_event(poll, 0).unwrap();
    assert_eq!(host.poll_event_io_result(old_event).unwrap(), result);

    assert_eq!(
        host.read_io_result(read, result),
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    );
    assert_eq!(host.poll_event_io_result(old_event).unwrap(), result);
    let still_pending = host
        .io_results
        .borrow()
        .io_results
        .get(key_from_handle(result))
        .unwrap()
        .is_pending();

    // Finish the real second read before asserting: even the broken host
    // must not drop storage that Windows is still writing into.
    assert_ne!(
        unsafe {
            WriteFile(
                writer.as_file().unwrap().as_raw_handle(),
                b"b".as_ptr(),
                1,
                &mut written,
                std::ptr::null_mut(),
            )
        },
        0
    );
    assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
    let event = host.poll_get_event(poll, 0).unwrap();
    host.poll_event_io_result(event).unwrap();
    host.free_io_result(result).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
    host.poll_destroy(poll).unwrap();
    assert!(
        still_pending,
        "reading an old event must not retire the new submission"
    );
}

#[test]
fn cancellation_keeps_the_iocp_request_until_its_packet_is_consumed() {
    use windows_sys::Win32::Foundation::{
        ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_OPERATION_ABORTED,
    };

    let host = default_host();
    let poll = host.poll_create().unwrap();
    let [read, write] = host.pipe(true, false).unwrap();
    host.poll_register(poll, read, true).unwrap();
    let result = host.make_file_read_io_result(1, 0).unwrap();
    assert_eq!(
        host.read_io_result(read, result),
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    );
    assert_eq!(host.cancel_io_result(result, read), Ok(1));
    assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
    assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
    assert_eq!(
        host.io_result_get_status(result, read),
        Err(AsyncHostError::Native(ERROR_IO_INCOMPLETE as i32))
    );
    assert_eq!(host.poll_wait(poll, 5000, &Default::default()).unwrap(), 1);
    let event = host.poll_get_event(poll, 0).unwrap();
    assert_eq!(host.poll_event_io_result(event), Ok(result));
    assert_eq!(
        host.io_result_get_status(result, read),
        Err(AsyncHostError::Native(ERROR_OPERATION_ABORTED as i32))
    );
    host.free_io_result(result).unwrap();

    // A cached completion must never look up a replacement allocation through
    // its old OVERLAPPED address, even when the replacement uses the same slot.
    let replacement = host.make_file_read_io_result(1, 0).unwrap();
    assert_ne!(replacement, result);
    assert_eq!(host.poll_event_io_result(event), Err(AsyncHostError::Badf));
    assert_eq!(host.free_io_result(result), Err(AsyncHostError::Badf));
    assert_eq!(host.io_result_get_event(read), Err(AsyncHostError::Badf));
    host.free_io_result(replacement).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
    host.poll_destroy(poll).unwrap();
    assert!(host.leak_summary().is_none());
}

#[test]
fn host_shutdown_drains_multiple_reads_after_the_poll_handle_is_destroyed() {
    use windows_sys::Win32::Foundation::ERROR_IO_PENDING;

    let keys = Rc::new(RefCell::new(HostKeys::default()));
    let host = default_host_with_keys(keys.clone());
    let poll = host.poll_create().unwrap();
    let [read, _write] = host.pipe(true, false).unwrap();
    host.poll_register(poll, read, true).unwrap();
    let resource = Arc::downgrade(&host.acquire_resource(read).unwrap());
    let first = host.make_file_read_io_result(1, 0).unwrap();
    let second = host.make_file_read_io_result(1, 0).unwrap();
    let idle = host.make_file_read_io_result(0, 0).unwrap();
    for result in [first, second] {
        assert_eq!(
            host.read_io_result(read, result),
            Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
        );
    }
    host.poll_destroy(poll).unwrap();
    drop(host);
    for result in [first, second, idle] {
        assert_eq!(keys.borrow().kind(key_from_handle(result)), None);
    }
    assert!(
        resource.upgrade().is_none(),
        "shutdown must release the Resources after cancellation completes"
    );
}

#[test]
fn host_shutdown_drains_unregistered_reads_using_each_requests_event() {
    use windows_sys::Win32::Foundation::ERROR_IO_PENDING;

    let keys = Rc::new(RefCell::new(HostKeys::default()));
    let host = default_host_with_keys(keys.clone());
    let [read, _write] = host.pipe(true, false).unwrap();
    let resource = Arc::downgrade(&host.acquire_resource(read).unwrap());
    let first = host.make_file_read_io_result(1, 0).unwrap();
    let second = host.make_file_read_io_result(1, 0).unwrap();
    for result in [first, second] {
        assert_eq!(
            host.read_io_result(read, result),
            Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
        );
    }
    let poll = host.poll_create().unwrap();
    assert_eq!(
        host.poll_register(poll, read, true),
        Err(AsyncHostError::Inval)
    );
    assert_eq!(
        host.poll_register_legacy(poll, read, true),
        Err(AsyncHostError::Inval)
    );
    host.poll_destroy(poll).unwrap();
    drop(host);
    assert!(resource.upgrade().is_none());
    for result in [first, second] {
        assert_eq!(keys.borrow().kind(key_from_handle(result)), None);
    }
}

#[test]
fn immediate_read_needs_no_completion_packet() {
    use windows_sys::Win32::Storage::FileSystem::WriteFile;

    let host = default_host();
    let poll = host.poll_create().unwrap();
    let [read, write] = host.pipe(true, false).unwrap();
    host.poll_register(poll, read, true).unwrap();
    let writer = host.acquire_resource(write).unwrap();
    let mut written = 0;
    assert_ne!(
        unsafe {
            WriteFile(
                writer.as_file().unwrap().as_raw_handle(),
                b"x".as_ptr(),
                1,
                &mut written,
                std::ptr::null_mut(),
            )
        },
        0
    );
    let result = host.make_file_read_io_result(1, 0).unwrap();
    assert_eq!(host.read_io_result(read, result), Ok(1));
    let mut output = [0];
    host.io_result_copy_read(&mut output[..], result, 0, 0, 1)
        .unwrap();
    assert_eq!(output, *b"x");
    host.free_io_result(result).unwrap();
    assert_eq!(host.poll_wait(poll, 0, &Default::default()), Ok(0));
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
    host.poll_destroy(poll).unwrap();
}

#[test]
fn watcher_iocp_completion_returns_the_buffer_and_handles_buffered_changes() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Foundation::ERROR_IO_PENDING;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let host = default_host();
    let directory = tempfile::tempdir().unwrap();
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED)
        .open(directory.path())
        .unwrap();
    let resource = Resource::from_file(file, crate::filesystem::FileGrant::UNTRACKED);
    let directory_handle = host.resources.borrow_mut().insert_resource(resource);
    let poll = host.poll_create().unwrap();
    host.poll_register(poll, directory_handle, true).unwrap();
    let buffer = host.insert_windows_watcher_buffer();
    let result = host.make_read_dir_changes_io_result(buffer).unwrap();
    assert_eq!(
        host.read_dir_changes_io_result(directory_handle, result),
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    );
    std::fs::write(directory.path().join("first"), b"x").unwrap();
    assert_eq!(host.poll_wait(poll, 5000, &Default::default()), Ok(1));
    assert!(host.io_result_get_status(result, directory_handle).unwrap() > 0);
    host.free_io_result(result).unwrap();
    host.with_windows_watcher_buffer(buffer, |buffer| {
        assert_eq!(
            crate::filesystem::watch::event_path_units(buffer, 0)?,
            "first".encode_utf16().collect::<Vec<_>>()
        );
        Ok(())
    })
    .unwrap();

    // The directory handle keeps recording notifications between requests.
    // A successful resubmission must still deliver its IOCP identity.
    std::fs::write(directory.path().join("buffered"), b"y").unwrap();
    let result = host.make_read_dir_changes_io_result(buffer).unwrap();
    assert_eq!(
        host.read_dir_changes_io_result(directory_handle, result),
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    );
    let completed = host.poll_wait(poll, 5000, &Default::default());
    if completed != Ok(1) {
        // Bound the test even if a driver supplies event completion without an
        // IOCP packet. Confirm native completion before releasing the storage.
        let file = host.acquire_resource(directory_handle).unwrap();
        let mut table = host.io_results.borrow_mut();
        let request = table.io_results.get_mut(key_from_handle(result)).unwrap();
        request.request_cancel().unwrap();
        let mut bytes = 0;
        unsafe {
            windows_sys::Win32::System::IO::GetOverlappedResult(
                raw_overlapped_handle(&file).unwrap(),
                request.overlapped_ptr(),
                &mut bytes,
                1,
            );
        }
        assert!(request.native_completed());
        request.clear_pending();
    }
    host.free_io_result(result).unwrap();
    host.free_windows_watcher_buffer(buffer).unwrap();
    host.close_fd(directory_handle).unwrap();
    host.poll_destroy(poll).unwrap();
    assert_eq!(completed, Ok(1));
}
