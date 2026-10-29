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

fn main() {
    let mut args = std::env::args().skip(1);
    assert_eq!(args.next().as_deref(), Some("--moonrun-artifact-handoff"));
    let handoff = std::path::PathBuf::from(args.next().unwrap());
    assert!(handoff.is_dir());
    let mut connection = TcpStream::connect(args.next().unwrap()).unwrap();
    connection.write_all(b"R").unwrap();
    let mut release = [0];
    connection.read_exact(&mut release).unwrap();
    assert_eq!(release, [b'G']);
    if let Some(artifact) = args.next() {
        for file in std::fs::read_dir(artifact).unwrap() {
            let file = file.unwrap();
            std::fs::copy(file.path(), handoff.join(file.file_name())).unwrap();
        }
    } else {
        std::process::exit(23);
    }
}
