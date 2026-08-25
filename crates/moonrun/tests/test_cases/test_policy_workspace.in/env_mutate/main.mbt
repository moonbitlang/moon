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
fn get_tmp_path_len() -> Int = "moonbitlang/async" "fs/get_tmp_path_len"

///|
fn get_tmp_path(dst : Int, len : Int) -> Int = "moonbitlang/async" "fs/get_tmp_path"

///|
#borrow(str)
extern "wasm" fn string_to_ptr(str : String) -> Int =
  #|(func (param i32) (result i32) local.get 0)

///|
fn print_tmp_path() -> Unit {
  let len = get_tmp_path_len()
  if len < 0 {
    println("missing")
    return
  }
  let path = String::make(len, '\u{0}')
  if get_tmp_path(string_to_ptr(path), len) == 0 {
    println(path)
  } else {
    println("missing")
  }
}

///|
fn main {
  @env.set_env_var("MOONRUN_POLICY_ENV", "runtime override")
  if @env.get_env_var("MOONRUN_POLICY_ENV") is Some(value) {
    println(value)
  } else {
    println("missing")
  }

  @env.unset_env_var("MOONRUN_POLICY_ENV")
  if @env.get_env_var("MOONRUN_POLICY_ENV") is Some(value) {
    println(value)
  } else {
    println("missing")
  }

  // Prime the temp-path import before mutating the shared Run environment.
  let _ = get_tmp_path_len()
  @env.set_env_var("TMPDIR", "/first")
  @env.set_env_var("TMP", "C:/First")
  @env.set_env_var("TEMP", "C:/Fallback")
  print_tmp_path()

  @env.set_env_var("TMPDIR", "/second")
  @env.set_env_var("TMP", "C:/Second")
  print_tmp_path()

  @env.unset_env_var("TMPDIR")
  @env.unset_env_var("TMP")
  @env.unset_env_var("TEMP")
  print_tmp_path()
}
