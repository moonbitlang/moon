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
async fn main {
  let path = match @env.get_env_var("MOONRUN_TEST_PATH") {
    Some(path) => path
    None => abort("MOONRUN_TEST_PATH is missing")
  }
  @env.set_env_var("PATH", path)
  try {
    let (code, _) =
      @process.collect_stdout("moonrun-path-command", ["--version"])
    if code != 0 {
      abort("PATH command exited with code \{code}")
    }
    println("spawned through updated PATH")
  } catch {
    err => abort("failed to spawn through updated PATH: \{err}")
  }
}
