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

///|
#external
type XByteArrayCreateHandle

///|
#external
type XByteArrayReadHandle

///|
#external
type XExternByteArray

fn read_bytes_from_stdin_ffi() -> XExternByteArray  = "__moonbit_io_unstable" "read_bytes_from_stdin"
fn read_bytes_from_stdin() -> Bytes {
  let content = read_bytes_from_stdin_ffi()
  byte_array_from_extern(content)
}

pub fn byte_array_from_extern(e : XExternByteArray) -> Bytes {
  let buf = Array::new()
  let handle = begin_read_byte_array(e)
  while true {
    let ch = byte_array_read_byte(handle)
    if ch == -1 {
      break
    } else {
      buf.push(ch.to_byte())
    }
  }
  finish_read_byte_array(handle)
  Bytes::from_array(buf)
}

///|
fn begin_read_byte_array(s : XExternByteArray) -> XByteArrayReadHandle = "__moonbit_fs_unstable" "begin_read_byte_array"

///|
fn byte_array_read_byte(handle : XByteArrayReadHandle) -> Int = "__moonbit_fs_unstable" "byte_array_read_byte"

///|
fn finish_read_byte_array(handle : XByteArrayReadHandle) = "__moonbit_fs_unstable" "finish_read_byte_array"


///|
fn begin_create_byte_array() -> XByteArrayCreateHandle = "__moonbit_fs_unstable" "begin_create_byte_array"

///|
fn byte_array_append_byte(handle : XByteArrayCreateHandle, byte : Int) = "__moonbit_fs_unstable" "byte_array_append_byte"

///|
fn finish_create_byte_array(handle : XByteArrayCreateHandle) -> XExternByteArray = "__moonbit_fs_unstable" "finish_create_byte_array"

///|
fn check_byte_array_round_trip() -> Unit {
  let builder = begin_create_byte_array()
  let input = [0, 1, 255, 256, -1]
  for byte in input {
    byte_array_append_byte(builder, byte)
  }
  let reader = begin_read_byte_array(finish_create_byte_array(builder))
  let expected = [0, 1, 255, 0, 255]
  for byte in expected {
    if byte_array_read_byte(reader) != byte {
      abort("Extern byte array did not preserve Uint8Array coercion")
    }
  }
  if byte_array_read_byte(reader) != -1 {
    abort("Extern byte array reader did not stop at the end")
  }
  finish_read_byte_array(reader)
}

///|
fn main {
  check_byte_array_round_trip()
  let bytes = read_bytes_from_stdin()
  println(bytes.iter().count())
}
