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
#borrow(count, size)
fn wasi_environ_sizes_get(
  count : Ref[Int],
  size : Ref[Int],
) -> Int = "wasi_snapshot_preview1" "environ_sizes_get"

///|
fn assert_wasi_environment_count(expected : Int) -> Unit {
  let count = Ref(-1)
  let size = Ref(-1)
  if wasi_environ_sizes_get(count, size) != 0 {
    abort("WASI environ_sizes_get failed")
  }
  if count.val != expected {
    abort("WASI environment bypassed the Run environment")
  }
}

///|
fn main {
  if @env.get_env_var("MOONRUN_POLICY_ENV") is Some(value) {
    println(value)
    assert_wasi_environment_count(1)
  } else {
    println("missing")
    assert_wasi_environment_count(0)
  }
}
