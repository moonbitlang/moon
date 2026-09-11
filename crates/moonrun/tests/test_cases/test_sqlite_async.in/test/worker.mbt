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

// This fixture is an external consumer of async: only public pipe/io APIs
// and moonrun imports are used. Cancellation must wait for SQLite to finish.

///|
fn spawn_worker_with_pipe(id : Int, job : UInt64, writer : UInt64) -> UInt64 = "moonbitlang/async" "thread_pool/spawn_worker_with_pipe"

///|
fn wake_worker(worker : UInt64, id : Int, job : UInt64) -> Unit = "moonbitlang/async" "thread_pool/wake_worker"

///|
fn free_worker(worker : UInt64) -> Unit = "moonbitlang/async" "thread_pool/free_worker"

///|
fn job_get_ret(job : UInt64) -> Int = "moonbitlang/async" "thread_pool/job_get_ret"

///|
fn job_get_err(job : UInt64) -> Int = "moonbitlang/async" "thread_pool/job_get_err"

///|
async fn perform_host_job(job : UInt64, context~ : String) -> Int {
  @async.protect_from_cancel() <| () => {
    let (reader, writer) = @pipe.pipe()
    defer reader.close()
    defer writer.close()
    let worker = spawn_worker_with_pipe(1, job, writer.fd())
    defer free_worker(worker)
    writer.close()
    assert_eq(reader.read_exactly(4), b"\x01\x00\x00\x00")
    let error = job_get_err(job)
    if error != 0 {
      fail("\{context}: host errno \{error}")
    }
    job_get_ret(job)
  }
}

///|
async test "reuse a worker through a public completion pipe" {
  let (reader, writer) = @pipe.pipe()
  defer reader.close()
  defer writer.close()
  // Activate the writer's IOCP registration before the host borrows it.
  writer.write(b"ping")
  assert_eq(reader.read_exactly(4), b"ping")
  let open = make_open_job(b":memory:", 8, 6)
  let worker = spawn_worker_with_pipe(0x01020304, open, writer.fd())
  defer free_worker(worker)
  writer.close()
  assert_eq(reader.read_exactly(4), b"\x04\x03\x02\x01")
  // Exercise a SQLite result accessor directly, without job_get_err first.
  let database = take_handle(open)
  free_job(open)
  defer assert_eq(close(database), 0)
  let prepare = make_prepare_job(database, "SELECT 42", 0, 9)
  wake_worker(worker, -1, prepare)
  @async.with_task_group() <| group => {
    group.spawn_bg() <| () => { ignore(@fs.read_file("moon.mod")) }
    assert_eq(reader.read_exactly(4), b"\xff\xff\xff\xff")
  }
  assert_eq(job_get_ret(prepare), 0)
  let statement = take_handle(prepare)
  free_job(prepare)
  assert_eq(finalize(statement), 0)
}
