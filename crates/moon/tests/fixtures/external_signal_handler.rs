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

use std::{fs, thread, time::Duration};

#[cfg(unix)]
const SIGNAL_NAME: &str = "SIGINT";
#[cfg(windows)]
const SIGNAL_NAME: &str = "CTRL_BREAK_EVENT";

#[cfg(unix)]
mod signal {
    const SIGINT: i32 = 2;

    unsafe extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
        fn _exit(status: i32) -> !;
    }

    unsafe extern "C" fn handler(_: i32) {
        unsafe {
            _exit(42);
        }
    }

    pub fn install() {
        unsafe {
            let handler_fn: unsafe extern "C" fn(i32) = handler;
            signal(SIGINT, handler_fn as usize);
        }
    }
}

#[cfg(windows)]
mod signal {
    const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;

    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
        fn ExitProcess(exit_code: u32) -> !;
    }

    unsafe extern "system" fn handler(ctrl: u32) -> i32 {
        if ctrl == CTRL_C_EVENT || ctrl == CTRL_BREAK_EVENT {
            unsafe {
                ExitProcess(42);
            }
        }
        0
    }

    pub fn install() {
        unsafe {
            if SetConsoleCtrlHandler(Some(handler), 1) == 0 {
                std::process::exit(70);
            }
        }
    }
}

fn main() {
    signal::install();
    let ready = std::env::args_os()
        .nth(1)
        .expect("expected ready-file path argument");
    fs::write(ready, SIGNAL_NAME).unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
