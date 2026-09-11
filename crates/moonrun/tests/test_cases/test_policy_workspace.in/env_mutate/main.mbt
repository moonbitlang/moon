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
