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
  let marker = match @env.get_env_var("MOON_TEST_DETACH_MARKER") {
    Some(marker) => marker
    None => abort("missing detach marker path")
  }
  let pid_marker = match @env.get_env_var("MOON_TEST_DETACH_PID") {
    Some(marker) => marker
    None => abort("missing detached pid path")
  }
  for index = 0; index < 2; index = index + 1 {
    let marker = marker + "-\{index}"
    let output = @process.redirect_to_file(marker + ".log")
    let _ = @process.spawn_orphan(
      "moonx",
      ["moonbitlang/parser/cmd/moonfmt@0.3.3", marker],
      extra_env={ "MOON_TEST_DETACH_PID": pid_marker + "-\{index}" },
      stdout=output,
      stderr=output,
    )
  }
}
