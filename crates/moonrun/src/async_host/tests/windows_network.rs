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

//! Windows socket submission and completion through the Async Host boundary.

use super::*;
use crate::async_sys::socket as sys;
use windows_sys::Win32::Foundation::ERROR_IO_PENDING;

fn loopback(port: u16) -> Vec<u8> {
    let mut addr = vec![0; sys::ipv4_addr_size() as usize];
    sys::init_ip_addr(&mut addr, 0x7f000001, u32::from(port)).unwrap();
    addr
}

fn local_addr(host: &AsyncHost, socket: HostHandle) -> AsyncHostResult<Vec<u8>> {
    let mut addr = loopback(0);
    host.with_raw_socket(socket, |socket| sys::getsockname(socket, &mut addr))?;
    Ok(addr)
}

// Registering with IOCP suppresses completion packets on immediate success.
// Each wait occurs while only this result can complete on the host's port;
// e.g. the UDP peer sends its reply only after the outbound send is complete.
fn complete(
    host: &AsyncHost,
    poll: u64,
    socket: HostHandle,
    result: u64,
    submission: AsyncHostResult<i32>,
) -> i32 {
    match submission {
        Ok(bytes) => bytes,
        Err(error) => {
            assert_eq!(error, AsyncHostError::Native(ERROR_IO_PENDING as i32));
            assert_eq!(
                host.poll_wait(poll, 10_000, &Default::default()).unwrap(),
                1
            );
            let event = host.poll_get_event(poll, 0).unwrap();
            assert_eq!(host.poll_event_io_result(event).unwrap(), result);
            host.io_result_get_status(result, socket).unwrap()
        }
    }
}

#[test]
fn denied_connect_and_send_do_not_bind_or_retain_sockets() {
    // std initializes Winsock once for the process.
    let peer = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let policy = policy::load_json(br#"{}"#, &Env::ambient()).unwrap().0;
    let host = test_host(Some(policy), Env::ambient());
    let tcp = host.make_tcp_socket(4).unwrap();
    let udp = host.make_udp_socket(4, false).unwrap();
    let tcp_before = local_addr(&host, tcp);
    let udp_before = local_addr(&host, udp);
    let mut memory = loopback(peer.local_addr().unwrap().port());
    let addr_len = memory.len() as u32;
    let connect = host
        .make_connect_io_result(memory.as_mut_slice(), 0, addr_len)
        .unwrap();
    memory.extend_from_slice(b"hello");
    let send = host
        .make_socket_with_addr_write_io_result(
            memory.as_mut_slice(),
            addr_len,
            0,
            5,
            0,
            0,
            addr_len,
        )
        .unwrap();

    assert_eq!(
        host.connect_io_result(tcp, connect),
        Err(AsyncHostError::PermissionDenied)
    );
    assert_eq!(
        host.write_io_result(udp, send),
        Err(AsyncHostError::PermissionDenied)
    );
    assert_eq!(local_addr(&host, tcp), tcp_before);
    assert_eq!(local_addr(&host, udp), udp_before);
    // Denial cannot leave a pending operation that prevents either release.
    host.free_io_result(connect).unwrap();
    host.free_io_result(send).unwrap();
    host.close_fd(tcp).unwrap();
    host.close_fd(udp).unwrap();
}

#[test]
fn udp_pending_receive_keeps_host_buffers_and_socket_until_completion() {
    let peer = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    peer.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let host = default_host();
    let poll = host.poll_create().unwrap();
    let socket = host.make_udp_socket(4, false).unwrap();
    host.poll_register(poll, socket, false).unwrap();
    host.bind(socket, &loopback(0)).unwrap();
    let mut memory = loopback(peer.local_addr().unwrap().port());
    let addr_len = memory.len() as u32;
    let read = host
        .make_socket_with_addr_read_io_result(memory.as_mut_slice(), 5, 0, 0, addr_len)
        .unwrap();
    let receiving = host.read_io_result(socket, read);
    assert_eq!(
        receiving,
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    );
    assert_eq!(host.close_fd(socket), Err(AsyncHostError::Inval));
    assert_eq!(host.free_io_result(read), Err(AsyncHostError::Inval));

    memory.extend_from_slice(b"hello");
    let write = host
        .make_socket_with_addr_write_io_result(
            memory.as_mut_slice(),
            addr_len,
            0,
            5,
            0,
            0,
            addr_len,
        )
        .unwrap();
    memory.fill(0); // Submission uses the copied host data and destination.
    assert_eq!(
        complete(
            &host,
            poll,
            socket,
            write,
            host.write_io_result(socket, write)
        ),
        5
    );
    let mut packet = [0; 8];
    let (len, source) = peer.recv_from(&mut packet).unwrap();
    assert_eq!(&packet[..len], b"hello");
    peer.send_to(b"world", source).unwrap();
    assert_eq!(complete(&host, poll, socket, read, receiving), 5);

    // Copy into a fresh guest-memory view only after completion.
    let mut output = vec![0; addr_len as usize + 5];
    host.io_result_copy_read_with_addr(output.as_mut_slice(), read, addr_len, 0, 5, 0, addr_len)
        .unwrap();
    assert_eq!(&output[addr_len as usize..], b"world");
    assert_eq!(
        &output[..addr_len as usize],
        loopback(peer.local_addr().unwrap().port())
    );
    host.free_io_result(read).unwrap();
    host.free_io_result(write).unwrap();
    host.close_fd(socket).unwrap();
}

#[test]
fn tcp_accept_retains_both_sockets_and_connect_needs_no_bind_grant() {
    let _wsa = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let server = default_host();
    let server_poll = server.poll_create().unwrap();
    let listener = server.make_tcp_socket(4).unwrap();
    server.bind(listener, &loopback(0)).unwrap();
    server.listen(listener).unwrap();
    let accepted = server.make_tcp_socket(4).unwrap();
    for socket in [listener, accepted] {
        server.poll_register(server_poll, socket, false).unwrap();
    }
    let mut addr = local_addr(&server, listener).unwrap();
    let addr_len = addr.len() as u32;
    let accept = server.make_accept_io_result(addr_len).unwrap();
    let accepting = server.accept_io_result(listener, accepted, accept);
    assert_eq!(
        accepting,
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    );
    for socket in [listener, accepted] {
        assert_eq!(server.close_fd(socket), Err(AsyncHostError::Inval));
    }
    assert_eq!(server.free_io_result(accept), Err(AsyncHostError::Inval));

    let policy = policy::load_json(br#"{"net":{"connect":["127.0.0.1:*"]}}"#, &Env::ambient())
        .unwrap()
        .0;
    let client = test_host(Some(policy), Env::ambient());
    let client_poll = client.poll_create().unwrap();
    let connected = client.make_tcp_socket(4).unwrap();
    client.poll_register(client_poll, connected, false).unwrap();
    assert_eq!(
        client.bind(connected, &loopback(0)),
        Err(AsyncHostError::PermissionDenied)
    );
    let connect = client
        .make_connect_io_result(addr.as_mut_slice(), 0, addr_len)
        .unwrap();
    complete(
        &client,
        client_poll,
        connected,
        connect,
        client.connect_io_result(connected, connect),
    );
    complete(&server, server_poll, listener, accept, accepting);
    client.setup_connected_socket(connected).unwrap();
    server.setup_accepted_socket(listener, accepted).unwrap();

    // Exercise the address-free WSARecv/WSASend paths after ConnectEx/AcceptEx.
    let read = server.make_socket_read_io_result(1, 0).unwrap();
    let receiving = server.read_io_result(accepted, read);
    let write = client
        .make_socket_write_io_result(b"x".to_vec().as_mut_slice(), 0, 0, 1, 0)
        .unwrap();
    assert_eq!(
        complete(
            &client,
            client_poll,
            connected,
            write,
            client.write_io_result(connected, write)
        ),
        1
    );
    assert_eq!(complete(&server, server_poll, accepted, read, receiving), 1);
    let mut output = [0; 1];
    server
        .io_result_copy_read(output.as_mut_slice(), read, 0, 0, 1)
        .unwrap();
    assert_eq!(&output, b"x");

    for result in [accept, read] {
        server.free_io_result(result).unwrap();
    }
    for result in [connect, write] {
        client.free_io_result(result).unwrap();
    }
    for socket in [listener, accepted] {
        server.close_fd(socket).unwrap();
    }
    client.close_fd(connected).unwrap();
}
