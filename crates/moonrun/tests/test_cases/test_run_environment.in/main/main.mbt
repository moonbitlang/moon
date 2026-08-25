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
fn runtime_exit(code : Int) = "moonbitlang/async" "runtime/exit"

///|
fn main {
  let expected = match @env.get_env_var("MOONRUN_RUN_ENV_ISOLATION_INPUT") {
    Some(value) => value
    None => {
      runtime_exit(70)
      return
    }
  }
  @env.set_env_var("MOONRUN_RUN_ENV_ISOLATION_MUTABLE", expected)
  for _ in 0..<100_000 {
    if @env.get_env_var("MOONRUN_RUN_ENV_ISOLATION_MUTABLE") != Some(expected) {
      runtime_exit(71)
      return
    }
  }
  @env.unset_env_var("MOONRUN_RUN_ENV_ISOLATION_MUTABLE")
  if @env.get_env_var("MOONRUN_RUN_ENV_ISOLATION_MUTABLE") is Some(_) {
    runtime_exit(72)
  }
}
