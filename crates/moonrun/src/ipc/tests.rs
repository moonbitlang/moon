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
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::ffi::OsStringExt;

#[test]
fn wire_preserves_non_utf8_caller_state() {
    let path = PathBuf::from(OsString::from_vec(b"/a-\xff".to_vec()));
    let invocation = Invocation {
        program: path.clone(),
        args: vec!["a".into()],
        working_directory: path,
        environment: vec![("K".into(), OsString::from_vec(vec![255]))],
        inherited_policy: None,
        no_stack_trace: false,
        test_args: None,
        stack_size: None,
    };
    let bytes = wire::encode(&invocation).unwrap();
    let decoded: Invocation = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded.program, invocation.program);
    assert_eq!(decoded.working_directory, invocation.working_directory);
    assert_eq!(decoded.environment, invocation.environment);
}

#[test]
fn framing_rejects_oversize_and_truncated_input() {
    for bytes in [u32::MAX.to_be_bytes().to_vec(), vec![0, 0, 0, 2, b'{']] {
        let (mut sender, receiver) = UnixStream::pair().unwrap();
        sender.write_all(&bytes).unwrap();
        sender.shutdown(Shutdown::Write).unwrap();
        assert!(wire::read::<wire::Reply>(&receiver).is_err());
    }
}

#[test]
fn transferred_descriptors_are_cloexec_and_keep_shared_file_state() {
    let (sender, receiver) = UnixStream::pair().unwrap();
    let files = std::array::from_fn(|_| tempfile::tempfile().unwrap());
    wire::send_stdio(&sender, &files).unwrap();
    let mut received = wire::receive_stdio(&receiver).unwrap();
    for (original, received) in files.iter().zip(&mut received) {
        assert!(
            rustix::io::fcntl_getfd(received.as_fd())
                .unwrap()
                .contains(rustix::io::FdFlags::CLOEXEC)
        );
        received.write_all(b"shared-offset").unwrap();
        // Descriptor passing shares the open file description, not a reopened file.
        use std::io::Seek;
        assert_eq!(original.try_clone().unwrap().stream_position().unwrap(), 13);
    }
}

#[test]
fn malformed_descriptor_transfers_release_every_received_owner() {
    use rustix::net::{SendAncillaryBuffer, SendAncillaryMessage, SendFlags};
    use std::io::IoSlice;
    use std::mem::MaybeUninit;
    for count in [1, 4, 16, 200] {
        let (sender, receiver) = UnixStream::pair().unwrap();
        let (writer, mut reader) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let files: Vec<File> = (0..count)
            .map(|_| File::from(OwnedFd::from(writer.try_clone().unwrap())))
            .collect();
        let fds: Vec<_> = files.iter().map(AsFd::as_fd).collect();
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(200))];
        let mut ancillary = SendAncillaryBuffer::new(&mut space);
        assert!(ancillary.push(SendAncillaryMessage::ScmRights(&fds)));
        rustix::net::sendmsg(
            &sender,
            &[IoSlice::new(b"R")],
            &mut ancillary,
            SendFlags::empty(),
        )
        .unwrap();
        assert!(wire::receive_stdio(&receiver).is_err());
        drop(files);
        drop(writer);
        assert_eq!(
            reader.read(&mut [0]).unwrap(),
            0,
            "received fd leaked after rejection"
        );
    }
}

#[cfg(feature = "v8")]
#[test]
fn bad_version_is_rejected_before_loading_the_program() {
    let engine = Engine::default();
    let dir = tempfile::tempdir().unwrap();
    let invocation = Invocation {
        program: dir.path().join("missing.wasm"),
        args: vec![],
        working_directory: dir.path().to_owned(),
        environment: vec![],
        inherited_policy: None,
        no_stack_trace: false,
        test_args: None,
        stack_size: None,
    };
    let (client, server) = UnixStream::pair().unwrap();
    let host = std::thread::spawn(move || serve(&engine, server, RunOptions::default()));
    wire::send_stdio(
        &client,
        &std::array::from_fn(|_| tempfile::tempfile().unwrap()),
    )
    .unwrap();
    wire::write(
        &client,
        &wire::Request {
            version: wire::VERSION + 1,
            invocation,
        },
    )
    .unwrap();
    let reply = wire::read::<wire::Reply>(&client).unwrap();
    host.join().unwrap().unwrap();
    let wire::Reply::Failed(error) = reply else {
        panic!("bad version accepted")
    };
    assert!(error.contains("unsupported IPC protocol version"));
}

#[cfg(not(feature = "v8"))]
#[test]
fn unsupported_backends_report_an_error_before_execution() {
    let (client, server) = UnixStream::pair().unwrap();
    serve(&Engine::default(), server, RunOptions::default()).unwrap();
    let wire::Reply::Failed(error) = wire::read(&client).unwrap() else {
        panic!("unsupported backend accepted IPC execution")
    };
    assert!(error.contains("termination support"));
}
