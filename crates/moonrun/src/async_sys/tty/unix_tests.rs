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
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::Command;

const TTY_PATH_ENV: &str = "MOONRUN_TEST_TTY_PATH";
const ROWS: u16 = 24;
const COLS: u16 = 80;

/// Run the child test in a new session whose controlling terminal is a fresh
/// pty, with a line of input already queued.
#[test]
fn unix_tty_lifecycle() {
    let mut master = -1;
    let mut slave = -1;
    let mut winsize = libc::winsize {
        ws_row: ROWS,
        ws_col: COLS,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut winsize,
            )
        },
        0
    );
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    let path = unsafe { std::ffi::CStr::from_ptr(libc::ttyname(slave.as_raw_fd())) }
        .to_str()
        .unwrap()
        .to_owned();
    let input = b"hi\n";
    assert_eq!(
        unsafe { libc::write(master.as_raw_fd(), input.as_ptr().cast(), input.len()) },
        input.len() as isize
    );

    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "async_sys::tty::unix_tests::unix_tty_lifecycle_child",
            "--ignored",
            "--nocapture",
        ])
        .env(TTY_PATH_ENV, &path);
    let slave_fd = slave.as_raw_fd();
    unsafe {
        child.pre_exec(move || {
            if libc::setsid() < 0 || libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = child
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(slave);
    // Drain the echo: a session leader's exit waits for terminal output to
    // drain. Reading fails once the child's side closes.
    let drain = std::thread::spawn(move || {
        let mut buf = [0_u8; 256];
        while unsafe { libc::read(master.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
    });
    let output = child.wait_with_output().unwrap();
    drain.join().unwrap();
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "TTY child failed with {:?}:\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
#[ignore = "runs inside unix_tty_lifecycle's pty session"]
fn unix_tty_lifecycle_child() {
    #[cfg(target_os = "macos")]
    assert_eq!(
        unix::tty_path().unwrap().to_str().unwrap(),
        std::env::var(TTY_PATH_ENV).unwrap()
    );
    let tty = open().expect("open the controlling terminal");
    assert_eq!(
        size(&tty.terminal).unwrap(),
        (i64::from(ROWS) << 32) | i64::from(COLS)
    );
    let state = get_state(&tty.terminal).unwrap();
    assert_eq!(state.len(), state_size());
    set_state(&tty.terminal, &state).unwrap();
    assert_eq!(get_state(&tty.terminal).unwrap(), state);
    assert_eq!(
        set_state(&tty.terminal, &state[1..]),
        Err(crate::async_host::AsyncHostError::Inval)
    );
    drop(tty);
    guest_reads_through_the_event_bus();
}

/// The guest registers the single terminal Handle on the event bus and reads
/// it with `io/read/unix`, as `src/internal/event_loop/tty_unix.mbt` does.
fn guest_reads_through_the_event_bus() {
    let source = r#"(module
        (import "moonbitlang/async" "tty/open" (func $open (param i32 i32) (result i32)))
        (import "moonbitlang/async" "event_bus/create" (func $create (result i64)))
        (import "moonbitlang/async" "event_bus/try_register" (func $register (param i64 i64 i32) (result i32)))
        (import "moonbitlang/async" "event_bus/wait" (func $wait (param i64 i32) (result i32)))
        (import "moonbitlang/async" "event_bus/destroy" (func $destroy (param i64)))
        (import "moonbitlang/async" "io/read/unix" (func $read (param i64 i32 i32 i32) (result i32)))
        (import "moonbitlang/async" "fd_util/close_fd" (func $close (param i64) (result i32)))
        (memory (export "memory") 1)
        (global $bus (mut i64) (i64.const 0))
        (func $check (param $ok i32) (if (i32.eqz (local.get $ok)) (then unreachable)))
        (func (export "_start")
            ;; Too short a destination fails without opening anything.
            (call $check (i32.eq (call $open (i32.const 0) (i32.const 0)) (i32.const -1)))
            (call $check (i32.eq (call $open (i32.const 0) (i32.const 1)) (i32.const 1)))
            (global.set $bus (call $create))
            (call $check (i32.eq (call $register (global.get $bus) (i64.load (i32.const 0)) (i32.const 1)) (i32.const 1)))
            (call $check (i32.eq (call $wait (global.get $bus) (i32.const 5000)) (i32.const 1)))
            (call $check (i32.eq (call $read (i64.load (i32.const 0)) (i32.const 64) (i32.const 0) (i32.const 16)) (i32.const 3)))
            (call $check (i32.eq (i32.load (i32.const 64)) (i32.const 0x000a6968)))
            ;; Registration made the terminal nonblocking.
            (call $check (i32.eq (call $read (i64.load (i32.const 0)) (i32.const 64) (i32.const 0) (i32.const 16)) (i32.const -1)))
            (call $destroy (global.get $bus))
            (call $check (i32.eqz (call $close (i64.load (i32.const 0)))))))"#;
    let engine = crate::Engine::default();
    let module = engine
        .compile("tty-read.wasm", wat::parse_str(source).unwrap())
        .unwrap();
    assert_eq!(
        engine.run(&module, crate::RunOptions::default()).unwrap(),
        crate::RunOutcome::Completed
    );
}
