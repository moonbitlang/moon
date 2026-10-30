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
const SQLITE_OK = 0

///|
const SQLITE_OPEN_READWRITE = 0x00000002

///|
const SQLITE_OPEN_CREATE = 0x00000004

///|
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
fn sqlite3_db_mutex(database : UInt64) -> UInt64 = "moonbitlang/sqlite" "sqlite3_db_mutex"

///|
fn sqlite3_mutex_enter(mutex : UInt64) -> Unit = "moonbitlang/sqlite" "sqlite3_mutex_enter"

///|
fn sqlite3_close(database : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_close"

///|
fn main raise {
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
  )
  let database = database.val
  let mutex = sqlite3_db_mutex(database)
  sqlite3_mutex_enter(mutex)
  ignore(sqlite3_close(database))
}
