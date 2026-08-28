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
