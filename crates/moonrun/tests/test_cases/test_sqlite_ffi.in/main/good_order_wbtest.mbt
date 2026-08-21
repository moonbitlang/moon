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
test "clear bindings after reset" {
  let database = open_binding_order_database()
  let statement = prepare(database, "SELECT ?1, ?2", sqlite3_null_handle())
  let text = "text binding"
  let blob = b"blob binding"
  assert_true(
    sqlite3_bind_text16(statement, 1, text, 0, text.length()) == SQLITE_OK,
    msg="text bind failed",
  )
  assert_true(
    sqlite3_bind_blob(statement, 2, blob, 0, blob.length()) == SQLITE_OK,
    msg="blob bind failed",
  )
  assert_true(
    sqlite3_step(statement) == SQLITE_ROW,
    msg="query returned no row",
  )

  assert_true(sqlite3_reset(statement) == SQLITE_OK, msg="reset failed")
  assert_true(
    sqlite3_clear_bindings(statement) == SQLITE_OK,
    msg="clear bindings after reset failed",
  )
  assert_true(
    sqlite3_step(statement) == SQLITE_ROW,
    msg="query returned no row",
  )
  assert_true(
    sqlite3_column_type(statement, 0) == SQLITE_NULL &&
    sqlite3_column_type(statement, 1) == SQLITE_NULL,
    msg="bindings were not cleared",
  )
  finish_binding_order_test(database, statement)
}
