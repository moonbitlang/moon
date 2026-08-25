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
async test "ambient cwd is observed by each process" {
  let input = @fs.read_file("input.txt").text()
  let output = if input == "first" { b"first" } else { b"second" }
  @fs.write_file("output.txt", output, create_mode=CreateOrTruncate)
  match @env.current_dir() {
    Some(cwd) => @fs.write_file(
      "cwd.txt",
      cwd,
      create_mode=CreateOrTruncate,
    )
    None => abort("current directory is unavailable")
  }
}
