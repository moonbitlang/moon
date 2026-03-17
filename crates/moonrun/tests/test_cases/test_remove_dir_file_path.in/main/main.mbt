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
#external
type StringCreateHandle

///|
#external
type ExternString

fn begin_create_string() -> StringCreateHandle = "__moonbit_fs_unstable" "begin_create_string"

fn string_append_char(handle : StringCreateHandle, ch : Char) = "__moonbit_fs_unstable" "string_append_char"

fn finish_create_string(handle : StringCreateHandle) -> ExternString = "__moonbit_fs_unstable" "finish_create_string"

fn string_to_extern(s : String) -> ExternString {
  let handle = begin_create_string()
  s.iter().each(fn(ch) { string_append_char(handle, ch) })
  finish_create_string(handle)
}

fn remove_dir(path : ExternString) = "__moonbit_fs_unstable" "remove_dir"

fn main {
  let path = string_to_extern("file.txt")
  remove_dir(path)
  println("unexpected")
}
