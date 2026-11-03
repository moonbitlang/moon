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
  if args[1] == "echo" {
    assert_eq(@stdio.stdin.read_exactly(6), b"hello\n")
    println("terminal roundtrip")
    return
  }
  if args[1] == "terminal" {
    let code = @process.run("/bin/sh", ["-c", "exec \"$1\" \"$2\" echo", "ipc-shell", args[2], args[3]])
    assert_eq(code, 0)
    return
  }
  if args[1] == "host-wait" {
    @signal.set_global_cancellation_signals([])
    ignore(@process.run(
      "/bin/sh",
      ["-c", "printf '%s' \"$MOONRUN_IPC_ENDPOINT\" > endpoint; exec \"$1\" \"$2\" spin > ready", "ipc-shell", args[2], args[3]],
      cwd="child",
    ))
    return
  }
  if args[1] == "probe" {
    println("guest=\{pid()}")
    return
  }
  if args[1] == "spin" {
    @signal.set_global_cancellation_signals([])
    println("ready")
    while true { () }
    return
  }
  if args[1] == "leaf-denied" {
    let denied = try {
      @fs.read_file("input") |> ignore
      false
    } catch { _ => true }
    assert_true(denied)
    println("nested ceiling preserved")
    return
  }
  if args[1] == "narrow" {
    let (code, output) = @process.collect_stdout(
      "/bin/sh",
      ["-c", "exec \"$1\" --policy permissive.toml \"$2\" leaf-denied", "ipc-shell", args[2], args[3]],
    )
    assert_eq(code, 0)
    println(output.text().trim(char_set="\r\n"))
    return
  }
  if args[1] == "nested" {
    let (code, output) = @process.collect_stdout(
      "/bin/sh",
      ["-c", "exec \"$1\" --policy narrow.toml \"$2\" narrow \"$1\" \"$2\"", "ipc-shell", args[2], args[3]],
      cwd="child",
    )
    assert_eq(code, 0)
    println(output.text().trim(char_set="\r\n"))
    return
  }
  if args[1] == "signals" {
    for signal in ["TERM", "KILL"] {
      let (code, output) = @process.collect_stdout(
        "/bin/sh",
        ["-c", "\"$1\" \"$2\" spin > ready & client=$!; while ! test -s ready; do sleep 0.01; done; kill -\"$3\" \"$client\"; wait \"$client\"; status=$?; rm ready; if test \"$3\" = TERM; then test \"$status\" -eq 143; else test \"$status\" -eq 137; fi || exit 90; exec \"$1\" \"$2\" probe", "ipc-shell", args[2], args[3], signal],
        cwd="child",
      )
      assert_eq(code, 0)
      assert_eq(output.text().trim(char_set="\r\n"), "guest=\{pid()}")
    }
    println("client signals and disconnect cleaned up")
    return
  }
  if args[1] == "local" {
    let (code, output) = @process.collect_stdout(
      "/bin/sh", ["-c", "exec \"$1\" \"$2\" probe", "ipc-shell", args[2], args[3]],
    )
    assert_eq(code, 0)
    assert_true(output.text().trim(char_set="\r\n") != "guest=\{pid()}")
    println("local execution preserved")
    return
  }
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
