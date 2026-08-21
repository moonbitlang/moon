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
test "reject clearing text bindings while a row is active" {
  let database = open_binding_order_database()
  let statement = prepare(database, "SELECT ?1", sqlite3_null_handle())
  let text = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  assert_true(
    sqlite3_bind_text16(statement, 1, text, 0, text.length()) == SQLITE_OK,
    msg="text bind failed",
  )
  assert_true(
    sqlite3_step(statement) == SQLITE_ROW,
    msg="query returned no row",
  )

  assert_true(
    sqlite3_clear_bindings(statement) == SQLITE_MISUSE,
    msg="bindings were cleared while a row was active",
  )
  let length = sqlite3_column_text16_length(statement, 0)
  assert_true(length == text.length(), msg="active text length changed")
  let output : FixedArray[UInt16] = FixedArray::make(length, 0)
  assert_true(
    sqlite3_column_text16(statement, 0, output, output.length()) == length,
    msg="active text copy failed",
  )
  for code_unit in output {
    assert_true(code_unit == 0x78, msg="active text was corrupted")
  }

  assert_true(sqlite3_reset(statement) == SQLITE_OK, msg="reset failed")
  finish_binding_order_test(database, statement)
}

///|
test "reject clearing blob bindings while a row is active" {
  let database = open_binding_order_database()
  let statement = prepare(database, "SELECT ?1", sqlite3_null_handle())
  let blob = b"\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5\xA5"
  assert_true(
    sqlite3_bind_blob(statement, 1, blob, 0, blob.length()) == SQLITE_OK,
    msg="blob bind failed",
  )
  assert_true(
    sqlite3_step(statement) == SQLITE_ROW,
    msg="query returned no row",
  )

  assert_true(
    sqlite3_clear_bindings(statement) == SQLITE_MISUSE,
    msg="bindings were cleared while a row was active",
  )
  let length = sqlite3_column_blob_length(statement, 0)
  assert_true(length == blob.length(), msg="active blob length changed")
  let output : FixedArray[Byte] = FixedArray::make(length, 0)
  assert_true(
    sqlite3_column_blob(statement, 0, output, output.length()) == length,
    msg="active blob copy failed",
  )
  for byte in output {
    assert_true(byte == 0xA5, msg="active blob was corrupted")
  }

  assert_true(sqlite3_reset(statement) == SQLITE_OK, msg="reset failed")
  finish_binding_order_test(database, statement)
}
