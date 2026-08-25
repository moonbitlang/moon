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
