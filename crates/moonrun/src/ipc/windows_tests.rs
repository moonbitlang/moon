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
use std::io::Seek;

fn pair() -> (Connection, Connection) {
    let (mut listener, _wake, endpoint) = Listener::bind().unwrap();
    let worker = std::thread::spawn(move || listener.accept().unwrap());
    let client = Connection::connect(endpoint).unwrap();
    (client, worker.join().unwrap())
}

#[test]
fn transport_lifecycle() {
    const CHILD: &str = "MOONRUN_WINDOWS_IPC_TEST";
    if std::env::var_os(CHILD).is_none() {
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ipc::transport::tests::transport_lifecycle",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
        return;
    }
    // An early disconnect must not stop admission or an established connection.
    let (mut listener, wake, endpoint) = Listener::bind().unwrap();
    let first = Connection::connect(&endpoint).unwrap();
    let retained = listener.accept().unwrap();
    drop(Connection::connect(&endpoint).unwrap());
    let worker = std::thread::spawn(move || {
        let healthy = listener.accept().unwrap();
        let mut byte = [0];
        (&healthy).read_exact(&mut byte).unwrap();
        assert_eq!(byte, *b"H");
        (&retained).write_all(b"R").unwrap();
        assert!(listener.accept().is_err());
    });
    let healthy = Connection::connect(&endpoint).unwrap();
    (&healthy).write_all(b"H").unwrap();
    let mut byte = [0];
    (&first).read_exact(&mut byte).unwrap();
    assert_eq!(byte, *b"R");
    wake.close();
    worker.join().unwrap();
    assert!(Connection::connect(&endpoint).is_err());

    // Acquired handles are non-inheritable and share file identity/offset.
    let (client, server) = pair();
    let files = std::array::from_fn(|_| tempfile::tempfile().unwrap());
    let worker = std::thread::spawn(move || receive_stdio(&server).unwrap());
    send_stdio(&client, &files).unwrap();
    let mut received = worker.join().unwrap();
    for (original, received) in files.iter().zip(&mut received) {
        let received = received.as_mut().unwrap();
        let mut flags = 0;
        assert_ne!(
            unsafe { GetHandleInformation(received.as_raw_handle(), &mut flags) },
            0
        );
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
        received.write_all(b"shared").unwrap();
        assert_eq!(original.try_clone().unwrap().stream_position().unwrap(), 6);
    }

    // Reject non-file handles after duplication, releasing the acquired owner.
    for _ in 0..4 {
        let (client, server) = pair();
        let event = Event::new().unwrap();
        let worker = std::thread::spawn(move || assert!(receive_stdio(&server).is_err()));
        let mut invalid = [0; 28];
        invalid[0] = b'R';
        invalid[2..10].copy_from_slice(&(event.raw() as usize as u64).to_le_bytes());
        (&client).write_all(&invalid).unwrap();
        worker.join().unwrap();
        assert_eq!((&client).read(&mut [0]).unwrap(), 0);
    }

    // Closing shared ownership wakes a blocked read even if a clone remains.
    let (client, server) = pair();
    let clone = client.try_clone().unwrap();
    let worker = std::thread::spawn(move || assert!((&client).read(&mut [0]).is_err()));
    clone.shutdown(Shutdown::Both).unwrap();
    worker.join().unwrap();
    assert_eq!((&server).read(&mut [0]).unwrap(), 0);

    // Final replies cannot block indefinitely behind a non-reading peer.
    let (_client, server) = pair();
    assert_eq!(
        write_all_deadline(&server, &vec![0; 900_000])
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );

    let path = PathBuf::from(std::ffi::OsString::from_wide(&[
        b'C' as u16,
        b':' as u16,
        b'\\' as u16,
        0xd800,
    ]));
    let invocation = crate::ipc::Invocation {
        program: path.clone(),
        working_directory: path,
        args: vec![],
        environment: vec![("KEY".into(), std::ffi::OsString::from_wide(&[0xdfff]))],
        inherited_policy: None,
        no_stack_trace: false,
        test_args: None,
        stack_size: None,
    };
    let bytes = crate::ipc::wire::encode(&invocation).unwrap();
    let decoded: crate::ipc::Invocation = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded.program, invocation.program);
    assert_eq!(decoded.environment, invocation.environment);
}

#[test]
fn shared_console_reuses_host_streams_and_keeps_redirection() {
    const CHILD: &str = "MOONRUN_WINDOWS_IPC_CONSOLE_TEST";
    if std::env::var_os(CHILD).is_none() {
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ipc::transport::tests::shared_console_reuses_host_streams_and_keeps_redirection",
            ])
            .env(CHILD, "1")
            .timeout(Duration::from_secs(20))
            .assert()
            .success();
        return;
    }
    let standards = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];
    let original = standards.map(|id| unsafe { GetStdHandle(id) });
    unsafe {
        FreeConsole();
    }
    assert_ne!(unsafe { AllocConsole() }, 0);
    assert_ne!(unsafe { SetConsoleCP(437) }, 0);
    assert_ne!(unsafe { SetConsoleOutputCP(437) }, 0);
    let input = owned(unsafe {
        CreateFileW(
            windows_sys::w!("CONIN$"),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    })
    .unwrap();
    let output = owned(unsafe {
        CreateFileW(
            windows_sys::w!("CONOUT$"),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    })
    .unwrap();
    for (id, handle) in standards.into_iter().zip([
        input.as_raw_handle(),
        output.as_raw_handle(),
        output.as_raw_handle(),
    ]) {
        assert_ne!(unsafe { SetStdHandle(id, handle) }, 0);
    }
    assert_ne!(unsafe { SetConsoleMode(input.as_raw_handle(), 0) }, 0);
    let mut redirected = tempfile::tempfile().unwrap();
    let files = [
        File::from(input.try_clone().unwrap()),
        redirected.try_clone().unwrap(),
        File::from(output.try_clone().unwrap()),
    ];
    let (client, server) = pair();
    let worker = std::thread::spawn(move || receive_stdio(&server).unwrap());
    send_stdio(&client, &files).unwrap();
    let bindings = worker.join().unwrap();
    assert!(bindings[0].is_none() && bindings[1].is_some() && bindings[2].is_none());
    let stdio = crate::runtime::Stdio::bound(bindings);
    assert_eq!(
        stdio.raw(crate::runtime::StdioStream::Stdin).unwrap(),
        input.as_raw_handle()
    );
    assert_eq!(
        stdio.raw(crate::runtime::StdioStream::Stderr).unwrap(),
        output.as_raw_handle()
    );
    stdio
        .with_stdout(|out| out.write_all(b"redirected\xff"))
        .unwrap();
    redirected.rewind().unwrap();
    let mut bytes = Vec::new();
    redirected.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"redirected\xff");
    let text = "λé";
    for byte in text.as_bytes() {
        stdio
            .with_stderr(|out| out.write_all(std::slice::from_ref(byte)))
            .unwrap();
    }
    let wide: Vec<_> = text.encode_utf16().collect();
    let mut observed = vec![0; wide.len()];
    let mut count = 0;
    assert_ne!(
        unsafe {
            ReadConsoleOutputCharacterW(
                output.as_raw_handle(),
                observed.as_mut_ptr(),
                observed.len() as u32,
                COORD { X: 0, Y: 0 },
                &mut count,
            )
        },
        0
    );
    assert_eq!(observed, wide);
    let text = "λé中文🦀";
    let wide: Vec<_> = text.encode_utf16().collect();
    let events: Vec<_> = wide
        .iter()
        .map(|&unit| INPUT_RECORD {
            EventType: KEY_EVENT as u16,
            Event: INPUT_RECORD_0 {
                KeyEvent: KEY_EVENT_RECORD {
                    bKeyDown: 1,
                    wRepeatCount: 1,
                    wVirtualKeyCode: 0,
                    wVirtualScanCode: 0,
                    uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                    dwControlKeyState: 0,
                },
            },
        })
        .collect();
    assert_ne!(
        unsafe {
            WriteConsoleInputW(
                input.as_raw_handle(),
                events.as_ptr(),
                events.len() as u32,
                &mut count,
            )
        },
        0
    );
    let mut bytes = vec![0; text.len()];
    // Deliberately read one byte at a time across multibyte Unicode characters.
    stdio
        .with_stdin(|input| -> io::Result<()> {
            for byte in &mut bytes {
                input.read_exact(std::slice::from_mut(byte))?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(bytes, text.as_bytes());
    assert_ne!(
        unsafe {
            SetConsoleMode(
                input.as_raw_handle(),
                ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT,
            )
        },
        0
    );
    let eof = INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: 1,
                wRepeatCount: 1,
                wVirtualKeyCode: b'Z' as u16,
                wVirtualScanCode: 0,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: 0x1a },
                dwControlKeyState: LEFT_CTRL_PRESSED,
            },
        },
    };
    assert_ne!(
        unsafe { WriteConsoleInputW(input.as_raw_handle(), &eof, 1, &mut count) },
        0
    );
    assert_eq!(stdio.with_stdin(|input| input.read(&mut [0])).unwrap(), 0);
    assert_eq!(unsafe { GetConsoleCP() }, 437);
    assert_eq!(unsafe { GetConsoleOutputCP() }, 437);
    // A client console must not silently become the host's redirected stdout.
    assert_ne!(
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, redirected.as_raw_handle()) },
        0
    );
    let (client, server) = pair();
    let worker = std::thread::spawn(move || {
        assert_eq!(
            receive_stdio(&server).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        )
    });
    let files = [
        File::from(input.try_clone().unwrap()),
        File::from(output.try_clone().unwrap()),
        File::from(output.try_clone().unwrap()),
    ];
    assert!(send_stdio(&client, &files).is_err());
    worker.join().unwrap();
    for (id, handle) in standards.into_iter().zip(original) {
        assert_ne!(unsafe { SetStdHandle(id, handle) }, 0);
    }
}

use std::os::windows::ffi::OsStringExt;

#[cfg(feature = "v8")]
#[test]
fn session_shutdown_joins_active_execution() {
    const CHILD: &str = "MOONRUN_WINDOWS_IPC_SHUTDOWN_TEST";
    if std::env::var_os(CHILD).is_none() {
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ipc::transport::tests::session_shutdown_joins_active_execution",
            ])
            .env(CHILD, "1")
            .timeout(Duration::from_secs(20))
            .assert()
            .success();
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("spin.wasm");
    std::fs::write(&program, wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1) (data (i32.const 16) "R")
        (func (export "_start")
            i32.const 0 i32.const 16 i32.store
            i32.const 4 i32.const 1 i32.store
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write drop
            (loop $forever br $forever)))"#).unwrap()).unwrap();
    let server = crate::ipc::server::IpcServer::start(crate::Engine::default(), None).unwrap();
    let endpoint = server.endpoint();
    let (mut ready, writer) = std::io::pipe().unwrap();
    let run = crate::ipc::ClientRun::submit(
        Connection::connect(&endpoint).unwrap(),
        crate::ipc::Invocation {
            program,
            working_directory: directory.path().to_owned(),
            args: vec![],
            environment: vec![],
            inherited_policy: None,
            no_stack_trace: false,
            test_args: None,
            stack_size: None,
        },
        [
            tempfile::tempfile().unwrap(),
            File::from(OwnedHandle::from(writer)),
            tempfile::tempfile().unwrap(),
        ],
    )
    .unwrap();
    let mut byte = [0];
    ready.read_exact(&mut byte).unwrap();
    assert_eq!(byte, *b"R");
    drop(server);
    assert!(run.wait().is_err());
    assert!(Connection::connect(&endpoint).is_err());
}
