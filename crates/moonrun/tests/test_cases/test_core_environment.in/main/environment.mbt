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
fn current_dir_raw() -> UInt64 = "moonbitlang/core" "env/current_dir"

///|
// All lengths are byte counts, including UTF-16LE inputs on Windows.
#unsafe_skip_stub_check
#borrow(key)
fn get_env_var_raw(key : Bytes, key_len : Int32) -> UInt64 = "moonbitlang/core" "env/get_env_var"

///|
#unsafe_skip_stub_check
#borrow(key, value)
fn set_env_var_raw(
  key : Bytes,
  key_len : Int32,
  value : Bytes,
  value_len : Int32,
) -> Int32 = "moonbitlang/core" "env/set_env_var"

///|
#unsafe_skip_stub_check
#borrow(key)
fn unset_env_var_raw(key : Bytes, key_len : Int32) -> Int32 = "moonbitlang/core" "env/unset_env_var"

///|
#unsafe_skip_stub_check
fn buffer_is_null(handle : UInt64) -> Bool = "moonbitlang/async" "c_buffer/is_null"

///|
#unsafe_skip_stub_check
fn buffer_length(handle : UInt64) -> UInt32 = "moonbitlang/async" "c_buffer/length"

///|
#unsafe_skip_stub_check
#borrow(buffer)
fn buffer_copy(
  handle : UInt64,
  source_offset : Int32,
  buffer : Bytes,
  destination_offset : Int32,
  length : Int32,
) -> Unit = "moonbitlang/async" "c_buffer/blit_from_c"

///|
#unsafe_skip_stub_check
fn buffer_free(handle : UInt64) -> Unit = "moonbitlang/async" "c_buffer/free"

///|
fn get_errno() -> Int32 = "moonbitlang/async" "os_error/get_errno"

///|
fn get_platform() -> Int32 = "moonbitlang/async" "runtime/get_platform"

///|
let is_windows : Bool = get_platform() == 2

///|
// Native errors: ENOENT on Linux/macOS, ERROR_ENVVAR_NOT_FOUND on Windows.
let missing_variable : Int32 = if is_windows { 203 } else { 2 }

///|
suberror HostError {
  HostError(Int32)
}

///|
// Consume one non-null snapshot. Length and copy observe the same owned bytes;
// defer releases the host allocation on normal return and error propagation.
fn read_snapshot_bytes(handle : UInt64) -> Bytes raise {
  defer buffer_free(handle)
  let length = buffer_length(handle)
  guard length <= 0x7fff_ffffU else {
    fail("snapshot is too large for a MoonBit buffer")
  }
  let buffer = Bytes::make(length.reinterpret_as_int(), 0)
  buffer_copy(handle, 0, buffer, 0, buffer.length())
  buffer
}

///|
// This text wrapper chooses strict decoding. Raw callers can instead keep the
// bytes or choose lossy decoding; neither policy is imposed by the host ABI.
fn read_snapshot(handle : UInt64) -> String raise {
  let buffer = read_snapshot_bytes(handle)
  // OS strings have no BOM marker: a leading U+FEFF is part of the value.
  if is_windows {
    @utf16.decode(buffer, ignore_bom=false, endianness=Little)
  } else {
    @utf8.decode(buffer, ignore_bom=false)
  }
}

///|
fn encode_native(text : String) -> Bytes {
  if is_windows {
    @utf16.encode(text, bom=false, endianness=Little)
  } else {
    @utf8.encode(text)
  }
}

///|
fn current_dir() -> String raise {
  let handle = current_dir_raw()
  if buffer_is_null(handle) {
    raise HostError(get_errno())
  }
  read_snapshot(handle)
}

///|
fn get_env_var(name : String) -> String? raise {
  let key = encode_native(name)
  let handle = get_env_var_raw(key, key.length())
  if buffer_is_null(handle) {
    let error = get_errno()
    if error == missing_variable {
      return None
    }
    raise HostError(error)
  }
  // An empty value owns a non-null, zero-length buffer.
  Some(read_snapshot(handle))
}

///|
fn set_env_var(name : String, text : String) -> Unit raise {
  let key = encode_native(name)
  let value = encode_native(text)
  let status = set_env_var_raw(key, key.length(), value, value.length())
  if status != 0 {
    raise HostError(status)
  }
}

///|
fn unset_env_var(name : String) -> Unit raise {
  let key = encode_native(name)
  let status = unset_env_var_raw(key, key.length())
  if status != 0 {
    raise HostError(status)
  }
}
