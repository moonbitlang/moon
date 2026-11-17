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
fn check_unicode_and_legacy_interop() -> Unit raise {
  let key = "MOONRUN_批量_🔑"
  assert_eq(get_env_var(key), None)
  set_env_var(key, "\u{feff}λ🦀")
  assert_eq(get_env_var(key), Some("\u{feff}λ🦀"))
  assert_eq(@env.get_env_var(key), Some("\u{feff}λ🦀"))
  @env.set_env_var(key, "legacy")
  assert_eq(get_env_var(key), Some("legacy"))
  set_env_var(key, "")
  assert_eq(get_env_var(key), Some(""))
  unset_env_var(key)
  assert_eq(get_env_var(key), None)
  assert_eq(@env.get_env_var(key), None)
}

///|
fn check_snapshot_survives_mutation() -> Unit raise {
  let key = encode_native("MOONRUN_BULK_SNAPSHOT")
  set_env_var("MOONRUN_BULK_SNAPSHOT", "λ🦀")
  let snapshot = get_env_var_raw(key, key.length())
  assert_false(buffer_is_null(snapshot))
  assert_eq(
    buffer_length(snapshot),
    encode_native("λ🦀").length().reinterpret_as_uint(),
  )
  set_env_var("MOONRUN_BULK_SNAPSHOT", "replacement")
  assert_eq(get_env_var("MOONRUN_BULK_SNAPSHOT"), Some("replacement"))
  unset_env_var("MOONRUN_BULK_SNAPSHOT")
  assert_eq(get_env_var("MOONRUN_BULK_SNAPSHOT"), None)
  // Consuming the original snapshot still returns the original value.
  assert_eq(read_snapshot(snapshot), "λ🦀")
}

///|
fn check_raw_bytes_and_guest_decoding() -> Unit raise {
  let key = encode_native("MOONRUN_BULK_RAW")
  let value = if is_windows { b"\x00\xd8" } else { b"\xff" }
  assert_eq(set_env_var_raw(key, key.length(), value, value.length()), 0)
  let snapshot = get_env_var_raw(key, key.length())
  assert_false(buffer_is_null(snapshot))
  assert_eq(read_snapshot_bytes(snapshot), value)
  // Host lookup succeeds; the guest's strict String wrapper rejects this value.
  let rejected = try {
    ignore(get_env_var("MOONRUN_BULK_RAW"))
    false
  } catch {
    @utf8.Malformed(_) => true
    @utf16.Malformed(_) => true
    _ => false
  }
  assert_true(rejected)
  unset_env_var("MOONRUN_BULK_RAW")
}

///|
fn main raise {
  check_unicode_and_legacy_interop()
  check_snapshot_survives_mutation()
  check_raw_bytes_and_guest_decoding()
  let cwd = current_dir()
  assert_eq(Some(cwd), @env.current_dir())
  assert_eq(Some(cwd), get_env_var("MOONRUN_EXPECTED_CWD"))
  println("bulk environment ok")
}
