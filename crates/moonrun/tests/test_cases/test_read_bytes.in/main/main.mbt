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
