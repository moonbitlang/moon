// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
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
            signal(SIGINT, handler as usize);
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
