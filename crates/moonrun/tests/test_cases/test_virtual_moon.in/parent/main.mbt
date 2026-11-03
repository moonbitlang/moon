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
  let program = "test/virtual/main@0.0.1"
  if args.length() > 1 && args[1] == "denied" {
    let denied = try {
      ignore(@process.run("moonx", [program]))
      false
    } catch { _ => true }
    assert_true(denied)
    println("spawn denied")
    return
  }
  if args.length() > 1 && args[1] == "orphan" {
    println("root spawning")
    ignore(@process.spawn_orphan("moonx", [program, "adopted"]))
    return
  }
  if args.length() > 1 && args[1] == "orphan-loop" {
    // Let the CLI own SIGTERM even if this root has not returned when its
    // child prints ready. Engine tests separately cover adoption ordering.
    @signal.set_global_cancellation_signals([])
    println("root spawning")
    ignore(@process.spawn_orphan("moonx", [program, "loop"]))
    return
  }
  if args.length() > 1 && args[1] == "adopted" {
    println("adopted child completed")
    return
  }
  if args.length() > 1 && args[1] == "wait" {
    println("ready")
    assert_eq(@stdio.stdin.read_exactly(2), b"go")
    exit(23)
    return
  }
  if args.length() > 1 && args[1] == "loop" {
    println("ready")
    while true { () }
    return
  }
  println(pid())
  if args.length() > 1 {
    assert_eq(@env.get_env_var("VIRTUAL_CHAIN"), Some("child"))
    let (code, output) = @process.collect_stdout(
      "moonx",
      ["--virtual-process-tree", "leaf.mbtx", "arg with spaces", "--help", "--target", "native", "--moonrun-artifact-handoff"],
      cwd="subdir",
      extra_env={ "VIRTUAL_CHAIN": "grandchild" },
    )
    assert_eq(code, 0)
    println(output.text().trim(char_set="\r\n"))
  } else {
    let (code, output) = @process.collect_stdout(
      "moonx",
      ["--virtual-process-tree", "--experimental-policy", "child-policy.toml", program, "--", "child"],
      extra_env={ "VIRTUAL_CHAIN": "child" },
    )
    assert_eq(code, 0)
    println(output.text().trim(char_set="\r\n"))

    // Only moonx is a guest virtual execution entry; Moon subcommands must
    // fail at spawn rather than accidentally entering artifact preparation.
    for arguments in [["build"], ["run", "-"], ["run", "-e", "fn main {}"]] {
      let rejected = try {
        ignore(@process.run("moon", arguments))
        false
      } catch { _ => true }
      assert_true(rejected)
    }
    for flag in ["--help", "--version"] {
      let (code, output) = @process.collect_stdout("moonx", [flag])
      assert_eq(code, 0)
      assert_true(output.text().contains("moonx"))
    }
    for arguments in [[], ["--target", "native"], ["--unknown-option"],
      ["--moonrun-artifact-handoff", "untrusted-handoff", "subdir/leaf.mbtx"]] {
      let (code, error) = @process.collect_stderr("moonx", arguments)
      assert_eq(code, 2)
      assert_true(error.text().length() > 0)
    }

    // A cancelled wait leaves the child available for a later wait_pid.
    let (read, output) = @process.read_from_process()
    defer read.close()
    let (input, write) = @process.write_to_process()
    defer write.close()
    let child = @process.spawn_orphan(
      "moonx", [program, "wait"],
      stdin=input, stdout=output,
    )
    assert_eq(read.read_exactly(6), b"ready\n")
    @async.with_task_group() <| group => {
      let waiting = group.spawn(() => @process.wait_pid(child))
      @async.sleep(10)
      waiting.cancel()
    }
    write.write("go")
    assert_eq(@process.wait_pid(child), 23)

    // Cancel only after the Wasm has started: the native build helper is gone.
    @async.with_task_group() <| group => {
      let (read, output) = @process.read_from_process()
      defer read.close()
      let running = group.spawn(() => @process.run(
        "moonx", [program, "loop"],
        stdout=output, cancel_handler=@process.hard_cancel(),
      ))
      assert_eq(read.read_exactly(6), b"ready\n")
      running.cancel()
    }

    let (code, error) = @process.collect_stderr(
      "moonx", ["invalid.mbtx"],
    )
    assert_true(code != 0)
    assert_true(error.text().length() > 0)
  }
}
