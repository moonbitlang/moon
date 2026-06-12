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

fn get_platform() -> Int = "moonbit_v0" "runtime/get_platform"

///|
fn ms_since_epoch() -> Int64 = "moonbit_v0" "time/get_ms_since_epoch"

///|
fn sleep_ms(duration : Int) -> Int = "moonbit_v0" "time/sleep_ms"

///|
fn get_errno() -> Int = "moonbit_v0" "os_error/get_errno"

///|
fn get_tmp_path_len() -> Int = "moonbit_v0" "fs/get_tmp_path_len"

///|
fn get_tmp_path(dst : Int, len : Int) -> Int = "moonbit_v0" "fs/get_tmp_path"

///|
#borrow(bytes)
extern "wasm" fn bytes_to_ptr(bytes : Bytes) -> Int =
  #|(func (param i32) (result i32) local.get 0)

///|
#borrow(str)
extern "wasm" fn string_to_ptr(str : String) -> Int =
  #|(func (param i32) (result i32) local.get 0)

///|
fn blit_to_c(dst : Int, src : Int, offset : Int, len : Int) -> Unit = "moonbit_v0" "c_buffer/blit_to_c"

///|
fn blit_from_c(src : Int, dst : Int, offset : Int, len : Int) -> Unit = "moonbit_v0" "c_buffer/blit_from_c"

///|
fn c_buffer_get(buf : Int, index : Int) -> Byte = "moonbit_v0" "c_buffer/c_buffer_get"

///|
fn strlen(buf : Int) -> Int = "moonbit_v0" "c_buffer/strlen"

///|
fn pointer_is_null(ptr : Int) -> Bool = "moonbit_v0" "c_buffer/pointer_is_null"

///|
fn null_pointer() -> Int = "moonbit_v0" "c_buffer/null_pointer"

///|
fn errno_is_lock_violation(errno : Int) -> Bool = "moonbit_v0" "fs/errno_is_lock_violation"

///|
fn make_tcp_socket(family : Int) -> Int = "moonbit_v0" "socket/make_tcp_socket"

///|
fn assert_true(cond : Bool, message : String) -> Unit {
  if !cond {
    abort(message)
  }
}

///|
fn main {
  let platform = get_platform()
  assert_true(platform >= 0 && platform <= 2, "invalid platform")
  let before = ms_since_epoch()
  assert_true(sleep_ms(1) == 0, "sleep failed")
  let after = ms_since_epoch()
  assert_true(after >= before, "time moved backwards")
  let tmp_path_len = get_tmp_path_len()
  assert_true(tmp_path_len > 0, "tmp path size query failed")
  assert_true(get_tmp_path(0, 0) == -1, "tmp path fill accepted a missing buffer")
  let tmp_path = String::make(tmp_path_len, '\u{0}')
  let tmp_path_ptr = string_to_ptr(tmp_path)
  assert_true(get_tmp_path(tmp_path_ptr, tmp_path_len) == 0, "tmp path fill failed")
  assert_true(tmp_path != String::make(tmp_path_len, '\u{0}'), "tmp path fill wrote an empty path")
  let src = b"abcdef\x00"
  let dst = Bytes::make(6, b'_')
  blit_to_c(bytes_to_ptr(dst), bytes_to_ptr(src), 2, 3)
  assert_true(dst[0] == b'c' && dst[1] == b'd' && dst[2] == b'e', "blit_to_c failed")
  let dst2 = Bytes::make(6, b'_')
  blit_from_c(bytes_to_ptr(src), bytes_to_ptr(dst2), 2, 3)
  assert_true(dst2[2] == b'a' && dst2[3] == b'b' && dst2[4] == b'c', "blit_from_c failed")
  assert_true(c_buffer_get(bytes_to_ptr(src), 1) == b'b', "c_buffer_get failed")
  assert_true(strlen(bytes_to_ptr(src)) == 6, "strlen failed")
  assert_true(null_pointer() == 0, "null pointer value changed")
  assert_true(pointer_is_null(0), "null pointer check failed")
  assert_true(!errno_is_lock_violation(0), "zero errno is a lock violation")
  assert_true(make_tcp_socket(0) == -1, "unsupported socket stub succeeded")
  assert_true(get_errno() != 0, "unsupported socket stub did not set errno")
  println("ok")
}
