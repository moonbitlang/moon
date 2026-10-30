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
