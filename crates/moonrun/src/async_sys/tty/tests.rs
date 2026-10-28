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
use std::io::{Read, Seek};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use windows_sys::Win32::Foundation::{ERROR_OPERATION_ABORTED, GENERIC_READ, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::Threading::SetEvent;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_MENU;

#[test]
fn windows_tty_lifecycle() {
    use windows_sys::Win32::Foundation::{
        HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Threading::*;

    // CREATE_NEW_CONSOLE plus SW_HIDE prevents default-terminal delegation and
    // user input from interfering. std::process::Command cannot set wShowWindow.
    let mut output = tempfile::tempfile().unwrap();
    let output_handle = output.as_raw_handle();
    assert_ne!(
        unsafe { SetHandleInformation(output_handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
        0
    );
    let mut command: Vec<u16> = format!(
        "\"{}\" --exact async_sys::tty::tests::windows_tty_lifecycle_child --ignored --nocapture",
        std::env::current_exe().unwrap().display()
    )
    .encode_utf16()
    .chain([0])
    .collect();
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESHOWWINDOW | STARTF_USESTDHANDLES,
        wShowWindow: 0, // SW_HIDE
        hStdInput: output_handle,
        hStdOutput: output_handle,
        hStdError: output_handle,
        ..unsafe { std::mem::zeroed() }
    };
    let mut process = unsafe { std::mem::zeroed::<PROCESS_INFORMATION>() };
    assert_ne!(
        unsafe {
            CreateProcessW(
                std::ptr::null(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_NEW_CONSOLE,
                std::ptr::null(),
                std::ptr::null(),
                &startup,
                &mut process,
            )
        },
        0
    );
    let process_handle = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
    let _thread = unsafe { OwnedHandle::from_raw_handle(process.hThread) };
    let wait = unsafe { WaitForSingleObject(process_handle.as_raw_handle(), 30_000) };
    if wait != WAIT_OBJECT_0 {
        unsafe { TerminateProcess(process_handle.as_raw_handle(), 1) };
        panic!("TTY subprocess did not finish within 30 seconds");
    }
    let mut code = 0;
    assert_ne!(
        unsafe { GetExitCodeProcess(process_handle.as_raw_handle(), &mut code) },
        0
    );
    output.rewind().unwrap();
    let mut log = String::new();
    output.read_to_string(&mut log).unwrap();
    assert_eq!(code, 0, "{log}");
}

#[test]
#[ignore = "runs inside windows_tty_lifecycle's private hidden console"]
fn windows_tty_lifecycle_child() {
    let tty = open().expect("open the Windows controlling terminal");
    let packed_size = size(&tty.output).unwrap();
    assert!(packed_size >> 32 > 0);
    assert!(packed_size & 0xffff_ffff > 0);
    let mut info = unsafe { std::mem::zeroed::<CONSOLE_SCREEN_BUFFER_INFO>() };
    assert_ne!(
        unsafe { GetConsoleScreenBufferInfo(raw(&tty.control), &mut info) },
        0
    );
    assert_eq!(
        packed_size >> 32,
        i64::from(info.srWindow.Bottom - info.srWindow.Top + 1)
    );
    assert_eq!(
        packed_size & 0xffff_ffff,
        i64::from(info.srWindow.Right - info.srWindow.Left + 1)
    );
    raw_mode_roundtrip(&tty);
    raw_mode_rolls_back_input_on_output_failure(&tty);
    guest_owns_raw_state(&tty);
    console_input_records(tty);
    console_read_cancellation();
    // No controlling console must remain an error, rather than falling back to stdio.
    assert_ne!(unsafe { FreeConsole() }, 0);
    assert!(open().is_err());
}

fn raw(resource: &Resource) -> std::os::windows::io::RawHandle {
    resource.as_handle().unwrap().as_raw_handle()
}

fn mode(resource: &Resource) -> u32 {
    let mut mode = 0;
    assert_ne!(unsafe { GetConsoleMode(raw(resource), &mut mode) }, 0);
    mode
}

fn raw_mode_roundtrip(tty: &TtyResources) {
    let saved = (mode(&tty.input), mode(&tty.control));
    let state = enter_raw(&tty.input, &tty.control).unwrap();
    assert_eq!(state.len(), state_size());
    assert_eq!(&state[..4], saved.0.to_le_bytes());
    assert_eq!(&state[4..], saved.1.to_le_bytes());
    assert_eq!(
        mode(&tty.input)
            & (ENABLE_ECHO_INPUT
                | ENABLE_LINE_INPUT
                | ENABLE_PROCESSED_INPUT
                | ENABLE_QUICK_EDIT_MODE),
        0
    );
    let input_flags = ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_WINDOW_INPUT | ENABLE_EXTENDED_FLAGS;
    assert_eq!(mode(&tty.input) & input_flags, input_flags);
    let output_flags =
        ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN;
    assert_eq!(mode(&tty.control) & output_flags, output_flags);
    let raw_modes = (mode(&tty.input), mode(&tty.control));
    for len in [0, state_size() - 1, state_size() + 1] {
        assert_eq!(
            leave_raw(&tty.input, &tty.control, &vec![0; len]),
            Err(crate::async_host::AsyncHostError::Inval)
        );
        assert_eq!((mode(&tty.input), mode(&tty.control)), raw_modes);
    }
    leave_raw(&tty.input, &tty.control, &state).unwrap();
    assert_eq!((mode(&tty.input), mode(&tty.control)), saved);
    // State is reusable bytes, not a consumed host Handle.
    leave_raw(&tty.input, &tty.control, &state).unwrap();
    assert_eq!((mode(&tty.input), mode(&tty.control)), saved);
}

fn raw_mode_rolls_back_input_on_output_failure(tty: &TtyResources) {
    let output = unsafe {
        CreateFileW(
            windows_sys::w!("CONOUT$"),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(output, INVALID_HANDLE_VALUE);
    let output = Resource::new(output);
    let saved = mode(&tty.input);
    assert!(enter_raw(&tty.input, &output).is_err());
    assert_eq!(mode(&tty.input), saved);
}

fn guest_owns_raw_state(tty: &TtyResources) {
    let saved = (mode(&tty.input), mode(&tty.control));
    let engine = crate::Engine::default();
    for trap in [false, true] {
        let finish = if trap {
            "unreachable"
        } else {
            "(call $check (call $leave (i64.load (i32.const 0)) (i64.load (i32.const 16)) (i32.const 41)))"
        };
        let source = format!(
            r#"(module
            (import "moonbitlang/async" "tty/open" (func $open (param i32 i32) (result i32)))
            (import "moonbitlang/async" "tty/state_size" (func $state_size (result i32)))
            (import "moonbitlang/async" "tty/enter_raw" (func $enter (param i64 i64 i32 i32) (result i32)))
            (import "moonbitlang/async" "tty/leave_raw" (func $leave (param i64 i64 i32) (result i32)))
            (import "moonbitlang/async" "tty/size" (func $size (param i64) (result i64)))
            (import "moonbitlang/async" "fd_util/close_fd" (func $close (param i64) (result i32)))
            (memory (export "memory") 1)
            (func $check (param $err i32) (if (local.get $err) (then unreachable)))
            (func (export "_start")
                (call $check (i32.ne (call $state_size) (i32.const 8)))
                (call $check (call $open (i32.const 0) (i32.const 4)))
                (call $check (i64.eq (i64.load (i32.const 0)) (i64.load (i32.const 16))))
                (call $check (i64.eq (i64.load (i32.const 16)) (i64.load (i32.const 24))))
                (call $check (i64.le_s (call $size (i64.load (i32.const 16))) (i64.const 0)))
                ;; An invalid destination must fail before changing either mode.
                (call $check (i32.ne (call $enter (i64.load (i32.const 0)) (i64.load (i32.const 16)) (i32.const 0) (i32.const 65532)) (i32.const -1)))
                (call $check (call $enter (i64.load (i32.const 0)) (i64.load (i32.const 16)) (i32.const 0) (i32.const 32)))
                (call $check (i32.ne (i32.load (i32.const 32)) (i32.const {input_mode})))
                (call $check (i32.ne (i32.load (i32.const 36)) (i32.const {output_mode})))
                ;; Move the bytes to an unaligned buffer and erase the original.
                (memory.copy (i32.const 41) (i32.const 32) (call $state_size))
                (memory.fill (i32.const 32) (i32.const 0) (call $state_size))
                (call $check (i32.ne (call $leave (i64.load (i32.const 0)) (i64.load (i32.const 16)) (i32.const 65532)) (i32.const -1)))
                {finish}
                (call $check (call $close (i64.load (i32.const 0))))
                (call $check (call $close (i64.load (i32.const 8))))
                (call $check (call $close (i64.load (i32.const 16))))
                (call $check (call $close (i64.load (i32.const 24))))))"#,
            input_mode = saved.0,
            output_mode = saved.1,
        );
        let module = engine
            .compile("tty-modes.wasm", wat::parse_str(source).unwrap())
            .unwrap();
        let outcome = engine.run(&module, crate::RunOptions::default()).unwrap();
        assert_eq!(
            outcome,
            if trap {
                crate::RunOutcome::Exited(1)
            } else {
                crate::RunOutcome::Completed
            }
        );
        if trap {
            // Like upstream, dropping the Runtime closes Resources but does not
            // restore a configuration whose only saved copy belonged to the guest.
            assert_ne!((mode(&tty.input), mode(&tty.control)), saved);
            assert_ne!(unsafe { SetConsoleMode(raw(&tty.input), saved.0) }, 0);
            assert_ne!(unsafe { SetConsoleMode(raw(&tty.control), saved.1) }, 0);
        } else {
            assert_eq!((mode(&tty.input), mode(&tty.control)), saved);
        }
    }
}

fn key(unit: u16, down: bool, virtual_key: u16) -> INPUT_RECORD {
    INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: i32::from(down),
                wRepeatCount: 1,
                wVirtualKeyCode: virtual_key,
                wVirtualScanCode: 0,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                dwControlKeyState: 0,
            },
        },
    }
}

fn inject(input: &Resource, records: &[INPUT_RECORD]) {
    let mut count = 0;
    assert_ne!(
        unsafe {
            WriteConsoleInputW(
                raw(input),
                records.as_ptr(),
                records.len() as u32,
                &mut count,
            )
        },
        0
    );
    assert_eq!(count as usize, records.len());
}

fn console_input_records(tty: TtyResources) {
    use crate::filesystem::Job;
    let state = enter_raw(&tty.input, &tty.control).unwrap();
    let input = Arc::new(tty.input);
    let wakeup = Arc::new(tty.wakeup);
    assert_ne!(unsafe { FlushConsoleInputBuffer(raw(&input)) }, 0);
    let resize = INPUT_RECORD {
        EventType: WINDOW_BUFFER_SIZE_EVENT as u16,
        Event: INPUT_RECORD_0 {
            WindowBufferSizeEvent: WINDOW_BUFFER_SIZE_RECORD {
                dwSize: COORD { X: 80, Y: 25 },
            },
        },
    };
    let records = [
        key(0x03bb, true, 0),
        key(b'x' as u16, false, 0),
        key(0, true, 0),
        key(0xd83d, false, VK_MENU),
        key(0xde00, false, VK_MENU),
        INPUT_RECORD {
            EventType: FOCUS_EVENT as u16,
            Event: INPUT_RECORD_0 {
                FocusEvent: FOCUS_EVENT_RECORD { bSetFocus: 1 },
            },
        },
        INPUT_RECORD {
            EventType: FOCUS_EVENT as u16,
            Event: INPUT_RECORD_0 {
                FocusEvent: FOCUS_EVENT_RECORD { bSetFocus: 0 },
            },
        },
        resize,
    ];
    inject(&input, &records);
    // The next read must clear cancellation left by a previous Job.
    assert_ne!(unsafe { SetEvent(raw(&wakeup)) }, 0);
    let mut job = Job::tty_read_console_input(input.clone(), wakeup.clone(), 24);
    assert_eq!(job.run().unwrap(), 9);
    assert!(job.tty_read_console_input_resized().unwrap());
    let mut memory = [0_u8; 18];
    job.copy_read_console_result(0, &mut memory, 0, 0, 9)
        .unwrap();
    let text: Vec<u16> = memory
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(
        text,
        [0x03bb, 0xd83d, 0xde00, 0x1b, 0x5b, 0x49, 0x1b, 0x5b, 0x4f]
    );
    inject(&input, &[resize]);
    let mut short = Job::tty_read_console_input(input.clone(), wakeup.clone(), 2);
    assert_eq!(short.run(), Err(crate::async_host::AsyncHostError::Inval));
    let mut job = Job::tty_read_console_input(input.clone(), wakeup, 3);
    assert_eq!(job.run().unwrap(), -1);
    assert!(job.tty_read_console_input_resized().unwrap());
    leave_raw(&input, &tty.control, &state).unwrap();
}

fn console_read_cancellation() {
    use crate::async_sys::internal::event_loop::thread_pool::*;
    use slotmap::Key;
    for before_read in [true, false] {
        let tty = open().unwrap();
        assert_ne!(unsafe { FlushConsoleInputBuffer(raw(&tty.input)) }, 0);
        let input = Arc::new(tty.input);
        let wakeup = Arc::new(tty.wakeup);
        let job = crate::filesystem::Job::tty_read_console_input(input, wakeup, 3);
        let (started, ready) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let (completed, results) = mpsc::channel();
        let worker = spawn_worker(
            WorkerJob::new(
                WorkerCompletionId::from_abi(1),
                crate::async_host::HandleKey::null(),
                job.into(),
            ),
            move |job| {
                started.send(()).unwrap();
                resume.recv().unwrap();
                run_host_job(&mut job.job);
            },
            completed,
            WorkerCompletionDestination::Test(Box::new(|_| {})),
        );
        ready.recv_timeout(Duration::from_secs(3)).unwrap();
        if before_read {
            assert_eq!(
                cancel_worker(&worker).unwrap(),
                CancellationOutcome::NeedWait
            );
            release.send(()).unwrap();
        } else {
            release.send(()).unwrap();
            // Let the empty-console wait start; shutdown must wake and join it.
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(shutdown_workers([worker]).is_empty());
        let result = results.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(result.job.err(), ERROR_OPERATION_ABORTED as i32);
    }
}
