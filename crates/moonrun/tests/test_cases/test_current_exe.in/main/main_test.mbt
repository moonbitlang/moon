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
#unsafe_skip_stub_check
fn current_exe() -> UInt64 = "moonbitlang/core" "env/current_exe"

///|
#unsafe_skip_stub_check
fn c_buffer_length(handle : UInt64) -> Int = "moonbitlang/async" "c_buffer/length"

///|
#unsafe_skip_stub_check
#borrow(path)
fn c_buffer_copy_to_string(
  handle : UInt64,
  source_offset : Int,
  path : String,
  destination_offset : Int,
  byte_length : Int,
) -> Unit = "moonbitlang/async" "c_buffer/blit_from_c"

///|
#unsafe_skip_stub_check
fn c_buffer_free(handle : UInt64) -> Unit = "moonbitlang/async" "c_buffer/free"

///|
#unsafe_skip_stub_check
fn get_errno() -> Int = "moonbitlang/async" "os_error/get_errno"

///|
test "current executable is the absolute loaded wasm path" {
  let handle = current_exe()
  if handle == 0 {
    fail("failed to obtain the current executable path: errno=\{get_errno()}")
  }
  defer c_buffer_free(handle)
  let byte_length = c_buffer_length(handle)
  assert_true(
    byte_length % 2 == 0,
    msg="current executable path is not a MoonBit string",
  )
  let path = String::make(byte_length / 2, '\u{0}')
  c_buffer_copy_to_string(handle, 0, path, 0, byte_length)
  let relative_path = "_build/wasm/debug/test/main/main.blackbox_test.wasm"
  let windows_relative_path =
    "_build\\wasm\\debug\\test\\main\\main.blackbox_test.wasm"
  assert_true(
    path.length() > relative_path.length(),
    msg="current executable path is not absolute",
  )
  let suffix = path.unsafe_substring(
    start=path.length() - relative_path.length(),
    end=path.length(),
  )
  assert_true(
    suffix == relative_path || suffix == windows_relative_path,
    msg="current executable path does not identify the loaded wasm file",
  )
}
