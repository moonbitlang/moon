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

fn main {
  check_lossless_utf16_round_trip()
  let args = get_args()
  let reader_args = get_args_from_reader()
  if args.length() != reader_args.length() {
    abort("String-array reader returned the wrong number of arguments")
  }
  for i in 0..<args.length() {
    if args[i] != reader_args[i] {
      abort("String-array reader returned different arguments")
    }
  }
  if args.length() > 1 && args[1] == "exit-7" {
    exit_with_code(7)
  }
  if args.length() > 1 && args[1] == "spin" {
    while true {}
  }
  @debug.to_string(args) |> println
}

fn env_get_var(s : ExternString) -> ExternString = "__moonbit_fs_unstable" "env_get_var"

fn args_get() -> JSArray = "__moonbit_fs_unstable" "args_get"

fn get_env_var(name : String) -> String? {
  let res = env_get_var(string_to_extern(name))
  let mbt_string = string_from_extern(res)
  if mbt_string == "" {
    None
  } else {
    Some(mbt_string)
  }
}

fn get_args() -> Array[String] {
  let arr = args_get()
  let len = array_len(arr)
  let res = []
  for i = 0; i < len; i = i + 1 {
    let val = arr[i]
    if not(jsvalue_is_string(val)) {
      abort("Expected all strings in array")
    }
    res.push(string_from_extern(jsvalue_get_string(val)))
  }
  res
}

#external
type JSValue

fn jsvalue_is_string(v : JSValue) -> Bool = "__moonbit_fs_unstable" "jsvalue_is_string"

fn jsvalue_get_string(v : JSValue) -> ExternString = "%identity"


#external
type JSArray

fn array_len(arr : JSArray) -> Int = "__moonbit_fs_unstable" "array_len"

fn array_get(arr : JSArray, idx : Int) -> JSValue = "__moonbit_fs_unstable" "array_get"

fn JSArray::op_get(self : JSArray, idx : Int) -> JSValue {
  return array_get(self, idx)
}

#external
type StringArrayReadHandle

fn begin_read_string_array(arr : JSArray) -> StringArrayReadHandle = "__moonbit_fs_unstable" "begin_read_string_array"

fn string_array_read_string(handle : StringArrayReadHandle) -> ExternString = "__moonbit_fs_unstable" "string_array_read_string"

fn finish_read_string_array(handle : StringArrayReadHandle) = "__moonbit_fs_unstable" "finish_read_string_array"

fn get_args_from_reader() -> Array[String] {
  let handle = begin_read_string_array(args_get())
  let result = []
  while true {
    let value = string_from_extern(string_array_read_string(handle))
    if value == "ffi_end_of_/string_array" {
      break
    }
    result.push(value)
  }
  finish_read_string_array(handle)
  result
}


#external
type StringCreateHandle

#external
type StringReadHandle

#external
type ExternString

fn begin_create_string() -> StringCreateHandle = "__moonbit_fs_unstable" "begin_create_string"

fn string_append_char(handle : StringCreateHandle, ch : Char) = "__moonbit_fs_unstable" "string_append_char"

fn string_append_code_unit(handle : StringCreateHandle, code_unit : Int) = "__moonbit_fs_unstable" "string_append_char"

fn finish_create_string(handle : StringCreateHandle) -> ExternString = "__moonbit_fs_unstable" "finish_create_string"

fn string_to_extern(s : String) -> ExternString {
  let handle = begin_create_string()
  s.iter().each(fn(ch) { string_append_char(handle, ch) })
  finish_create_string(handle)
}

fn begin_read_string(s : ExternString) -> StringReadHandle = "__moonbit_fs_unstable" "begin_read_string"

/// Read one char from the string, returns -1 if the end of the string is reached.
/// The number returned is the unicode codepoint of the character.
fn string_read_char(handle : StringReadHandle) -> Int = "__moonbit_fs_unstable" "string_read_char"

fn finish_read_string(handle : StringReadHandle) = "__moonbit_fs_unstable" "finish_read_string"

fn check_lossless_utf16_round_trip() -> Unit {
  let builder = begin_create_string()
  let expected = [0x61, 0xd800, 0x62, 0xdc00, 0xd83d, 0xde00]
  for code_unit in expected {
    string_append_code_unit(builder, code_unit)
  }
  let reader = begin_read_string(finish_create_string(builder))
  for code_unit in expected {
    if string_read_char(reader) != code_unit {
      abort("Extern string did not preserve its UTF-16 code units")
    }
  }
  if string_read_char(reader) != -1 {
    abort("Extern string reader did not stop at the end")
  }
  finish_read_string(reader)
}

fn string_from_extern(e : ExternString) -> String {
  let buf = @buffer.new()
  let handle = begin_read_string(e)
  while true {
    let ch = string_read_char(handle)
    if ch == -1 {
      break
    } else {
      buf.write_char(Char::from_int(ch))
    }
  }
  finish_read_string(handle)
  buf.to_string()
}
