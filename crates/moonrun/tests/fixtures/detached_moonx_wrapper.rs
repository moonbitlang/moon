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

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;

fn main() {
    // Do not interpret the inherited-policy marker here. This intermediary
    // deliberately treats it as an opaque piece of the process environment,
    // so the black-box test remains valid for either an FD or HANDLE backend.
    let server = std::env::var("MOON_TEST_DETACH_SERVER")
        .expect("missing detached moonx synchronization server");
    let real_moonx =
        std::env::var_os("MOON_TEST_REAL_MOONX").expect("missing real moonx executable");
    let mut server = TcpStream::connect(server).expect("connect to detached moonx test");
    server.write_all(b"R").expect("announce detached moonx");

    let mut release = [0];
    server
        .read_exact(&mut release)
        .expect("wait for detached moonx release");
    assert_eq!(release, [b'G'], "unexpected detached moonx release");

    let status = Command::new(real_moonx)
        .args(std::env::args_os().skip(1))
        .status()
        .expect("run real moonx");
    server
        .write_all(if status.success() { b"S" } else { b"F" })
        .expect("report detached moonx result");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
