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
const SQLITE_OK = 0

///|
const SQLITE_OPEN_READWRITE = 0x00000002

///|
const SQLITE_OPEN_CREATE = 0x00000004

///|
/// Returns the shared null value for database, statement, and VFS Handles.
fn sqlite3_null_handle() -> UInt64 = "moonbitlang/sqlite" "sqlite3_null_handle"

///|
#unsafe_skip_stub_check
#borrow(filename, database)
fn sqlite3_open_v2(
  filename : Bytes,
  filename_length : Int,
  database : Ref[UInt64],
  flags : Int,
  vfs : UInt64,
) -> Int = "moonbitlang/sqlite" "sqlite3_open_v2"

///|
fn main {
  let null_handle = sqlite3_null_handle()
  let database = Ref(null_handle)
  let filename = b":memory:\x00"
  if sqlite3_open_v2(
      filename,
      filename.length(),
      database,
      SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
      null_handle,
    ) !=
    SQLITE_OK {
    abort("memory database open failed")
  }
  println("leaked")
}
