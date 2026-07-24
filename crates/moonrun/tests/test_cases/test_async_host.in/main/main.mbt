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
fn get_platform() -> Int = "moonbitlang/async" "runtime/get_platform"

///|
fn get_pid() -> Int = "moonbitlang/async" "env_util/getpid"

///|
fn get_errno() -> Int = "moonbitlang/async" "os_error/get_errno"

///|
fn invalid_fd() -> UInt64 = "moonbitlang/async" "fd_util/invalid_fd"

///|
fn open_pid_handle(pid : Int) -> UInt64 = "moonbitlang/async" "process/open_pid_handle"

///|
#unsafe_skip_stub_check
#borrow(out)
fn get_process_result(
  handle : UInt64,
  pid : Int,
  out : Ref[Int],
) -> Int = "moonbitlang/async" "process/get_process_result"

///|
fn ms_since_epoch() -> UInt64 = "moonbitlang/async" "time/get_ms_since_epoch"

///|
#unsafe_skip_stub_check
#borrow(buffer)
fn random_fill(
  buffer : FixedArray[Byte],
  length : Int,
) -> Int = "moonbitlang/async" "random/fill"

///|
#unsafe_skip_stub_check
#borrow(buffer)
fn wasi_random_get(
  buffer : FixedArray[Byte],
  length : Int,
) -> Int = "wasi_snapshot_preview1" "random_get"

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
fn make_sleep_job(ms : Int) -> UInt64 = "moonbitlang/async" "thread_pool/make_sleep_job"

///|
fn free_job(job : UInt64) -> Unit = "moonbitlang/async" "thread_pool/free_job"

///|
fn job_get_ret(job : UInt64) -> Int = "moonbitlang/async" "thread_pool/job_get_ret"

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
fn c_buffer_free(ptr : UInt64) -> Unit = "moonbitlang/async" "c_buffer/free"

///|
fn get_tmp_path_len() -> Int = "moonbitlang/async" "fs/get_tmp_path_len"

///|
fn get_tmp_path(dst : Int, len : Int) -> Int = "moonbitlang/async" "fs/get_tmp_path"

///|
fn get_tmp_path_buffer() -> UInt64 = "moonbitlang/async" "fs/get_tmp_path_buffer"

///|
#borrow(str)
extern "wasm" fn string_to_ptr(str : String) -> Int =
  #|(func (param i32) (result i32) local.get 0)

///|
extern "wasm" fn grow_memory_for_test(pages : Int) -> Int =
  #|(func (param i32) (result i32)
  #|  local.get 0
  #|  memory.grow)

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
  if platform == 1 {
    let out = Ref(0)
    assert_true(
      get_process_result(invalid_fd(), get_pid(), out) == -1,
      "waiting on the current process unexpectedly succeeded",
    )
    assert_true(get_errno() != 0, "failed process wait did not set errno")
    assert_true(
      open_pid_handle(get_pid()) == invalid_fd(),
      "macOS PID handle fallback returned a valid handle",
    )
    assert_true(
      get_errno() == 0,
      "macOS PID handle fallback did not clear stale errno",
    )
  }
  let before = ms_since_epoch()
  assert_true(before > 1_600_000_000_000, "time is not Unix epoch milliseconds")
  let after = ms_since_epoch()
  assert_true(after >= before, "time moved backwards")
  let random_bytes = FixedArray::make(32, b'\x00')
  assert_true(
    random_fill(random_bytes, random_bytes.length()) == 0,
    "secure random fill failed",
  )
  let mut random_byte_changed = false
  for byte in random_bytes {
    if byte != b'\x00' {
      random_byte_changed = true
      break
    }
  }
  assert_true(random_byte_changed, "secure random fill wrote only zeroes")
  assert_true(grow_memory_for_test(1) >= 0, "failed to grow wasm memory")
  assert_true(
    random_fill(random_bytes, random_bytes.length()) == 0,
    "secure random fill failed after memory.grow",
  )
  assert_true(
    random_fill(random_bytes, 2_147_483_647) == -1,
    "invalid async random range did not use the native error convention",
  )
  assert_true(get_errno() != 0, "async random failure did not record errno")
  let wasi_random_bytes = FixedArray::make(32, b'\x00')
  assert_true(
    wasi_random_get(wasi_random_bytes, wasi_random_bytes.length()) == 0,
    "WASI random_get failed",
  )
  let mut wasi_random_byte_changed = false
  for byte in wasi_random_bytes {
    if byte != b'\x00' {
      wasi_random_byte_changed = true
      break
    }
  }
  assert_true(wasi_random_byte_changed, "WASI random_get wrote only zeroes")
  assert_true(
    wasi_random_get(wasi_random_bytes, 2_147_483_647) == 21,
    "invalid WASI random range did not return WASI errno.fault",
  )
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
  let tmp_path_buf = get_tmp_path_buffer()
  let tmp_path_from_buf_len = os_string_decode_len(tmp_path_buf, 0, -1)
  assert_true(tmp_path_from_buf_len > 0, "tmp path buffer size query failed")
  let tmp_path_from_buf = String::make(tmp_path_from_buf_len, '\u{0}')
  os_string_decode(
    tmp_path_buf,
    0,
    -1,
    string_to_ptr(tmp_path_from_buf),
    tmp_path_from_buf_len,
  )
  c_buffer_free(tmp_path_buf)
  assert_true(
    tmp_path_from_buf != String::make(tmp_path_from_buf_len, '\u{0}'),
    "tmp path buffer decode wrote an empty path",
  )
  let tmp_path_len = get_tmp_path_len()
  assert_true(tmp_path_len > 0, "tmp path size query failed")
  assert_true(
    get_tmp_path(0, 0) == -1,
    "tmp path fill accepted a missing buffer",
  )
  let tmp_path = String::make(tmp_path_len, '\u{0}')
  let tmp_path_ptr = string_to_ptr(tmp_path)
  assert_true(
    get_tmp_path(tmp_path_ptr, tmp_path_len) == 0,
    "tmp path fill failed",
  )
  assert_true(
    tmp_path != String::make(tmp_path_len, '\u{0}'),
    "tmp path fill wrote an empty path",
  )
  assert_true(!errno_is_lock_violation(0), "zero errno is a lock violation")
  let bus = event_bus_create()
  assert_true(event_bus_wait(bus, 0) == 0, "empty event bus did not time out")
  let completion_source = init_thread_pool(bus)
  let job = make_sleep_job(0)
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
  assert_true(
    saw_completion, "event bus did not report worker completion source",
  )
  // Windows carries the completed job id in the IOCP event; Unix drains ids
  // from the readable completion pipe.
  if platform != 2 {
    let completed_jobs = FixedArray::make(4, 0)
    assert_true(
      fetch_completion(
        completion_source,
        completed_jobs,
        completed_jobs.length(),
      ) ==
      4,
      "completion drain returned wrong byte count",
    )
    assert_true(
      completed_jobs[0] == 17,
      "completion drain returned wrong job id",
    )
  }
  assert_true(
    job_get_ret(job) == 0,
    "completed worker job result was not readable",
  )
  free_worker(worker)
  free_job(job)
  destroy_thread_pool()
  event_bus_destroy(bus)
  println("ok")
}
