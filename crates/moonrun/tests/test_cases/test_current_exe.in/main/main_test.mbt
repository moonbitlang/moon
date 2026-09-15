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
fn current_exe_dir() -> UInt64 = "moonbitlang/core" "env/current_exe_dir"

///|
#unsafe_skip_stub_check
fn os_string_decode_len(handle : UInt64, offset : Int, len : Int) -> Int = "moonbitlang/async" "os_string/decode_len"

///|
#unsafe_skip_stub_check
#borrow(path)
fn os_string_decode(
  handle : UInt64,
  offset : Int,
  len : Int,
  path : String,
  path_len : Int,
) -> Unit = "moonbitlang/async" "os_string/decode"

///|
#unsafe_skip_stub_check
fn c_buffer_free(handle : UInt64) -> Unit = "moonbitlang/async" "c_buffer/free"

///|
#unsafe_skip_stub_check
fn get_errno() -> Int = "moonbitlang/async" "os_error/get_errno"

///|
fn read_path(handle : UInt64) -> String raise {
  if handle == 0 {
    fail("failed to obtain the current executable path: errno=\{get_errno()}")
  }
  defer c_buffer_free(handle)
  let len = os_string_decode_len(handle, 0, -1)
  let path = String::make(len, '\u{0}')
  os_string_decode(handle, 0, -1, path, len)
  path
}

///|
test "current executable and directory are absolute loaded wasm paths" {
  assert_eq(
    Some(read_path(current_exe())),
    @env.get_env_var("MOONRUN_TEST_CURRENT_EXE"),
  )
  assert_eq(
    Some(read_path(current_exe_dir())),
    @env.get_env_var("MOONRUN_TEST_CURRENT_EXE_DIR"),
  )
}
