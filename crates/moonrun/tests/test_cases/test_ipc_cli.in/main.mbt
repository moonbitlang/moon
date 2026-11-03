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

#unsafe_skip_stub_check
fn pid() -> Int = "moonbitlang/async" "env_util/getpid"

#unsafe_skip_stub_check
fn exit(code : Int) = "moonbitlang/async" "runtime/exit"

async fn main {
  let args = @env.args()
  if args[1] == "child" {
    println("guest=\{pid()}")
    assert_eq(args[2], "arg with spaces")
    assert_eq(@env.get_env_var("IPC_VALUE"), Some("caller"))
    assert_eq(@fs.read_file("input").text(), "caller cwd")
    let denied = try {
      @fs.read_file("../secret") |> ignore
      false
    } catch { _ => true }
    assert_true(denied)
    println("caller state and ceiling preserved")
    exit(23)
    return
  }
  println("host=\{pid()}")
  let (code, output) = @process.collect_stdout(
    "/bin/sh",
    ["-c", "printf 'client=%s\\n' \"$$\"; exec \"$1\" \"$2\" child 'arg with spaces'", "ipc-shell", args[2], args[3]],
    cwd="child",
    extra_env={ "IPC_VALUE": "caller" },
  )
  println(output.text().trim(char_set="\r\n"))
  assert_eq(code, 23)
}
