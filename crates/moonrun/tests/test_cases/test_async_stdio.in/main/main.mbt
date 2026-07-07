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
fn get_stdio_handle(id : Int) -> UInt64 = "moonbitlang/async" "stdio/get_stdio_handle"

///|
fn kind_of_fd(fd : UInt64) -> Int = "moonbitlang/async" "fd_util/kind_of_fd"

///|
fn make_read_job(
  fd : UInt64,
  len : Int,
  position : Int64,
) -> UInt64 = "moonbitlang/async" "thread_pool/make_read_job"

///|
#unsafe_skip_stub_check
#borrow(buf)
fn get_read_result(
  job : UInt64,
  buf : FixedArray[Byte],
  offset : Int,
  len : Int,
) -> Unit = "moonbitlang/async" "thread_pool/get_read_result"

///|
#unsafe_skip_stub_check
#borrow(buf)
fn make_write_job(
  fd : UInt64,
  buf : Bytes,
  offset : Int,
  len : Int,
  position : Int64,
) -> UInt64 = "moonbitlang/async" "thread_pool/make_write_job"

///|
fn run_job(job : UInt64) -> Unit = "moonbitlang/async" "thread_pool/run_job"

///|
fn free_job(job : UInt64) -> Unit = "moonbitlang/async" "thread_pool/free_job"

///|
fn job_get_ret(job : UInt64) -> Int = "moonbitlang/async" "thread_pool/job_get_ret"

///|
fn job_get_err(job : UInt64) -> Int = "moonbitlang/async" "thread_pool/job_get_err"

///|
fn assert_true(cond : Bool, message : String) -> Unit {
  if !cond {
    abort(message)
  }
}

///|
fn assert_file_kind(fd : UInt64, label : String) -> Unit {
  let kind = kind_of_fd(fd)
  assert_true(kind > 0, "\{label} was not classified as a concrete file kind")
}

///|
fn write_all(fd : UInt64, data : Bytes, label : String) -> Unit {
  let no_position : Int64 = -1
  let job = make_write_job(fd, data, 0, data.length(), no_position)
  run_job(job)
  assert_true(job_get_err(job) == 0, "\{label} write failed")
  assert_true(job_get_ret(job) == data.length(), "\{label} write was partial")
  free_job(job)
}

///|
fn main {
  let stdin = get_stdio_handle(0)
  let stdout = get_stdio_handle(1)
  let stderr = get_stdio_handle(2)
  assert_file_kind(stdin, "stdin")
  assert_file_kind(stdout, "stdout")
  assert_file_kind(stderr, "stderr")

  let no_position : Int64 = -1
  let input = FixedArray::make(12, b'\x00')
  let read = make_read_job(stdin, input.length(), no_position)
  run_job(read)
  assert_true(job_get_err(read) == 0, "stdin read failed")
  assert_true(job_get_ret(read) == input.length(), "stdin read was partial")
  get_read_result(read, input, 0, input.length())
  assert_true(input[0] == b's', "stdin content did not start with expected byte")
  assert_true(input[11] == b'\n', "stdin content did not end with newline")
  free_job(read)

  write_all(stdout, b"stdout-ok\n", "stdout")
  write_all(stderr, b"stderr-ok\n", "stderr")
}
