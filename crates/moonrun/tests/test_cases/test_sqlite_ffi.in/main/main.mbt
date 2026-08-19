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
const SQLITE_ERROR = 1

///|
const SQLITE_BUSY = 5

///|
const SQLITE_CANTOPEN = 14

///|
const SQLITE_AUTH = 23

///|
const SQLITE_ROW = 100

///|
const SQLITE_DONE = 101

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
/// Returns the current UTF-16 error length in code units, excluding NUL.
fn sqlite3_errmsg16_length(database : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_errmsg16_length"

///|
/// Copies the current UTF-16 error and returns its content length, excluding
/// NUL. The output is unchanged if it cannot also hold the terminator.
#unsafe_skip_stub_check
#borrow(output)
fn sqlite3_errmsg16(
  database : UInt64,
  output : FixedArray[UInt16],
  capacity : Int,
) -> Int = "moonbitlang/sqlite" "sqlite3_errmsg16"

///|
fn sqlite3_close(database : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_close"

///|
#unsafe_skip_stub_check
#borrow(sql, statement, tail)
fn sqlite3_prepare16_v2(
  database : UInt64,
  sql : String,
  sql_offset : Int,
  sql_length : Int,
  statement : Ref[UInt64],
  tail : Ref[Int],
) -> Int = "moonbitlang/sqlite" "sqlite3_prepare16_v2"

///|
fn sqlite3_step(statement : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_step"

///|
fn sqlite3_finalize(statement : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_finalize"

///|
fn sqlite3_column_int64(statement : UInt64, column : Int) -> Int64 = "moonbitlang/sqlite" "sqlite3_column_int64"

///|
extern "wasm" fn grow_memory_for_test(pages : Int) -> Int =
  #|(func (param i32) (result i32)
  #|  local.get 0
  #|  memory.grow)

///|
fn prepare(
  database : UInt64,
  sql : StringView,
  null_handle : UInt64,
) -> UInt64 raise {
  let statement = Ref(null_handle)
  let tail = Ref(sql.start_offset())
  assert_true(
    sqlite3_prepare16_v2(
      database,
      sql.data(),
      sql.start_offset(),
      sql.length(),
      statement,
      tail,
    ) ==
    SQLITE_OK,
    msg="sqlite3_prepare16_v2 failed",
  )
  assert_true(
    statement.val != null_handle,
    msg="sqlite3_prepare16_v2 returned null",
  )
  assert_true(
    tail.val == sql.start_offset() + sql.length(),
    msg="sqlite3_prepare16_v2 returned the wrong tail",
  )
  statement.val
}

///|
fn execute(
  database : UInt64,
  sql : StringView,
  null_handle : UInt64,
) -> Unit raise {
  let statement = prepare(database, sql, null_handle)
  assert_true(sqlite3_step(statement) == SQLITE_DONE, msg="sqlite3_step failed")
  assert_true(
    sqlite3_finalize(statement) == SQLITE_OK,
    msg="sqlite3_finalize failed",
  )
}

///|
fn main raise {
  let null_handle = sqlite3_null_handle()
  assert_true(
    sqlite3_close(null_handle) == SQLITE_OK,
    msg="sqlite3_close(NULL) failed",
  )
  assert_true(
    sqlite3_finalize(null_handle) == SQLITE_OK,
    msg="sqlite3_finalize(NULL) failed",
  )

  let rejected = Ref(null_handle)
  let rejected_filename = b"not-a-memory-database\x00"
  assert_true(
    sqlite3_open_v2(
      rejected_filename,
      rejected_filename.length(),
      rejected,
      SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
      null_handle,
    ) ==
    SQLITE_CANTOPEN,
    msg="file-backed database was not rejected",
  )
  assert_true(
    rejected.val == null_handle,
    msg="rejected database returned a handle",
  )

  let custom_vfs = Ref(null_handle)
  let memory_filename = b":memory:\x00"
  assert_true(
    sqlite3_open_v2(
      memory_filename,
      memory_filename.length(),
      custom_vfs,
      SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
      1UL,
    ) ==
    SQLITE_CANTOPEN,
    msg="custom VFS handle was not rejected",
  )
  assert_true(
    custom_vfs.val == null_handle,
    msg="custom VFS returned a database",
  )

  let database = Ref(null_handle)
  assert_true(
    sqlite3_open_v2(
      memory_filename,
      memory_filename.length(),
      database,
      SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
      null_handle,
    ) ==
    SQLITE_OK,
    msg="memory database open failed",
  )
  let database = database.val
  assert_true(
    database != null_handle,
    msg="memory database returned a null handle",
  )

  execute(database, "PRAGMA encoding='UTF-16le'", null_handle)

  // SQLite imports must reacquire the current memory backing after growth.
  assert_true(grow_memory_for_test(1) >= 0, msg="failed to grow wasm memory")

  execute(database, "CREATE TABLE 项目(value INTEGER)", null_handle)
  execute(database, "INSERT INTO 项目 VALUES (5000000000)", null_handle)

  let invalid_statement = Ref(null_handle)
  let invalid_tail = Ref(0)
  let invalid_sql = "SELECT 不存在"
  assert_true(
    sqlite3_prepare16_v2(
      database,
      invalid_sql,
      0,
      invalid_sql.length(),
      invalid_statement,
      invalid_tail,
    ) ==
    SQLITE_ERROR,
    msg="invalid SQL did not fail",
  )
  let short_error : FixedArray[UInt16] = FixedArray::make(5, 0xFFFF)
  let error_length = sqlite3_errmsg16_length(database)
  let copied_error_length = sqlite3_errmsg16(
    database,
    short_error,
    short_error.length(),
  )
  assert_true(
    copied_error_length == error_length,
    msg="error length changed while copying",
  )
  assert_true(
    error_length > short_error.length(),
    msg="short buffer unexpectedly held the error",
  )
  assert_true(
    short_error[0] == 0xFFFF &&
    short_error[1] == 0xFFFF &&
    short_error[2] == 0xFFFF &&
    short_error[3] == 0xFFFF &&
    short_error[4] == 0xFFFF,
    msg="short output was partially overwritten",
  )
  let complete_error : FixedArray[UInt16] = FixedArray::make(
    error_length + 1, 0xFFFF,
  )
  assert_true(
    sqlite3_errmsg16(database, complete_error, complete_error.length()) ==
    error_length,
    msg="error length changed while copying",
  )
  assert_true(
    complete_error[error_length] == 0,
    msg="complete error was not NUL-terminated",
  )
  assert_true(
    complete_error[error_length - 3] == 0x4E0D &&
    complete_error[error_length - 2] == 0x5B58 &&
    complete_error[error_length - 1] == 0x5728,
    msg="complete error was not copied as UTF-16",
  )

  let query = prepare(database, "SELECT value FROM 项目", null_handle)
  assert_true(sqlite3_step(query) == SQLITE_ROW, msg="query returned no row")
  assert_true(
    sqlite3_column_int64(query, 0) == 5000000000L,
    msg="query returned wrong value",
  )
  assert_true(
    sqlite3_step(query) == SQLITE_DONE,
    msg="query returned an extra row",
  )
  assert_true(sqlite3_finalize(query) == SQLITE_OK, msg="query finalize failed")

  let batch = "ignored SELECT 42; SELECT 7 ignored"
  let batch_start = "ignored ".length()
  let batch_end = batch.length() - " ignored".length()
  let first_view = batch.view(start_offset=batch_start, end_offset=batch_end)
  let first_statement = Ref(null_handle)
  let first_tail = Ref(batch_start)
  assert_true(
    sqlite3_prepare16_v2(
      database,
      first_view.data(),
      first_view.start_offset(),
      first_view.length(),
      first_statement,
      first_tail,
    ) ==
    SQLITE_OK,
    msg="first batch prepare failed",
  )
  assert_true(
    first_tail.val == batch_start + "SELECT 42;".length(),
    msg="first batch prepare returned the wrong backing-string tail",
  )
  assert_true(
    sqlite3_step(first_statement.val) == SQLITE_ROW,
    msg="first batch query returned no row",
  )
  assert_true(
    sqlite3_column_int64(first_statement.val, 0) == 42L,
    msg="first batch query returned wrong value",
  )
  assert_true(
    sqlite3_finalize(first_statement.val) == SQLITE_OK,
    msg="first batch query finalize failed",
  )

  let second_view = batch.view(
    start_offset=first_tail.val,
    end_offset=batch_end,
  )
  let second_statement = prepare(database, second_view, null_handle)
  assert_true(
    sqlite3_step(second_statement) == SQLITE_ROW,
    msg="second batch query returned no row",
  )
  assert_true(
    sqlite3_column_int64(second_statement, 0) == 7L,
    msg="second batch query returned wrong value",
  )
  assert_true(
    sqlite3_finalize(second_statement) == SQLITE_OK,
    msg="second batch query finalize failed",
  )

  let comment_backing = "xx-- no statement"
  let comment = comment_backing.view(start_offset=2)
  let comment_statement = Ref(null_handle)
  let comment_tail = Ref(comment.start_offset())
  assert_true(
    sqlite3_prepare16_v2(
      database,
      comment.data(),
      comment.start_offset(),
      comment.length(),
      comment_statement,
      comment_tail,
    ) ==
    SQLITE_OK,
    msg="comment-only prepare failed",
  )
  assert_true(
    comment_statement.val == null_handle,
    msg="comment-only prepare returned a statement",
  )
  assert_true(
    comment_tail.val == comment.start_offset() + comment.length(),
    msg="comment-only prepare returned the wrong tail",
  )

  let denied_statement = Ref(null_handle)
  let denied_tail = Ref(0)
  let attach = "ATTACH DATABASE ':memory:' AS other"
  assert_true(
    sqlite3_prepare16_v2(
      database,
      attach,
      0,
      attach.length(),
      denied_statement,
      denied_tail,
    ) ==
    SQLITE_AUTH,
    msg="ATTACH was not denied",
  )
  assert_true(
    denied_statement.val == null_handle,
    msg="denied ATTACH returned a statement",
  )

  let outstanding = prepare(database, "SELECT 1", null_handle)
  assert_true(
    sqlite3_close(database) == SQLITE_BUSY,
    msg="sqlite3_close did not preserve the busy database handle",
  )
  assert_true(
    sqlite3_finalize(outstanding) == SQLITE_OK,
    msg="outstanding statement finalize failed",
  )
  assert_true(sqlite3_close(database) == SQLITE_OK, msg="sqlite3_close failed")
  println("ok")
}
