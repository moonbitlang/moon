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

use std::io::Write;
use std::path::PathBuf;

fn moon_cmd() -> snapbox::cmd::Command {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../moon/Cargo.toml");
    snapbox::cmd::Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--bin")
        .arg("moon")
        .arg("--")
}

const WASI_ERRNO_NOTSUP: i32 = 58;

#[derive(Clone, Copy)]
enum PollOneoffCase {
    ClockUnsupported,
    StdinReady,
    StdinHangup,
    StdoutWritable,
}

fn poll_oneoff_wasm(case: PollOneoffCase) -> Vec<u8> {
    use wasm_encoder::{
        BlockType, CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection,
        Function, FunctionSection, ImportSection, Instruction, MemArg, MemorySection, MemoryType,
        Module, TypeSection, ValType,
    };

    fn memarg(align: u32) -> MemArg {
        MemArg {
            offset: 0,
            align,
            memory_index: 0,
        }
    }

    fn i32_store(function: &mut Function, offset: i32, value: i32) {
        function
            .instruction(&Instruction::I32Const(offset))
            .instruction(&Instruction::I32Const(value))
            .instruction(&Instruction::I32Store(memarg(2)));
    }

    fn i64_store(function: &mut Function, offset: i32, value: i64) {
        function
            .instruction(&Instruction::I32Const(offset))
            .instruction(&Instruction::I64Const(value))
            .instruction(&Instruction::I64Store(memarg(3)));
    }

    fn i32_store8(function: &mut Function, offset: i32, value: i32) {
        function
            .instruction(&Instruction::I32Const(offset))
            .instruction(&Instruction::I32Const(value))
            .instruction(&Instruction::I32Store8(memarg(0)));
    }

    fn i32_store16(function: &mut Function, offset: i32, value: i32) {
        function
            .instruction(&Instruction::I32Const(offset))
            .instruction(&Instruction::I32Const(value))
            .instruction(&Instruction::I32Store16(memarg(1)));
    }

    fn fail_if_nonzero(function: &mut Function) {
        function
            .instruction(&Instruction::If(BlockType::Empty))
            .instruction(&Instruction::Call(4))
            .instruction(&Instruction::End);
    }

    fn check_i32_eq(function: &mut Function, offset: i32, expected: i32) {
        function
            .instruction(&Instruction::I32Const(offset))
            .instruction(&Instruction::I32Load(memarg(2)))
            .instruction(&Instruction::I32Const(expected))
            .instruction(&Instruction::I32Ne);
        fail_if_nonzero(function);
    }

    fn check_u16_eq(function: &mut Function, offset: i32, expected: i32) {
        function
            .instruction(&Instruction::I32Const(offset))
            .instruction(&Instruction::I32Load16U(memarg(1)))
            .instruction(&Instruction::I32Const(expected))
            .instruction(&Instruction::I32Ne);
        fail_if_nonzero(function);
    }

    fn check_i64_eq(function: &mut Function, offset: i32, expected: i64) {
        function
            .instruction(&Instruction::I32Const(offset))
            .instruction(&Instruction::I64Load(memarg(3)))
            .instruction(&Instruction::I64Const(expected))
            .instruction(&Instruction::I64Ne);
        fail_if_nonzero(function);
    }

    fn check_i64_nonzero(function: &mut Function, offset: i32) {
        function
            .instruction(&Instruction::I32Const(offset))
            .instruction(&Instruction::I64Load(memarg(3)))
            .instruction(&Instruction::I64Eqz);
        fail_if_nonzero(function);
    }

    fn fd_write_body(function: &mut Function, data_offset: i32, data_len: i32) {
        i32_store(function, 160, data_offset);
        i32_store(function, 164, data_len);
        function
            .instruction(&Instruction::I32Const(1))
            .instruction(&Instruction::I32Const(160))
            .instruction(&Instruction::I32Const(1))
            .instruction(&Instruction::I32Const(176))
            .instruction(&Instruction::Call(1))
            .instruction(&Instruction::Drop);
    }

    fn clock_subscription(function: &mut Function, base: i32, userdata: i64, timeout_ns: i64) {
        i64_store(function, base, userdata);
        i32_store8(function, base + 8, 0);
        i32_store(function, base + 16, 1);
        i64_store(function, base + 24, timeout_ns);
        i64_store(function, base + 32, 0);
        i32_store16(function, base + 40, 0);
    }

    fn stdin_read_subscription(function: &mut Function, base: i32, userdata: i64) {
        i64_store(function, base, userdata);
        i32_store8(function, base + 8, 1);
        i32_store(function, base + 16, 0);
    }

    fn stdout_write_subscription(function: &mut Function, base: i32, userdata: i64) {
        i64_store(function, base, userdata);
        i32_store8(function, base + 8, 2);
        i32_store(function, base + 16, 1);
    }

    fn poll_oneoff_call(function: &mut Function, nsubscriptions: i32) {
        poll_oneoff_call_expect(function, nsubscriptions, 0);
    }

    fn poll_oneoff_call_expect(function: &mut Function, nsubscriptions: i32, expected_errno: i32) {
        function
            .instruction(&Instruction::I32Const(0))
            .instruction(&Instruction::I32Const(96))
            .instruction(&Instruction::I32Const(nsubscriptions))
            .instruction(&Instruction::I32Const(192))
            .instruction(&Instruction::Call(0))
            .instruction(&Instruction::I32Const(expected_errno))
            .instruction(&Instruction::I32Ne);
        fail_if_nonzero(function);
    }

    let mut types = TypeSection::new();
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );
    types.ty().function(
        [ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        [ValType::I32],
    );
    types.ty().function([ValType::I32], []);
    types.ty().function([], []);

    let mut imports = ImportSection::new();
    imports.import(
        "wasi_snapshot_preview1",
        "poll_oneoff",
        EntityType::Function(0),
    );
    imports.import(
        "wasi_snapshot_preview1",
        "fd_write",
        EntityType::Function(1),
    );
    imports.import(
        "wasi_snapshot_preview1",
        "proc_exit",
        EntityType::Function(2),
    );

    let mut functions = FunctionSection::new();
    functions.function(3).function(3).function(3);

    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("_start", ExportKind::Func, 5);

    let mut code = CodeSection::new();

    let mut ok = Function::new([]);
    fd_write_body(&mut ok, 256, 3);
    ok.instruction(&Instruction::End);
    code.function(&ok);

    let mut fail = Function::new([]);
    fd_write_body(&mut fail, 259, 5);
    fail.instruction(&Instruction::I32Const(1))
        .instruction(&Instruction::Call(2))
        .instruction(&Instruction::Unreachable)
        .instruction(&Instruction::End);
    code.function(&fail);

    let mut start = Function::new([]);
    match case {
        PollOneoffCase::ClockUnsupported => {
            clock_subscription(&mut start, 0, 11, 0);
            poll_oneoff_call_expect(&mut start, 1, WASI_ERRNO_NOTSUP);
        }
        PollOneoffCase::StdinReady => {
            stdin_read_subscription(&mut start, 0, 22);
            poll_oneoff_call(&mut start, 1);
            check_i32_eq(&mut start, 192, 1);
            check_i64_eq(&mut start, 96, 22);
            check_u16_eq(&mut start, 104, 0);
            check_u16_eq(&mut start, 106, 1);
            check_i64_nonzero(&mut start, 112);
        }
        PollOneoffCase::StdinHangup => {
            stdin_read_subscription(&mut start, 0, 33);
            poll_oneoff_call(&mut start, 1);
            check_i32_eq(&mut start, 192, 1);
            check_i64_eq(&mut start, 96, 33);
            check_u16_eq(&mut start, 104, 0);
            check_u16_eq(&mut start, 106, 1);
            check_i32_eq(&mut start, 120, 1);
        }
        PollOneoffCase::StdoutWritable => {
            stdout_write_subscription(&mut start, 0, 44);
            poll_oneoff_call(&mut start, 1);
            check_i32_eq(&mut start, 192, 1);
            check_i64_eq(&mut start, 96, 44);
            check_u16_eq(&mut start, 104, 0);
            check_u16_eq(&mut start, 106, 2);
            check_i64_nonzero(&mut start, 112);
        }
    }
    start
        .instruction(&Instruction::Call(3))
        .instruction(&Instruction::End);
    code.function(&start);

    let mut data = DataSection::new();
    data.active(0, &ConstExpr::i32_const(256), b"ok\nfail\n".to_vec());

    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&memory)
        .section(&exports)
        .section(&code)
        .section(&data);
    module.finish()
}

fn write_poll_oneoff_wasm(case: PollOneoffCase) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(".wasm")
        .tempfile()
        .expect("create wasm tempfile");
    file.write_all(&poll_oneoff_wasm(case))
        .expect("write wasm module");
    file
}

struct TestDir(moon_test_util::test_dir::TestDir);

impl TestDir {
    // create a new TestDir with the test directory in tests/test_cases/<sub>
    fn new(sub: &str) -> Self {
        let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
        Self(moon_test_util::test_dir::TestDir::from_case_root(
            case_root, sub, false,
        ))
    }

    fn join(&self, sub: &str) -> PathBuf {
        self.0.join(sub)
    }
}

impl AsRef<std::path::Path> for TestDir {
    fn as_ref(&self) -> &std::path::Path {
        self.0.as_ref()
    }
}

#[test]
fn test_moonrun_version() {
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg("--version")
        .assert()
        .success()
        .stdout_eq(format!(
            "moonrun {} ({} {})\n",
            env!("CARGO_PKG_VERSION"),
            env!("VERGEN_GIT_SHA"),
            env!("VERGEN_BUILD_DATE")
        ));
}

#[test]
fn test_moonrun_wasm_stack_trace() {
    let dir = TestDir::new("test_stack_trace.in");

    moon_cmd().current_dir(&dir).arg("build").assert().success();

    let main_wasm = dir.join("_build/wasm-gc/debug/build/main/main.wasm");
    let redactions = moon_test_util::stack_trace::stack_trace_redactions(dir.as_ref());
    let assert = snapbox::Assert::new().redact_with(redactions);

    fn moonrun_stack_trace_case(
        main_wasm: &std::path::Path,
        mode: Option<&str>,
        assert: snapbox::Assert,
    ) -> snapbox::cmd::Command {
        let cmd = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .with_assert(assert)
            .arg(main_wasm);
        if let Some(mode) = mode {
            cmd.arg("--").arg(mode)
        } else {
            cmd
        }
    }

    moonrun_stack_trace_case(&main_wasm, None, assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[Unit] [CORE_PATH]/abort/abort.mbt[LINE_NUMBER]
    at @username/hello/main.abort_with_tuple [..]/main/main.mbt[LINE_NUMBER]
    at @username/hello/main.default_abort_chain [..]/main/main.mbt[LINE_NUMBER]
    at @__moonbit_main [..]/main/main.mbt[LINE_NUMBER]

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-generic-int"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[Int] [CORE_PATH]/abort/abort.mbt[LINE_NUMBER]
    at @username/hello/main.abort_generic[Int] [..]/main/main.mbt[LINE_NUMBER]
    at @__moonbit_main [..]/main/main.mbt[LINE_NUMBER]

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-generic-tuple"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[(Int, String)] [CORE_PATH]/abort/abort.mbt[LINE_NUMBER]
    at @username/hello/main.abort_generic[(Int, String)] [..]/main/main.mbt[LINE_NUMBER]
    at @__moonbit_main [..]/main/main.mbt[LINE_NUMBER]

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-method"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[UInt] [CORE_PATH]/abort/abort.mbt[LINE_NUMBER]
    at @username/hello/main.CrashBox::abort_method [..]/main/main.mbt[LINE_NUMBER]
    at @__moonbit_main [..]/main/main.mbt[LINE_NUMBER]

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-closure"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[Int] [CORE_PATH]/abort/abort.mbt[LINE_NUMBER]
    at @username/hello/main.abort_via_closure.inner[stamp=[..]] [..]/main/main.mbt[LINE_NUMBER]
    at @username/hello/main.abort_via_closure [..]/main/main.mbt[LINE_NUMBER]
    at @__moonbit_main [..]/main/main.mbt[LINE_NUMBER]

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("panic-result"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @username/hello/main.panic_with_result [..]/main/main.mbt[LINE_NUMBER]
    at @__moonbit_main [..]/main/main.mbt[LINE_NUMBER]

"#]]);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&main_wasm)
        .arg("--no-stack-trace")
        .assert()
        .failure()
        .stderr_eq("RuntimeError: unreachable\n");
}

#[test]
fn test_moonrun_wasm_stack_trace_in_test_blocks() {
    let dir = TestDir::new("test_stack_trace.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["test", "--target", "wasm-gc", "--build-only"])
        .assert()
        .success();

    fn moon_test_case(dir: &TestDir, args: &[&str]) -> snapbox::cmd::Command {
        moon_cmd()
            .current_dir(dir)
            .arg("test")
            .arg("--target")
            .arg("wasm-gc")
            .args(args)
    }
    let redactions = moon_test_util::stack_trace::stack_trace_redactions(dir.as_ref());
    let assert = snapbox::Assert::new().redact_with(redactions);

    moon_test_case(&dir, &["--filter", "stacktrace test abort closure"])
        .with_assert(assert.clone())
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
[username/hello] test main/main.mbt:[..] ("stacktrace test abort closure") failed: Error
    at throw
    at @moonbitlang/core/abort.abort[Int] [..]/abort/abort.mbt[LINE_NUMBER]
    at @username/hello/main.abort_via_closure.inner[stamp=[..]] [..]/main/main.mbt[LINE_NUMBER]
    at @username/hello/main.abort_via_closure [..]/main/main.mbt[LINE_NUMBER]
    at @username/hello/main.__test_6d61696e2e6d6274_2 [..]/main/main.mbt[LINE_NUMBER]
    at @username/hello/main.__test_6d61696e2e6d6274_2.dyncall
    at @username/hello/main.moonbit_test_driver_internal_catch_error [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER]
    at impl @username/hello/main.MoonBit_Test_Driver for @username/hello/main.MoonBit_Test_Driver_Internal_No_Args with run_test [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER]
    at @username/hello/main.moonbit_test_driver_internal_do_execute [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER]
    at @username/hello/main.moonbit_test_driver_internal_execute [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER]
Total tests: 1, passed: 0, failed: 1.

"#]]);

    moon_test_case(&dir, &["main/main.mbt", "--index", "1"])
        .with_assert(assert.clone())
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
[username/hello] test main/main.mbt:[..] ("stacktrace test abort method") failed: Error
    at throw
    at @moonbitlang/core/abort.abort[UInt] [..]/abort/abort.mbt[LINE_NUMBER]
    at @username/hello/main.CrashBox::abort_method [..]/main/main.mbt[LINE_NUMBER]
    at @username/hello/main.__test_6d61696e2e6d6274_1 [..]/main/main.mbt[LINE_NUMBER]
    at @username/hello/main.__test_6d61696e2e6d6274_1.dyncall
    at @username/hello/main.moonbit_test_driver_internal_catch_error [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER]
    at impl @username/hello/main.MoonBit_Test_Driver for @username/hello/main.MoonBit_Test_Driver_Internal_No_Args with run_test [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER]
    at @username/hello/main.moonbit_test_driver_internal_do_execute [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER]
    at @username/hello/main.moonbit_test_driver_internal_execute [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER]
    at @username/hello/main.moonbit_test_driver_internal_execute_wrapper/[..] [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER]
Total tests: 1, passed: 0, failed: 1.

"#]]);
}

#[test]
fn test_moon_run_with_cli_args() {
    let dir = TestDir::new("test_cli_args.in");

    moon_cmd().current_dir(&dir).arg("build").assert().success();

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");
    // `argv` passed to CLI is:
    // <wasm_file> <...rest argv to moonrun>

    // Assert it has the WASM file as argv[0]
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("[\"[..]/_build/wasm-gc/debug/build/main/main.wasm\"]\n");

    // Assert it passes the rest verbatim
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .arg("--")
        .args(["中文", "😄👍", "hello", "1242"])
        .assert()
        .success()
        .stdout_eq(
            "[\n  \"[..]/_build/wasm-gc/debug/build/main/main.wasm\",\n  \"中文\",\n  \"😄👍\",\n  \"hello\",\n  \"1242\",\n]\n",
        );

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .arg("--no-stack-trace") // this ia an arg accepted by moonrun
        .arg("--")
        .args(["--arg1", "--arg2", "arg3"])
        .assert()
        .success()
        .stdout_eq(
            "[\n  \"[..]/_build/wasm-gc/debug/build/main/main.wasm\",\n  \"--arg1\",\n  \"--arg2\",\n  \"arg3\",\n]\n",
        );
}

#[test]
fn test_moon_run_with_read_bytes_from_stdin() {
    let dir = TestDir::new("test_read_bytes.in");

    moon_cmd().current_dir(&dir).arg("build").assert().success();

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .stdin("中文😄👍hello1242")
        .assert()
        .success()
        .stdout_eq(format!("{}\n", "中文😄👍hello1242".len()));

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .stdin("")
        .assert()
        .success()
        .stdout_eq("0\n");
}

#[test]
fn test_moonrun_wasi_poll_oneoff_clock_unsupported() {
    let wasm_file = write_poll_oneoff_wasm(PollOneoffCase::ClockUnsupported);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(wasm_file.path())
        .assert()
        .success()
        .stdout_eq("ok\n");
}

#[test]
fn test_moonrun_wasi_poll_oneoff_stdin_ready() {
    let wasm_file = write_poll_oneoff_wasm(PollOneoffCase::StdinReady);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(wasm_file.path())
        .stdin("x")
        .assert()
        .success()
        .stdout_eq("ok\n");
}

#[test]
fn test_moonrun_wasi_poll_oneoff_stdin_hangup() {
    let wasm_file = write_poll_oneoff_wasm(PollOneoffCase::StdinHangup);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(wasm_file.path())
        .stdin("")
        .assert()
        .success()
        .stdout_eq("ok\n");
}

#[test]
fn test_moonrun_wasi_poll_oneoff_stdout_writable() {
    let wasm_file = write_poll_oneoff_wasm(PollOneoffCase::StdoutWritable);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(wasm_file.path())
        .assert()
        .success()
        .stdout_eq("ok\n");
}

fn assert_poll_oneoff_waits_for_stdin_hangup(
    configure_command: impl FnOnce(&mut std::process::Command),
) {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let wasm_file = write_poll_oneoff_wasm(PollOneoffCase::StdinHangup);
    let mut command = Command::new(snapbox::cmd::cargo_bin!("moonrun"));
    command
        .arg(wasm_file.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_command(&mut command);

    let mut child = command.spawn().expect("spawn moonrun");
    let child_stdin = child.stdin.take().expect("hold child stdin open");

    let start = Instant::now();
    while start.elapsed() <= Duration::from_millis(250) {
        if child.try_wait().expect("poll moonrun").is_some() {
            let output = child.wait_with_output().expect("wait for moonrun output");
            panic!(
                "moonrun exited before stdin hangup\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    drop(child_stdin);

    let start = Instant::now();
    loop {
        if child.try_wait().expect("poll moonrun").is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            panic!("moonrun did not exit after stdin hangup");
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let output = child.wait_with_output().expect("wait for moonrun output");
    assert!(
        output.status.success(),
        "moonrun failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"ok\n");
}

#[test]
fn test_moonrun_wasi_poll_oneoff_blocking_stdin_waits_for_hangup() {
    assert_poll_oneoff_waits_for_stdin_hangup(|_| {});
}

#[cfg(unix)]
#[test]
fn test_moonrun_wasi_poll_oneoff_nonblocking_stdin_waits_for_hangup() {
    use std::os::unix::process::CommandExt;

    assert_poll_oneoff_waits_for_stdin_hangup(|command| {
        // SAFETY: The closure only changes fd 0 to nonblocking in the child after fork and before exec.
        unsafe {
            command.pre_exec(|| {
                rustix::io::ioctl_fionbio(rustix::stdio::stdin(), true)
                    .map_err(std::io::Error::from)
            });
        }
    });
}

#[test]
fn test_moon_run_with_is_windows() {
    let dir = TestDir::new("test_os_platform_detection");

    moon_cmd().current_dir(&dir).arg("build").assert().success();

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq(if std::env::consts::OS == "windows" {
            "1\n"
        } else {
            "0\n"
        });
}

#[test]
fn test_moon_fmt_skips_prebuild_output() {
    // Prepare a temp copy of the test case
    let dir = TestDir::new("test_fmt_skip_prebuild_output");

    // The prebuild command is a NOOP; we intentionally wrote a sloppy file as the "generated" output.
    // Ensure the source remains sloppy after fmt (formatter must skip prebuild outputs).
    let generated_src = dir.join("main/generated.mbt");
    let original = std::fs::read_to_string(&generated_src).expect("read generated.mbt");

    // Run: moon fmt
    moon_cmd()
        .current_dir(&dir)
        .args(["fmt"])
        .assert()
        .success();

    let after = std::fs::read_to_string(&generated_src).expect("read generated.mbt");
    assert_eq!(original, after, "Formatter should skip prebuild outputs");
}
