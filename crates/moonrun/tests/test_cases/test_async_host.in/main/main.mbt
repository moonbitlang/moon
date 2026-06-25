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

fn get_platform() -> Int = "moonbitlang/async" "runtime/get_platform"

///|
fn invalid_fd() -> UInt64 = "moonbitlang/async" "fd_util/invalid_fd"

///|
fn ms_since_epoch() -> UInt64 = "moonbitlang/async" "time/get_ms_since_epoch"

///|
fn event_bus_create() -> UInt64 = "moonbitlang/async" "event_bus/create"

///|
fn event_bus_destroy(bus : UInt64) -> Unit = "moonbitlang/async" "event_bus/destroy"

///|
fn event_bus_wait(bus : UInt64, timeout_ms : Int) -> Int = "moonbitlang/async" "event_bus/wait"

///|
fn event_bus_get_event(bus : UInt64, index : Int) -> UInt64 = "moonbitlang/async" "event_bus/get_event"

///|
fn event_bus_event_fd(event : UInt64) -> UInt64 = "moonbitlang/async" "event_bus/event_fd"

///|
fn event_bus_event_events(event : UInt64) -> Int = "moonbitlang/async" "event_bus/event_events/unix"

///|
fn event_bus_event_bytes_transferred(event : UInt64) -> Int = "moonbitlang/async" "event_bus/event_bytes_transferred/windows"

///|
fn init_thread_pool(bus : UInt64) -> UInt64 = "moonbitlang/async" "thread_pool/init_thread_pool"

///|
fn destroy_thread_pool() -> Unit = "moonbitlang/async" "thread_pool/destroy_thread_pool"

///|
fn make_read_job(fd : UInt64, len : Int, position : Int64) -> UInt64 = "moonbitlang/async" "thread_pool/make_read_job"

///|
fn free_job(job : UInt64) -> Unit = "moonbitlang/async" "thread_pool/free_job"

///|
fn spawn_worker(worker_id : Int, job : UInt64) -> UInt64 = "moonbitlang/async" "thread_pool/spawn_worker"

///|
fn free_worker(worker : UInt64) -> Unit = "moonbitlang/async" "thread_pool/free_worker"

///|
#unsafe_skip_stub_check
#borrow(completed_jobs)
fn fetch_completion(
  source_fd : UInt64,
  completed_jobs : FixedArray[Int],
  max_jobs : Int,
) -> Int = "moonbitlang/async" "thread_pool/fetch_completion/unix"

///|
fn errno_to_string(errno : Int) -> UInt64 = "moonbitlang/async" "os_error/errno_to_string"

///|
fn os_string_decode_len(
  ptr : UInt64,
  offset : Int,
  len : Int,
) -> Int = "moonbitlang/async" "os_string/decode_len"

///|
fn os_string_decode(
  ptr : UInt64,
  offset : Int,
  len : Int,
  out : Int,
  out_len : Int,
) -> Unit = "moonbitlang/async" "os_string/decode"

///|
fn free_errno_str(ptr : UInt64) -> Unit = "moonbitlang/async" "os_error/free_errno_str"

///|
fn get_tmp_path_len() -> Int = "moonbitlang/async" "fs/get_tmp_path_len"

///|
fn get_tmp_path(dst : Int, len : Int) -> Int = "moonbitlang/async" "fs/get_tmp_path"

///|
#borrow(str)
extern "wasm" fn string_to_ptr(str : String) -> Int =
  #|(func (param i32) (result i32) local.get 0)

///|
fn errno_is_lock_violation(errno : Int) -> Bool = "moonbitlang/async" "fs/errno_is_lock_violation"

///|
fn assert_true(cond : Bool, message : String) -> Unit {
  if !cond {
    abort(message)
  }
}

///|
fn main {
  let platform = get_platform()
  assert_true(platform >= 0 && platform <= 2, "invalid platform")
  let before = ms_since_epoch()
  assert_true(before > 1_600_000_000_000, "time is not Unix epoch milliseconds")
  let after = ms_since_epoch()
  assert_true(after >= before, "time moved backwards")
  let message_buf = errno_to_string(2)
  let message_len = os_string_decode_len(message_buf, 0, -1)
  assert_true(message_len > 0, "errno string size query failed")
  let message = String::make(message_len, '\u{0}')
  os_string_decode(message_buf, 0, -1, string_to_ptr(message), message_len)
  free_errno_str(message_buf)
  assert_true(
    message != String::make(message_len, '\u{0}'),
    "errno string fill wrote an empty message",
  )
  let tmp_path_len = get_tmp_path_len()
  assert_true(tmp_path_len > 0, "tmp path size query failed")
  assert_true(get_tmp_path(0, 0) == -1, "tmp path fill accepted a missing buffer")
  let tmp_path = String::make(tmp_path_len, '\u{0}')
  let tmp_path_ptr = string_to_ptr(tmp_path)
  assert_true(get_tmp_path(tmp_path_ptr, tmp_path_len) == 0, "tmp path fill failed")
  assert_true(tmp_path != String::make(tmp_path_len, '\u{0}'), "tmp path fill wrote an empty path")
  assert_true(!errno_is_lock_violation(0), "zero errno is a lock violation")
  let bus = event_bus_create()
  assert_true(event_bus_wait(bus, 0) == 0, "empty event bus did not time out")
  let completion_source = init_thread_pool(bus)
  let job = make_read_job(invalid_fd(), 1, -1)
  let worker = spawn_worker(17, job)
  let event_count = event_bus_wait(bus, 1000)
  assert_true(event_count > 0, "worker completion did not wake event bus")
  let mut saw_completion = false
  for i in 0..<event_count {
    let event = event_bus_get_event(bus, i)
    let event_fd = event_bus_event_fd(event)
    if event_fd == completion_source {
      if platform == 2 {
        assert_true(
          event_bus_event_bytes_transferred(event) == 17,
          "IOCP completion did not carry job id",
        )
      } else {
        event_bus_event_events(event) |> ignore
      }
      saw_completion = true
    }
  }
  assert_true(saw_completion, "event bus did not report worker completion source")
  // Windows carries the completed job id in the IOCP event; Unix drains ids
  // from the readable completion pipe.
  if platform != 2 {
    let completed_jobs = FixedArray::make(4, 0)
    assert_true(
      fetch_completion(completion_source, completed_jobs, completed_jobs.length()) == 4,
      "completion drain returned wrong byte count",
    )
    assert_true(completed_jobs[0] == 17, "completion drain returned wrong job id")
  }
  free_worker(worker)
  free_job(job)
  destroy_thread_pool()
  event_bus_destroy(bus)
  println("ok")
}
