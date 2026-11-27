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
fn connect_on_an_already_bound_socket_returns_an_error_without_pending_io() {
    use crate::network::os::address;

    assert_eq!(crate::network::os::init_wsa(), 0);
    let host = default_host();
    let socket = host.make_tcp_socket(4).unwrap();
    let mut addr = vec![0; address::ipv4_addr_size()];
    address::init_ip_addr(&mut addr, 0x7f000001, 0).unwrap();
    host.bind(socket, &addr).unwrap();

    // A guest can bind before calling the ConnectEx import, even though normal
    // async code uses a fresh socket. Reject this ordering without panicking or
    // retaining an operation that was never submitted.
    address::init_ip_addr(&mut addr, 0x7f000001, 80).unwrap();
    let addr_len = addr.len() as u32;
    let result = host
        .make_connect_io_result(addr.as_mut_slice(), 0, addr_len)
        .unwrap();
    assert_eq!(
        host.connect_io_result(socket, result),
        Err(AsyncHostError::Native(
            windows_sys::Win32::Networking::WinSock::WSAEINVAL
        ))
    );
    host.free_io_result(result).unwrap();
    host.close_fd(socket).unwrap();
}

#[test]
fn denied_network_submission_does_not_bind_or_retain_the_socket() {
    use crate::async_host::tests::host_with_policy;
    use crate::network::os::address;

    assert_eq!(crate::network::os::init_wsa(), 0);
    let dir = tempfile::tempdir().unwrap();
    let policy = dir.path().join("deny-all.toml");
    std::fs::write(&policy, "").unwrap();
    let host = host_with_policy(&policy);
    let mut addr = vec![0; address::ipv4_addr_size()];
    address::init_ip_addr(&mut addr, 0x7f000001, 80).unwrap();
    let len = addr.len() as u32;

    let tcp = host.make_tcp_socket(4).unwrap();
    let connect = host
        .make_connect_io_result(addr.as_mut_slice(), 0, len)
        .unwrap();
    assert_eq!(
        host.connect_io_result(tcp, connect),
        Err(AsyncHostError::PermissionDenied)
    );
    // Authorization must precede ConnectEx's implicit local bind.
    host.with_resource(tcp, |socket| {
        assert_eq!(
            host.network()
                .getsockname(socket, &mut vec![0; len as usize])
                .unwrap_err()
                .errno(),
            windows_sys::Win32::Networking::WinSock::WSAEINVAL
        );
        Ok(())
    })
    .unwrap();
    host.free_io_result(connect).unwrap();
    host.close_fd(tcp).unwrap();

    let udp = host.make_udp_socket(4, false).unwrap();
    let send = host
        .make_socket_with_addr_write_io_result(addr.as_mut_slice(), 0, 0, 1, 0, 0, len)
        .unwrap();
    assert_eq!(
        host.write_io_result(udp, send),
        Err(AsyncHostError::PermissionDenied)
    );
    host.free_io_result(send).unwrap();
    host.close_fd(udp).unwrap();
    assert_eq!(host.leak_summary(), None);
}

#[test]
fn pending_udp_receive_keeps_address_outputs_until_completion() {
    use crate::network::os::address;
    use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
    use windows_sys::Win32::Foundation::ERROR_IO_PENDING;

    // Bound the whole process, including teardown if a lifetime regression
    // prevents completion or cancellation from releasing native storage.
    const CHILD: &str = "MOONRUN_TEST_PENDING_UDP_RECEIVE";
    if std::env::var_os(CHILD).is_none() {
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "async_host::windows_io::tests::pending_udp_receive_keeps_address_outputs_until_completion", "--nocapture"])
            .env(CHILD, "1")
            .timeout(std::time::Duration::from_secs(15))
            .assert().success();
        return;
    }
    assert_eq!(crate::network::os::init_wsa(), 0);
    let host = default_host();
    let socket = host.make_udp_socket(4, false).unwrap();
    let mut addr = vec![0; address::ipv4_addr_size()];
    address::init_ip_addr(&mut addr, 0x7f000001, 0).unwrap();
    host.bind(socket, &addr).unwrap();
    host.with_resource(socket, |socket| {
        host.network().getsockname(socket, &mut addr)
    })
    .unwrap();
    let endpoint = SocketAddrV4::new(
        Ipv4Addr::LOCALHOST,
        address::ip_addr_get_port(&addr).unwrap(),
    );
    let poll = host.poll_create().unwrap();
    host.poll_register(poll, socket, true).unwrap();

    // Start with IPv6-sized capacity: Winsock must update the retained address
    // length to IPv4 size after submission has already returned.
    let mut output = vec![0; address::ipv6_addr_size()];
    let capacity = output.len() as u32;
    let result = host
        .make_socket_with_addr_read_io_result(output.as_mut_slice(), 3, 0, 0, capacity)
        .unwrap();
    assert_eq!(
        host.read_io_result(socket, result),
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    );
    assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
    assert_eq!(host.close_fd(socket), Err(AsyncHostError::Inval));

    let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    sender.send_to(b"udp", endpoint).unwrap();
    assert_eq!(host.poll_wait(poll, 5000, &Default::default()).unwrap(), 1);
    let event = host.poll_get_event(poll, 0).unwrap();
    assert_eq!(host.poll_event_io_result(event), Ok(result));
    assert_eq!(host.io_result_get_status(result, socket), Ok(3));
    let mut memory = vec![0; 3 + addr.len()];
    host.io_result_copy_read_with_addr(
        memory.as_mut_slice(),
        result,
        0,
        0,
        3,
        3,
        addr.len() as u32,
    )
    .unwrap();
    assert_eq!(&memory[..3], b"udp");
    assert_eq!(
        address::ip_addr_get_port(&memory[3..]).unwrap(),
        sender.local_addr().unwrap().port()
    );
    host.free_io_result(result).unwrap();
    host.close_fd(socket).unwrap();
    host.poll_destroy(poll).unwrap();
    assert_eq!(host.leak_summary(), None);
}

#[test]
fn accept_completion_publishes_peer_address() {
    use crate::network::os::address;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream};
    use windows_sys::Win32::Foundation::ERROR_IO_PENDING;
    use windows_sys::Win32::System::IO::GetOverlappedResult;

    const CHILD: &str = "MOONRUN_TEST_ACCEPT_COMPLETION";
    if std::env::var_os(CHILD).is_none() {
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "async_host::windows_io::tests::accept_completion_publishes_peer_address",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .timeout(std::time::Duration::from_secs(15))
            .assert()
            .success();
        return;
    }
    assert_eq!(crate::network::os::init_wsa(), 0);
    // Exercise every way a successful accept becomes visible: IOCP, explicit
    // status, cancellation losing to completion, and an already queued client.
    for completion in ["iocp", "status", "cancel", "queued"] {
        let host = default_host();
        let server = host.make_tcp_socket(4).unwrap();
        let connection = host.make_tcp_socket(4).unwrap();
        let mut addr = vec![0; address::ipv4_addr_size()];
        address::init_ip_addr(&mut addr, 0x7f000001, 0).unwrap();
        host.bind(server, &addr).unwrap();
        host.listen(server).unwrap();
        host.with_resource(server, |socket| {
            host.network().getsockname(socket, &mut addr)
        })
        .unwrap();
        let endpoint = SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            address::ip_addr_get_port(&addr).unwrap(),
        ));
        let poll = if completion == "iocp" {
            let poll = host.poll_create().unwrap();
            host.poll_register(poll, server, true).unwrap();
            Some(poll)
        } else {
            None
        };
        let result = host.make_accept_io_result(addr.len() as u32).unwrap();
        let mut peer = vec![7; addr.len()];
        // Querying a fresh OVERLAPPED can report zero status, but must not
        // produce an address for an operation that never ran.
        let _ = host.io_result_get_status(result, server);
        assert_eq!(
            host.get_accept_peer_addr(peer.as_mut_slice(), result, 0, addr.len() as u32),
            Err(AsyncHostError::Inval)
        );
        let queued_client = (completion == "queued").then(|| {
            TcpStream::connect_timeout(&endpoint, std::time::Duration::from_secs(5)).unwrap()
        });
        let accepted = host.accept_io_result(server, connection, result);
        if completion != "queued" || accepted != Ok(1) {
            assert_eq!(
                accepted,
                Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
            );
            assert_eq!(
                host.get_accept_peer_addr(peer.as_mut_slice(), result, 0, addr.len() as u32),
                Err(AsyncHostError::Inval)
            );
            // AcceptEx borrows the pre-created connection as well as the listener.
            assert_eq!(host.close_fd(server), Err(AsyncHostError::Inval));
            assert_eq!(host.close_fd(connection), Err(AsyncHostError::Inval));
            assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
        }
        let client = queued_client.unwrap_or_else(|| {
            TcpStream::connect_timeout(&endpoint, std::time::Duration::from_secs(5)).unwrap()
        });
        if let Some(poll) = poll {
            assert_eq!(host.poll_wait(poll, 5000, &Default::default()).unwrap(), 1);
            let event = host.poll_get_event(poll, 0).unwrap();
            assert_eq!(host.poll_event_io_result(event), Ok(result));
        } else if accepted != Ok(1) {
            // Wait for native completion without retiring host ownership. The
            // selected public entry point must finalize the output below.
            let file = host.acquire_resource(server).unwrap();
            let mut table = host.io_results.borrow_mut();
            let request = table.io_results.get_mut(key_from_handle(result)).unwrap();
            let mut bytes = 0;
            assert_ne!(
                unsafe {
                    GetOverlappedResult(
                        raw_overlapped_handle(&file).unwrap(),
                        request.io.overlapped_ptr(),
                        &mut bytes,
                        1,
                    )
                },
                0
            );
            drop(table);
            if completion == "cancel" {
                assert_eq!(host.cancel_io_result(result, server), Ok(0));
            } else {
                assert_eq!(host.io_result_get_status(result, server), Ok(0));
            }
        }
        // No status query is required after IOCP or synchronous completion.
        // Read before SO_UPDATE_ACCEPT_CONTEXT, as the guest does.
        host.get_accept_peer_addr(peer.as_mut_slice(), result, 0, addr.len() as u32)
            .unwrap();
        assert_eq!(
            address::ip_addr_get_port(&peer).unwrap(),
            client.local_addr().unwrap().port(),
            "{completion}"
        );
        assert_eq!(
            address::ip_addr_get_ip(&peer).unwrap(),
            u32::from(Ipv4Addr::LOCALHOST)
        );
        // Repeated status/getter calls must keep the already produced result.
        assert_eq!(host.io_result_get_status(result, server), Ok(0));
        let mut again = vec![0; peer.len()];
        host.get_accept_peer_addr(again.as_mut_slice(), result, 0, addr.len() as u32)
            .unwrap();
        assert_eq!(again, peer);
        host.setup_accepted_socket(server, connection).unwrap();
        // An immediate error on resubmission must not expose the old address.
        assert!(host.accept_io_result(server, connection, result).is_err());
        assert_eq!(
            host.get_accept_peer_addr(peer.as_mut_slice(), result, 0, addr.len() as u32),
            Err(AsyncHostError::Inval)
        );
        host.free_io_result(result).unwrap();
        host.close_fd(connection).unwrap();
        host.close_fd(server).unwrap();
        if let Some(poll) = poll {
            host.poll_destroy(poll).unwrap();
        }
        assert_eq!(host.leak_summary(), None);
    }
}

#[test]
fn accept_peer_address_rejects_unsubmitted_and_failed_requests() {
    let host = default_host();
    let result = host.make_accept_io_result(16).unwrap();
    let mut memory = [7; 16];
    assert_eq!(
        host.get_accept_peer_addr(&mut memory[..], result, 0, 16),
        Err(AsyncHostError::Inval)
    );
    {
        let mut table = host.io_results.borrow_mut();
        let request = table.io_results.get_mut(key_from_handle(result)).unwrap();
        request
            .io
            .mark_pending(Arc::new(Resource::invalid()))
            .unwrap();
        request.io.overlapped.Internal = windows_sys::Win32::Foundation::STATUS_CANCELLED as usize;
        let overlapped = request.io.overlapped_ptr();
        // A failed completion retires the request without parsing its storage.
        table.complete(overlapped).unwrap();
    }
    assert_eq!(
        host.get_accept_peer_addr(&mut memory[..], result, 0, 16),
        Err(AsyncHostError::Inval)
    );
    assert_eq!(memory, [7; 16]);
    host.free_io_result(result).unwrap();
}

#[test]
fn io_result_status_rejects_wrong_fd_without_clearing_pending() {
    let mut result = HostIoResult::for_file_read(0, 0).unwrap();
    let pending_resource = Arc::new(Resource::invalid());
    let other_file = Arc::new(Resource::invalid());
    let pending_fd = pending_resource.raw_identity();

    result.io.mark_pending(pending_resource).unwrap();

    assert_eq!(
        result.io.validate_pending_resource(&other_file),
        Err(AsyncHostError::Badf)
    );
    assert_eq!(result.io.pending_resource_identity(), Some(pending_fd));
}

#[test]
fn io_result_creation_keeps_only_host_buffer_capacity() {
    let result = HostIoResult::for_file_read(3, 0).unwrap();

    let HostIoOperation::File(buffer) = &result.operation else {
        panic!("expected file read");
    };
    assert_eq!(buffer.as_ref(), &[0; 3]);
    assert_eq!(result.event, IO_RESULT_READ_EVENT);
    assert_eq!(result.io.pending_resource_identity(), None);
}

#[test]
fn read_dir_changes_io_result_leases_buffer_until_free() {
    let host = default_host();
    let buffer = host.buffers().allocate_windows_watcher();
    let result = host.make_read_dir_changes_io_result(buffer).unwrap();

    assert_eq!(
        host.make_read_dir_changes_io_result(buffer),
        Err(AsyncHostError::Badf)
    );
    assert_eq!(
        host.buffers().with_windows_watcher(buffer, |_| Ok(())),
        Err(AsyncHostError::Badf)
    );

    host.free_io_result(result).unwrap();
    assert_eq!(
        host.buffers()
            .with_windows_watcher(buffer, |buffer| Ok(buffer.capacity())),
        Ok(usize::try_from(crate::filesystem::watch::event_buffer_size()).unwrap())
    );
    host.buffers().free_windows_watcher(buffer).unwrap();
}

#[test]
fn pending_watcher_cancellation_keeps_the_buffer_lease() {
    let host = default_host();
    let [read, write] = host.pipe(true, true).unwrap();
    let buffer = host.buffers().allocate_windows_watcher();
    let request = host.make_read_dir_changes_io_result(buffer).unwrap();
    {
        // No native I/O is submitted. Keep this request pending until explicit
        // retirement, independently of when a real cancellation packet arrives.
        let mut results = host.io_results.borrow_mut();
        let result = results
            .io_results
            .get_mut(key_from_handle(request))
            .unwrap();
        result.io.overlapped.Internal = windows_sys::Win32::Foundation::STATUS_PENDING as usize;
        result
            .io
            .mark_pending(host.acquire_resource(read).unwrap())
            .unwrap();
    }

    assert_eq!(host.cancel_io_result(request, read), Ok(1));
    assert_eq!(host.free_io_result(request), Err(AsyncHostError::Inval));
    assert_eq!(
        host.buffers().with_windows_watcher(buffer, |_| Ok(())),
        Err(AsyncHostError::Badf)
    );
    host.io_results
        .borrow_mut()
        .io_results
        .get_mut(key_from_handle(request))
        .unwrap()
        .io
        .clear_pending();
    host.free_io_result(request).unwrap();
    assert_eq!(
        host.buffers().with_windows_watcher(buffer, |_| Ok(())),
        Ok(())
    );
    host.buffers().free_windows_watcher(buffer).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
    assert!(host.leak_summary().is_none());
}

#[test]
fn freed_watcher_buffer_is_not_restored_by_its_io_result() {
    let host = default_host();
    let buffer = host.buffers().allocate_windows_watcher();
    let result = host.make_read_dir_changes_io_result(buffer).unwrap();
    host.buffers().free_windows_watcher(buffer).unwrap();

    let replacement = host.buffers().allocate_windows_watcher();
    assert_ne!(buffer, replacement);
    let replacement_result = host.make_read_dir_changes_io_result(replacement).unwrap();

    // The old result must not return its bytes into the new buffer's lease.
    host.free_io_result(result).unwrap();
    for handle in [buffer, replacement] {
        assert_eq!(
            host.buffers().with_windows_watcher(handle, |_| Ok(())),
            Err(AsyncHostError::Badf)
        );
    }
    assert_eq!(
        host.buffers().free_windows_watcher(buffer),
        Err(AsyncHostError::Badf)
    );

    host.free_io_result(replacement_result).unwrap();
    assert_eq!(
        host.buffers()
            .with_windows_watcher(replacement, |buffer| Ok(buffer.capacity())),
        Ok(usize::try_from(crate::filesystem::watch::event_buffer_size()).unwrap())
    );
    host.buffers().free_windows_watcher(replacement).unwrap();
    assert!(host.leak_summary().is_none());
}

#[test]
fn dropping_host_retires_available_and_leased_watcher_buffers() {
    let keys = Rc::new(RefCell::new(HostKeys::default()));
    let host = default_host_with_keys(keys.clone());
    let available = host.buffers().allocate_windows_watcher();
    let leased = host.buffers().allocate_windows_watcher();
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
    let buffer = host.buffers().publish(vec![1, 2, 3].into_boxed_slice());

    assert_eq!(
        host.buffers().free_windows_watcher(buffer),
        Err(AsyncHostError::Badf)
    );
    assert_eq!(
        host.make_read_dir_changes_io_result(buffer),
        Err(AsyncHostError::Badf)
    );
    assert_eq!(
        host.buffers()
            .with_buffer(buffer, |bytes| Ok(bytes.to_vec())),
        Ok(vec![1, 2, 3])
    );
    host.buffers().free(buffer).unwrap();
}

#[test]
fn io_result_read_copy_uses_current_guest_destination() {
    let mut result = HostIoResult::for_file_read(3, 0).unwrap();
    let HostIoOperation::File(buffer) = &mut result.operation else {
        panic!("expected file read");
    };
    buffer.copy_from_slice(b"abc");
    let mut memory = vec![0; 16];

    result
        .copy_read_result(memory.as_mut_slice(), 8, 1, 3)
        .unwrap();

    assert_eq!(&memory[9..12], b"abc");
}

#[test]
fn accept_result_preserves_empty_generic_read_copy() {
    // Direct ABI callers can use the generic getter on an Accept result. It
    // exposes no payload, but still validates the destination of an empty copy.
    let host = default_host();
    let result = host.make_accept_io_result(16).unwrap();
    let mut memory = [7; 4];

    assert_eq!(
        host.io_result_copy_read(&mut memory[..], result, 4, 0, 0),
        Ok(())
    );
    assert_eq!(
        host.io_result_copy_read(&mut memory[..], result, 0, 0, 1),
        Err(AsyncHostError::Fault)
    );
    assert_eq!(
        host.io_result_copy_read(&mut memory[..], result, 5, 0, 0),
        Err(AsyncHostError::Fault)
    );
    assert_eq!(
        host.io_result_copy_read(&mut memory[..], result, u32::MAX, 1, 0),
        Err(AsyncHostError::Fault)
    );
    assert_eq!(
        host.io_result_copy_read_with_addr(&mut memory[..], result, 0, 0, 0, 0, 0),
        Err(AsyncHostError::Inval)
    );
    assert_eq!(memory, [7; 4]);
    host.free_io_result(result).unwrap();
    assert_eq!(host.leak_summary(), None);
}

#[test]
fn io_result_read_with_addr_copy_uses_current_guest_buffers() {
    let mut result =
        HostIoResult::for_socket_with_addr_read(3, 0, b"addr".as_slice().into()).unwrap();
    let HostIoOperation::SocketWithAddr {
        buffer, addr_len, ..
    } = &mut result.operation
    else {
        panic!("expected socket read with address");
    };
    buffer.copy_from_slice(b"abc");
    *addr_len = 4;
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
    let result = HostIoResult::for_socket_with_addr_read(3, 0, b"addr".as_slice().into()).unwrap();

    let HostIoOperation::SocketWithAddr {
        addr_buffer,
        addr_len,
        ..
    } = &result.operation
    else {
        panic!("expected socket read with address");
    };
    assert_eq!(addr_buffer.as_ref(), b"addr");
    assert_eq!(*addr_len, 4);
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
    let HostIoOperation::File(buffer) = &result.operation else {
        panic!("expected file write");
    };
    assert_eq!(buffer.as_ref(), b"abc");
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
            .io
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
        assert_eq!(result.io.pending_resource_identity(), Some(raw_read));
        result.io.clear_pending();
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
            .io
            .mark_pending(read_file)
            .unwrap();
    }

    assert_eq!(host.cancel_io_result(result, read), Ok(0));
    {
        let io_results = host.io_results.borrow();
        let result = io_results.io_results.get(key_from_handle(result)).unwrap();
        assert_eq!(result.io.pending_resource_identity(), None);
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
        result.io.overlapped.Internal = windows_sys::Win32::Foundation::STATUS_PENDING as usize;
        result.io.mark_pending(read_file).unwrap();
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
        assert_eq!(result.io.pending_resource_identity(), Some(raw_read));
        result.io.clear_pending();
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
            .io
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
        assert_eq!(result.io.pending_resource_identity(), Some(raw_read));
        result.io.clear_pending();
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
            .io
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
        assert_eq!(result.io.pending_resource_identity(), Some(raw_read));
        assert!(
            result
                .io
                .protects_pending_resource(&host.acquire_resource(write).unwrap())
        );
        assert_eq!(
            result
                .io
                .pending
                .as_ref()
                .unwrap()
                .close_guard
                .as_ref()
                .map(|file| file.raw_identity()),
            Some(raw_write)
        );
        result.io.clear_pending();
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
            .io
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
        .io
        .clear_pending();
    host.free_io_result(result).unwrap();
    host.close_fd(read).unwrap();
    host.close_fd(write).unwrap();
}

#[test]
fn completion_retires_the_pending_result() {
    let host = default_host();
    let result = host.make_file_read_io_result(0, 0).unwrap();
    let [read, write] = host.pipe(true, true).unwrap();
    let read_file = host.acquire_resource(read).unwrap();
    let overlapped = {
        let mut io_results = host.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(key_from_handle(result))
            .unwrap();
        result.io.mark_pending(read_file).unwrap();
        result.io.overlapped_ptr()
    };
    let key = host.io_results.borrow_mut().complete(overlapped).unwrap();
    assert_eq!(host.io_results.borrow().event_result(key).unwrap(), result);
    assert_eq!(
        host.io_results
            .borrow()
            .io_results
            .get(key_from_handle(result))
            .unwrap()
            .io
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
        .io
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
    let buffer = host.buffers().allocate_windows_watcher();
    let result = host.make_read_dir_changes_io_result(buffer).unwrap();
    assert_eq!(
        host.read_dir_changes_io_result(directory_handle, result),
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    );
    std::fs::write(directory.path().join("first"), b"x").unwrap();
    assert_eq!(host.poll_wait(poll, 5000, &Default::default()), Ok(1));
    // Native completion alone does not release the lease: status still has to
    // finalize the event buffer before the guest can parse it.
    assert_eq!(
        host.buffers().with_windows_watcher(buffer, |_| Ok(())),
        Err(AsyncHostError::Badf)
    );
    assert!(host.io_result_get_status(result, directory_handle).unwrap() > 0);
    host.free_io_result(result).unwrap();
    host.buffers()
        .with_windows_watcher(buffer, |buffer| {
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
        request.io.request_cancel().unwrap();
        let mut bytes = 0;
        unsafe {
            windows_sys::Win32::System::IO::GetOverlappedResult(
                raw_overlapped_handle(&file).unwrap(),
                request.io.overlapped_ptr(),
                &mut bytes,
                1,
            );
        }
        assert!(request.io.native_completed());
        request.io.clear_pending();
    }
    host.free_io_result(result).unwrap();
    host.buffers().free_windows_watcher(buffer).unwrap();
    host.close_fd(directory_handle).unwrap();
    host.poll_destroy(poll).unwrap();
    assert_eq!(completed, Ok(1));
}

#[test]
fn denied_watch_releases_its_request_and_returns_the_leased_buffer() {
    use crate::async_host::tests::host_with_policy;
    use crate::filesystem::Job as FilesystemJob;

    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("write_only");
    std::fs::write(&file, b"contents").unwrap();
    let policy = directory.path().join("policy.toml");
    std::fs::write(&policy, "[fs]\nwrite = ['write_only']\n").unwrap();
    let host = host_with_policy(&policy);
    let open = host
        .insert_job(FilesystemJob::open_legacy(
            file.into_os_string(),
            1,
            0,
            false,
            0,
            0,
        ))
        .unwrap();
    host.run_job(open).unwrap();
    let file = host.open_job_get_fd(open).unwrap();
    let buffer = host.buffers().allocate_windows_watcher();
    let request = host.make_read_dir_changes_io_result(buffer).unwrap();

    // Watch requires read authority even though this Resource was acquired
    // successfully. Denial must not leave a pending native request behind.
    assert_eq!(
        host.read_dir_changes_io_result(file, request),
        Err(AsyncHostError::PermissionDenied)
    );
    host.close_fd(file).unwrap();
    host.free_io_result(request).unwrap();
    host.buffers()
        .with_windows_watcher(buffer, |_| Ok(()))
        .unwrap();
    host.buffers().free_windows_watcher(buffer).unwrap();
    host.free_job(open).unwrap();
    assert_eq!(host.leak_summary(), None);
}
