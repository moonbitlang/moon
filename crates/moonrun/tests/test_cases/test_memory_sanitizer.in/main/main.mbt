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
fn assert_true(cond : Bool, message : String) -> Unit {
  if !cond {
    abort(message)
  }
}

///|
fn main {
  let ptr = 1024U
  @memory_sanitizer.register_object_alloc(16U, ptr)
  assert_true(@memory_sanitizer.object_is_valid(ptr), "object should be valid")
  @memory_sanitizer.register_object_free(ptr)
  assert_true(!@memory_sanitizer.object_is_valid(ptr), "object should be freed")
  println("ok")
}
