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

//! Bounded, versioned framing shared by local platform transports. No execution policy.

use super::Connection;
#[cfg(windows)]
pub(super) use super::transport::receive_stdio;
pub(super) use super::transport::send_stdio;
use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[cfg(unix)]
pub(super) fn receive_stdio(stream: &Connection) -> io::Result<[Option<std::fs::File>; 3]> {
    super::transport::receive_stdio(stream).map(|files| files.map(Some))
}

pub(super) const VERSION: u32 = 2;
const MAX_FRAME: usize = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub version: u32,
    pub invocation: super::Invocation,
}

#[derive(Serialize, Deserialize)]
pub(super) enum Control {
    Signal(i32),
    Exit(i32),
    KilledBySignal(i32),
}

#[derive(Serialize, Deserialize)]
pub(super) enum Reply {
    Completed,
    Exited(i32),
    KilledBySignal(i32),
    Failed(String),
}

pub(super) fn encode(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "IPC frame exceeds size limit",
        ));
    }
    Ok(bytes)
}

pub(super) fn write(stream: &Connection, value: &impl Serialize) -> io::Result<()> {
    write_frame(stream, &encode(value)?)
}

pub(super) fn write_frame(mut stream: &Connection, bytes: &[u8]) -> io::Result<()> {
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)
}

pub(super) fn write_reply(stream: &Connection, reply: &Reply) -> io::Result<()> {
    let bytes = encode(reply)?;
    let mut frame = Vec::with_capacity(4 + bytes.len());
    frame.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    frame.extend_from_slice(&bytes);
    super::transport::write_all_deadline(stream, &frame)
}

pub(super) fn read<T: DeserializeOwned>(mut stream: &Connection) -> io::Result<T> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame exceeds size limit",
        ));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

// serde's Path serialization requires UTF-8. OsString's Unix representation
// preserves bytes, including paths that cannot be represented as Rust strings.
pub(super) mod path {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        path.as_os_str().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        OsString::deserialize(deserializer).map(PathBuf::from)
    }
}
