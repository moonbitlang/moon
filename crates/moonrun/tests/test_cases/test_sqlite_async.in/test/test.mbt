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
async test "discard an unaccepted open without publishing a database handle" {
  let unaccepted = make_open_job(b":memory:", 8, 6)
  assert_eq(perform_host_job(unaccepted, context="unaccepted open"), 0)
  let discard = make_discard_job(unaccepted)
  assert_true(discard != null_handle())
  assert_eq(make_discard_job(unaccepted), null_handle())
  free_job(unaccepted)
  assert_eq(perform_host_job(discard, context="discard open"), 0)
  free_job(discard)
}

///|
async test "SQLite jobs remain ordered alongside filesystem work" {
  let database = Database::open()
  defer assert_eq(close(database.handle), 0)
  assert_eq(database.execute("CREATE TABLE entries(value INTEGER)"), Some(0L))
  @async.with_task_group() <| group => {
    for value in [1, 2, 3] {
      group.spawn_bg() <| () => {
        assert_eq(
          database.execute("INSERT INTO entries VALUES (\{value})"),
          Some(1L),
        )
      }
    }
    group.spawn_bg() <| () => { ignore(@fs.read_file("moon.mod")) }
  }
  let ordered = database.prepare(
    "SELECT group_concat(value, '') + 0 FROM entries",
  )
  assert_eq(database.step(ordered), (100, None))
  assert_eq(column_int64(ordered, 0), 123L)
  assert_eq(database.step(ordered), (101, None))
  database.finalize(ordered)
  assert_eq(
    database.execute("UPDATE entries SET value=9 WHERE value=99"),
    Some(0L),
  )
}

///|
async test "prepare tail is relative to the SQL view" {
  let database = Database::open()
  defer assert_eq(close(database.handle), 0)
  let view = make_prepare_job(database.handle, "xxSELECT 42;yy", 2, 10)
  assert_eq(database.run(view), 0)
  assert_eq(job_result(view), (0, 0, 10, None))
  let statement = take_handle(view)
  free_job(view)
  assert_eq(database.step(statement), (100, None))
  assert_eq(column_int64(statement, 0), 42L)
  assert_eq(finalize(statement), 0)
}

///|
async test "job error snapshots survive later operations" {
  let database = Database::open()
  defer assert_eq(close(database.handle), 0)
  let failed = make_prepare_job(
    database.handle,
    "SELECT missing_column",
    0,
    21,
  )
  defer free_job(failed)
  assert_eq(database.run(failed), 1)
  assert_eq(database.execute("CREATE TABLE entries(value INTEGER)"), Some(0L))
  assert_true(job_message(failed).contains("missing_column"))
}

///|
async test "prepare discards rejected trailing statements" {
  let database = Database::open()
  defer assert_eq(close(database.handle), 0)
  try database.prepare("SELECT 1; SELECT 2") catch {
    InvalidSql => ()
    error => raise error
  } noraise {
    _ => fail("expected rejected SQL")
  }
}

///|
async test "cancelled step preserves event loop progress and connection reuse" {
  let database = Database::open()
  defer assert_eq(close(database.handle), 0)
  let slow = database.prepare(
    "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<2000000) SELECT sum(x) FROM n",
  )
  let ticks = Ref(0)
  let finished = Ref(false)
  @async.with_task_group() <| group => {
    group.spawn_bg() <| () => {
      while !finished.val {
        @async.sleep(1)
        if !finished.val {
          ticks.val += 1
        }
      }
    }
    group.spawn_bg() <| () => {
      defer {
        finished.val = true
      }
      try @async.with_timeout(1, () => database.step(slow)) catch {
        @async.TimeoutError => ()
        error => raise error
      } noraise {
        _ => fail("expected cancellation")
      }
    }
  }
  assert_true(ticks.val > 0)
  assert_eq(finalize(slow), 0)
  assert_eq(database.execute("CREATE TABLE after_cancel(value INTEGER)"), Some(0L))
}
