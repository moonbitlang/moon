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
const SQLITE_CONSTRAINT = 19

///|
const SQLITE_CANTOPEN = 14

///|
const SQLITE_AUTH = 23

///|
const SQLITE_RANGE = 25

///|
const SQLITE_CONSTRAINT_UNIQUE = 2067

///|
const SQLITE_ROW = 100

///|
const SQLITE_DONE = 101

///|
const SQLITE_INTEGER = 1

///|
const SQLITE_FLOAT = 2

///|
const SQLITE_TEXT = 3

///|
const SQLITE_BLOB = 4

///|
const SQLITE_NULL = 5

///|
const SQLITE_OPEN_READONLY = 0x00000001

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
fn sqlite3_errcode(database : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_errcode"

///|
fn sqlite3_extended_errcode(database : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_extended_errcode"

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
fn sqlite3_bind_null(statement : UInt64, index : Int) -> Int = "moonbitlang/sqlite" "sqlite3_bind_null"

///|
fn sqlite3_bind_int64(statement : UInt64, index : Int, value : Int64) -> Int = "moonbitlang/sqlite" "sqlite3_bind_int64"

///|
fn sqlite3_bind_double(statement : UInt64, index : Int, value : Double) -> Int = "moonbitlang/sqlite" "sqlite3_bind_double"

///|
#unsafe_skip_stub_check
#borrow(value)
fn sqlite3_bind_text16(
  statement : UInt64,
  index : Int,
  value : String,
  value_offset : Int,
  value_length : Int,
) -> Int = "moonbitlang/sqlite" "sqlite3_bind_text16"

///|
#unsafe_skip_stub_check
#borrow(value)
fn sqlite3_bind_blob(
  statement : UInt64,
  index : Int,
  value : Bytes,
  value_offset : Int,
  value_length : Int,
) -> Int = "moonbitlang/sqlite" "sqlite3_bind_blob"

///|
fn sqlite3_step(statement : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_step"

///|
fn sqlite3_finalize(statement : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_finalize"

///|
fn sqlite3_column_count(statement : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_column_count"

///|
fn sqlite3_column_type(statement : UInt64, column : Int) -> Int = "moonbitlang/sqlite" "sqlite3_column_type"

///|
fn sqlite3_column_int64(statement : UInt64, column : Int) -> Int64 = "moonbitlang/sqlite" "sqlite3_column_int64"

///|
fn sqlite3_column_double(statement : UInt64, column : Int) -> Double = "moonbitlang/sqlite" "sqlite3_column_double"

///|
fn sqlite3_column_text16_length(statement : UInt64, column : Int) -> Int = "moonbitlang/sqlite" "sqlite3_column_text16_length"

///|
#unsafe_skip_stub_check
#borrow(output)
fn sqlite3_column_text16(
  statement : UInt64,
  column : Int,
  output : FixedArray[UInt16],
  capacity : Int,
) -> Int = "moonbitlang/sqlite" "sqlite3_column_text16"

///|
fn sqlite3_column_blob_length(statement : UInt64, column : Int) -> Int = "moonbitlang/sqlite" "sqlite3_column_blob_length"

///|
#unsafe_skip_stub_check
#borrow(output)
fn sqlite3_column_blob(
  statement : UInt64,
  column : Int,
  output : FixedArray[Byte],
  capacity : Int,
) -> Int = "moonbitlang/sqlite" "sqlite3_column_blob"

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

  let denied = Ref(null_handle)
  let denied_filename = b"denied/database.sqlite\x00"
  assert_true(
    sqlite3_open_v2(
      denied_filename,
      denied_filename.length(),
      denied,
      SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
      null_handle,
    ) ==
    SQLITE_CANTOPEN,
    msg="database outside the filesystem policy was not rejected",
  )
  assert_true(
    denied.val == null_handle,
    msg="denied database returned a handle",
  )

  let file_database = Ref(null_handle)
  let file_database_filename = b"allowed/database.sqlite\x00"
  assert_true(
    sqlite3_open_v2(
      file_database_filename,
      file_database_filename.length(),
      file_database,
      SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
      null_handle,
    ) ==
    SQLITE_OK,
    msg="file-backed database open failed",
  )
  let file_database = file_database.val
  execute(file_database, "CREATE TABLE persisted(value INTEGER)", null_handle)
  execute(file_database, "INSERT INTO persisted VALUES (42)", null_handle)
  assert_true(
    sqlite3_close(file_database) == SQLITE_OK,
    msg="file-backed database close failed",
  )

  let reopened = Ref(null_handle)
  assert_true(
    sqlite3_open_v2(
      file_database_filename,
      file_database_filename.length(),
      reopened,
      SQLITE_OPEN_READONLY,
      null_handle,
    ) ==
    SQLITE_OK,
    msg="file-backed database reopen failed",
  )
  let reopened = reopened.val
  let persisted = prepare(reopened, "SELECT value FROM persisted", null_handle)
  assert_true(
    sqlite3_step(persisted) == SQLITE_ROW,
    msg="file-backed database returned no row",
  )
  assert_true(
    sqlite3_column_int64(persisted, 0) == 42L,
    msg="file-backed value did not persist",
  )
  assert_true(
    sqlite3_finalize(persisted) == SQLITE_OK,
    msg="file-backed query finalize failed",
  )
  assert_true(
    sqlite3_close(reopened) == SQLITE_OK,
    msg="file-backed database reopen close failed",
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
  assert_true(
    sqlite3_errcode(database) == SQLITE_ERROR,
    msg="sqlite3_errcode did not report the prepare error",
  )
  assert_true(
    sqlite3_extended_errcode(database) == SQLITE_ERROR,
    msg="sqlite3_extended_errcode did not report the prepare error",
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
    error_length + 1,
    0xFFFF,
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

  let bound = prepare(database, "SELECT ?1, ?2, ?3, ?4, ?5", null_handle)
  let text_backing = "xx值😀yy"
  let text = text_backing.view(
    start_offset=2,
    end_offset=text_backing.length() - 2,
  )
  let blob_backing = b"\xAA\x00\x7F\xFF\xBB"
  let blob = blob_backing[1:4]
  assert_true(
    sqlite3_bind_int64(bound, 1, 5000000000L) == SQLITE_OK,
    msg="sqlite3_bind_int64 failed",
  )
  assert_true(
    sqlite3_bind_double(bound, 2, 3.5) == SQLITE_OK,
    msg="sqlite3_bind_double failed",
  )
  assert_true(
    sqlite3_bind_text16(
      bound,
      3,
      text.data(),
      text.start_offset(),
      text.length(),
    ) ==
    SQLITE_OK,
    msg="sqlite3_bind_text16 failed",
  )
  assert_true(
    sqlite3_bind_blob(bound, 4, blob.data(), blob.start_offset(), blob.length()) ==
    SQLITE_OK,
    msg="sqlite3_bind_blob failed",
  )
  assert_true(
    sqlite3_bind_null(bound, 5) == SQLITE_OK,
    msg="sqlite3_bind_null failed",
  )
  assert_true(
    sqlite3_bind_null(bound, 6) == SQLITE_RANGE,
    msg="out-of-range bind did not return SQLITE_RANGE",
  )
  assert_true(
    sqlite3_step(bound) == SQLITE_ROW,
    msg="bound query returned no row",
  )
  assert_true(
    sqlite3_column_count(bound) == 5,
    msg="bound query returned the wrong column count",
  )
  assert_true(
    sqlite3_column_type(bound, 0) == SQLITE_INTEGER &&
    sqlite3_column_type(bound, 1) == SQLITE_FLOAT &&
    sqlite3_column_type(bound, 2) == SQLITE_TEXT &&
    sqlite3_column_type(bound, 3) == SQLITE_BLOB &&
    sqlite3_column_type(bound, 4) == SQLITE_NULL,
    msg="bound query returned the wrong SQLite types",
  )
  assert_true(
    sqlite3_column_int64(bound, 0) == 5000000000L,
    msg="bound integer did not round-trip",
  )
  assert_true(
    sqlite3_column_double(bound, 1) == 3.5,
    msg="bound double did not round-trip",
  )

  let text_length = sqlite3_column_text16_length(bound, 2)
  assert_true(
    text_length == text.length(),
    msg="UTF-16 column returned the wrong length",
  )
  let short_text : FixedArray[UInt16] = FixedArray::make(2, 0xFFFF)
  assert_true(
    sqlite3_column_text16(bound, 2, short_text, short_text.length()) ==
    text_length,
    msg="short UTF-16 copy returned the wrong length",
  )
  assert_true(
    short_text[0] == 0xFFFF && short_text[1] == 0xFFFF,
    msg="short UTF-16 output was partially overwritten",
  )
  let text_output : FixedArray[UInt16] = FixedArray::make(
    text_length + 1,
    0xFFFF,
  )
  assert_true(
    sqlite3_column_text16(bound, 2, text_output, text_output.length()) ==
    text_length,
    msg="UTF-16 copy returned the wrong length",
  )
  assert_true(
    text_output[0] == 0x503C &&
    text_output[1] == 0xD83D &&
    text_output[2] == 0xDE00 &&
    text_output[3] == 0,
    msg="UTF-16 column did not round-trip",
  )

  let blob_length = sqlite3_column_blob_length(bound, 3)
  assert_true(blob_length == 3, msg="blob column returned the wrong length")
  let short_blob : FixedArray[Byte] = FixedArray::make(2, 0xAA)
  assert_true(
    sqlite3_column_blob(bound, 3, short_blob, short_blob.length()) ==
    blob_length,
    msg="short blob copy returned the wrong length",
  )
  assert_true(
    short_blob[0] == 0xAA && short_blob[1] == 0xAA,
    msg="short blob output was partially overwritten",
  )
  let blob_output : FixedArray[Byte] = FixedArray::make(blob_length, 0)
  assert_true(
    sqlite3_column_blob(bound, 3, blob_output, blob_output.length()) ==
    blob_length,
    msg="blob copy returned the wrong length",
  )
  assert_true(
    blob_output[0] == 0 && blob_output[1] == 0x7F && blob_output[2] == 0xFF,
    msg="blob column did not round-trip",
  )
  assert_true(
    sqlite3_finalize(bound) == SQLITE_OK,
    msg="bound query finalize failed",
  )

  execute(
    database, "CREATE TABLE unique_values(value INTEGER UNIQUE)", null_handle,
  )
  execute(database, "INSERT INTO unique_values VALUES (7)", null_handle)
  let duplicate = prepare(
    database, "INSERT INTO unique_values VALUES (?1)", null_handle,
  )
  assert_true(
    sqlite3_bind_int64(duplicate, 1, 7L) == SQLITE_OK,
    msg="duplicate value bind failed",
  )
  assert_true(
    sqlite3_step(duplicate) == SQLITE_CONSTRAINT,
    msg="duplicate value did not fail with SQLITE_CONSTRAINT",
  )
  assert_true(
    sqlite3_errcode(database) == SQLITE_CONSTRAINT,
    msg="sqlite3_errcode did not report the constraint error",
  )
  assert_true(
    sqlite3_extended_errcode(database) == SQLITE_CONSTRAINT_UNIQUE,
    msg="sqlite3_extended_errcode did not report the unique constraint",
  )
  assert_true(
    sqlite3_errmsg16_length(database) > 0,
    msg="constraint error did not provide a message",
  )
  assert_true(
    sqlite3_finalize(duplicate) == SQLITE_CONSTRAINT,
    msg="failed statement finalize did not preserve its result code",
  )

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
