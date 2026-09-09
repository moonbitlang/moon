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
#unsafe_skip_stub_check
#borrow(filename)
fn make_open_job(filename : Bytes, length : Int, flags : Int) -> UInt64 = "moonbitlang/sqlite" "sqlite3_make_open_job"

///|
#unsafe_skip_stub_check
#borrow(sql)
fn make_prepare_job(
  database : UInt64,
  sql : String,
  offset : Int,
  length : Int,
) -> UInt64 = "moonbitlang/sqlite" "sqlite3_make_prepare_job"

///|
fn make_step_job(database : UInt64, statement : UInt64) -> UInt64 = "moonbitlang/sqlite" "sqlite3_make_step_job"

///|
fn make_finalize_job(database : UInt64, statement : UInt64) -> UInt64 = "moonbitlang/sqlite" "sqlite3_make_finalize_job"

///|
fn make_discard_job(job : UInt64) -> UInt64 = "moonbitlang/sqlite" "sqlite3_make_discard_job"

///|
fn null_handle() -> UInt64 = "moonbitlang/sqlite" "sqlite3_null_handle"

///|
#unsafe_skip_stub_check
#borrow(output)
fn copy_result(job : UInt64, output : FixedArray[Int]) -> Unit = "moonbitlang/sqlite" "sqlite3_job_result"

///|
fn take_handle(job : UInt64) -> UInt64 = "moonbitlang/sqlite" "sqlite3_take_job_handle"

///|
fn message_length(job : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_job_message16_length"

///|
#unsafe_skip_stub_check
#borrow(output)
fn copy_message(job : UInt64, output : String, capacity : Int) -> Int = "moonbitlang/sqlite" "sqlite3_job_message16"

///|
fn free_job(job : UInt64) -> Unit = "moonbitlang/async" "thread_pool/free_job"

///|
fn close(database : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_close"

///|
fn finalize(statement : UInt64) -> Int = "moonbitlang/sqlite" "sqlite3_finalize"

///|
fn column_int64(statement : UInt64, column : Int) -> Int64 = "moonbitlang/sqlite" "sqlite3_column_int64"

///|
struct Database {
  handle : UInt64
  mutex : @async.Mutex
}

///|
fn job_message(job : UInt64) -> String raise {
  let length = message_length(job)
  let output = String::make(length, '\u0000')
  assert_eq(copy_message(job, output, length), length)
  output
}

///|
fn job_result(job : UInt64) -> (Int, Int, Int, Int64?) {
  let output = FixedArray::make(6, 0)
  copy_result(job, output)
  let changes = if output[3] == 0 {
    None
  } else {
    Some(
      output[4].reinterpret_as_uint().to_int64() | (output[5].to_int64() << 32),
    )
  }
  (output[0], output[1], output[2], changes)
}

///|
async fn Database::open() -> Database {
  @async.protect_from_cancel() <| () => {
    let job = make_open_job(b":memory:", 8, 6)
    defer free_job(job)
    assert_eq(@async.perform_host_job(job, context="sqlite open"), 0)
    { handle: take_handle(job), mutex: @async.Mutex(), }
  }
}

///|
async fn Database::run(self : Database, job : UInt64) -> Int {
  self.mutex.acquire()
  defer self.mutex.release()
  @async.perform_host_job(job, context="sqlite job")
}

///|
suberror InvalidSql

///|
async fn Database::prepare(self : Database, sql : String) -> UInt64 {
  let job = make_prepare_job(self.handle, sql, 0, sql.length())
  defer free_job(job)
  @async.protect_from_cancel(() => assert_eq(self.run(job), 0))
  let (_, _, tail, _) = job_result(job)
  if tail < 0 {
    raise InvalidSql
  }
  if tail != sql.length() || @async.is_being_cancelled() {
    @async.protect_from_cancel() <| () => {
      let discard = make_discard_job(job)
      assert_true(discard != null_handle())
      defer free_job(discard)
      assert_eq(self.run(discard), 0)
    }
    if tail != sql.length() {
      raise InvalidSql
    }
    @async.pause()
  }
  take_handle(job)
}

///|
async fn Database::finalize(self : Database, statement : UInt64) -> Unit {
  @async.protect_from_cancel() <| () => {
    let job = make_finalize_job(self.handle, statement)
    defer free_job(job)
    assert_eq(self.run(job), 0)
  }
}

///|
async fn Database::step(self : Database, statement : UInt64) -> (Int, Int64?) {
  let result = @async.protect_from_cancel() <| () => {
    let job = make_step_job(self.handle, statement)
    defer free_job(job)
    let status = self.run(job)
    let (code, extended, _, changes) = job_result(job)
    assert_eq(status, code)
    assert_eq(code, extended)
    (code, changes)
  }
  if @async.is_being_cancelled() {
    @async.pause()
  }
  result
}

///|
async fn Database::execute(self : Database, sql : String) -> Int64? {
  let statement = self.prepare(sql)
  defer self.finalize(statement)
  let (code, changes) = self.step(statement)
  assert_eq(code, 101)
  changes
}
