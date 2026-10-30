// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
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
