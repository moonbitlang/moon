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
#unsafe_skip_stub_check
fn exit(code : Int) = "moonbitlang/async" "runtime/exit"

///|
async fn main {
  let args = @env.args()
  let mode = args[2]
  if args[1] == "child" {
    println("ready")
    if mode == "exit" {
      exit(23)
    } else if mode == "retry" {
      assert_eq(@stdio.stdin.read_exactly(1), b"G")
      exit(23)
    } else if mode == "busy" {
      while true { () }
    } else {
      defer println("cleanup")
      @async.sleep(60_000)
    }
    return
  }
  let child = @string.parse_int(@stdio.stdin.read_until("\n").unwrap().trim())
  let expected = @string.parse_int(args[3])
  if mode == "retry" {
    @async.with_task_group() <| group => {
      let waiting = group.spawn(() => @process.wait_pid(child))
      @async.sleep(10)
      waiting.cancel()
    }
    println("retry")
    assert_eq(@process.wait_pid(child), expected)
  } else if mode == "exit" {
    assert_eq(@process.wait_pid(child), expected)
  } else {
    // Use the same cancellation handler and wait API as process.run.
    let @process.CancellationHandler(cancel) = @process.graceful_cancel(timeout=50)
    @async.with_task_group() <| group => {
      group.spawn_bg(no_wait=true, () => cancel(child))
      assert_eq(@process.wait_pid(child), expected)
    }
  }
  println("done")
}
