// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// any later version.
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
const SQLITE_MISUSE = 21

///|
fn open_binding_order_database() -> UInt64 raise {
  let null_handle = sqlite3_null_handle()
  let database = Ref(null_handle)
  let filename = b":memory:"
  assert_true(
    sqlite3_open_v2(
      filename,
      filename.length(),
      database,
      SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
      null_handle,
    ) ==
    SQLITE_OK,
    msg="memory database open failed",
  )
  assert_true(database.val != null_handle, msg="database handle is null")
  database.val
}

///|
fn finish_binding_order_test(
  database : UInt64,
  statement : UInt64,
) -> Unit raise {
  assert_true(
    sqlite3_finalize(statement) == SQLITE_OK,
    msg="statement finalize failed",
  )
  assert_true(sqlite3_close(database) == SQLITE_OK, msg="database close failed")
}
