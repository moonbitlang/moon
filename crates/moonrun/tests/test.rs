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

use std::{net::Ipv4Addr, path::PathBuf, sync::OnceLock, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MOONBIT_ASYNC_CHECK_FD_LEAK: &str = "MOONBIT_ASYNC_CHECK_FD_LEAK";

fn moon_cmd() -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(moon_bin())
        .env("MOONRUN_OVERRIDE", snapbox::cmd::cargo_bin!("moonrun"))
}

fn moon_bin() -> &'static PathBuf {
    static MOON_BIN: OnceLock<PathBuf> = OnceLock::new();
    MOON_BIN.get_or_init(|| {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../moon/Cargo.toml");
        escargot::CargoBuild::new()
            .manifest_path(manifest_path)
            .bin("moon")
            .current_release()
            .current_target()
            .run()
            .expect("failed to build moon")
            .path()
            .to_owned()
    })
}

#[test]
#[ignore = "run in CI when Moonrun or upstream async changes"]
fn test_moonrun_against_upstream_async() {
    let async_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .join("third_party/moonbitlang_async");
    let build_dir = async_dir.join("_build");
    std::fs::create_dir_all(&build_dir).expect("failed to create async test build dir");
    let target_dir = tempfile::Builder::new()
        .prefix("moon-test-target-")
        .tempdir_in(&build_dir)
        .expect("failed to create async test target dir");
    let moon = moon_bin();
    let path = std::env::join_paths(
        std::iter::once(
            moon.parent()
                .expect("test moon binary should have a parent directory")
                .to_path_buf(),
        )
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .expect("failed to put the test moon binary on PATH");

    moon_cmd()
        .current_dir(&async_dir)
        .env("MOON_OVERRIDE", moon)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .env("PATH", path)
        .arg("--target-dir")
        .arg(target_dir.path())
        .args(["test", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq("Total tests: 452, passed: 452, failed: 0.\n");
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

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

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
fn test_moonrun_help_describes_policy_shape() {
    let assert = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg("--help")
        .assert()
        .success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    assert!(stdout.contains("Experimental: Sandbox wasm runtime host access"));
    assert!(stdout.contains("JSON policy file"));
    assert!(!stdout.contains("TOML"));
    assert!(stdout.contains("deny-by-default mode"));
    assert!(stdout.contains(r#""from_host": ["*"]"#));
    assert!(stdout.contains(r#""read": ["*"]"#));
    assert!(stdout.contains(r#""spawn": true"#));
    assert!(stdout.contains(r#"net.connect containing "api.deepseek.com:443""#));
    assert!(stdout.contains("Hostname connect rules also permit DNS lookup"));
    assert!(stdout.contains("process.allow entries match the exact requested program"));
    assert!(stdout.contains("Omitting args_prefix allows any arguments"));
    assert!(stdout.contains("logical request, not the executable eventually selected"));
    assert!(stdout.contains("ambient filesystem, network, and process access"));
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

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

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
fn test_moonrun_exits_with_guest_exit_code() {
    let dir = TestDir::new("test_cli_args.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(dir.join("_build/wasm-gc/debug/build/main/main.wasm"))
        .arg("--")
        .arg("exit-7")
        .assert()
        .code(7)
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn moonrun_library_returns_guest_exit_without_terminating_embedder() {
    let dir = TestDir::new("test_cli_args.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

    let wasm = dir.join("_build/wasm-gc/debug/build/main/main.wasm");
    let engine = moonrun::Engine::default();
    let options = || {
        moonrun::RunOptions::default()
            .with_args(["exit-7"])
            .with_working_directory(moonrun::WorkingDirectory::Ambient)
    };

    assert_eq!(
        engine.run_file(&wasm, options()).unwrap(),
        moonrun::RunOutcome::Exited(7)
    );
    assert_eq!(
        engine.run_file(&wasm, options()).unwrap(),
        moonrun::RunOutcome::Exited(7)
    );
}

#[test]
fn moonrun_observes_each_process_working_directory() {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let case_dir = case_root.join("test_working_directory_instances.in");
    let dir = tempfile::Builder::new()
        .prefix("test_working_directory_instances.")
        .tempdir_in(&case_root)
        .expect("create temp fixture");
    moon_test_util::test_dir::copy_tree(&case_dir, dir.path(), false).expect("copy test fixture");

    moon_cmd()
        .current_dir(dir.path())
        .args(["test", "main", "--target", "wasm", "--build-only"])
        .assert()
        .success();

    let wasm = dir.path().join(
        "_build/wasm/debug/test/moon/working_directory_instances/main/main.blackbox_test.wasm",
    );
    for marker in ["first", "second"] {
        let instance = dir.path().join(marker);
        std::fs::create_dir(&instance).unwrap();
        std::fs::write(instance.join("input.txt"), marker).unwrap();

        snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .current_dir(&instance)
            .arg("--test-args")
            .arg(
                r#"{"package":"moon/working_directory_instances/main","file_and_index":[["main_test.mbt",[{"start":0,"end":1}]]]}"#,
            )
            .arg(&wasm)
            .assert()
            .success()
            .stdout_eq(snapbox::str![[r#"
----- BEGIN MOON TEST RESULT -----
{"type":"start","file":"main_test.mbt","index":0}
----- END MOON TEST RESULT -----
----- BEGIN MOON TEST RESULT -----
{"type":"result","file":"main_test.mbt","index":0,"message":""}
----- END MOON TEST RESULT -----

"#]])
            .stderr_eq("");

        assert_eq!(
            std::fs::read_to_string(instance.join("output.txt")).unwrap(),
            marker
        );
        assert_eq!(
            std::fs::canonicalize(PathBuf::from(
                std::fs::read_to_string(instance.join("cwd.txt")).unwrap(),
            ))
            .unwrap(),
            std::fs::canonicalize(instance).unwrap(),
        );
    }
}

#[test]
fn moonrun_library_compiles_modules_when_loading() {
    let wasm = tempfile::Builder::new()
        .prefix("invalid.")
        .suffix(".wasm")
        .tempfile()
        .unwrap();
    std::fs::write(wasm.path(), b"not WebAssembly").unwrap();

    let error = moonrun::Engine::default()
        .load_file(wasm.path())
        .unwrap_err();

    assert!(format!("{error:#}").contains("failed to compile"));
}

#[tokio::test(flavor = "multi_thread")]
async fn moonrun_library_runs_multiple_modules_with_caller_owned_scheduling() {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let case_dir = case_root.join("test_concurrent_http_instances.in");
    let dir = tempfile::Builder::new()
        .prefix("test_concurrent_http_instances.")
        .tempdir_in(&case_root)
        .expect("create temp fixture");
    moon_test_util::test_dir::copy_tree(&case_dir, dir.path(), false).expect("copy test fixture");

    moon_cmd()
        .current_dir(dir.path())
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let first_readiness = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let second_readiness = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let first_readiness_port = first_readiness.local_addr().unwrap().port();
    let second_readiness_port = second_readiness.local_addr().unwrap().port();

    let wasm_path = dir
        .path()
        .join("_build/wasm/debug/build/moon/concurrent_http_instances/main/main.wasm");
    let wasm = std::fs::read(&wasm_path).unwrap();
    std::fs::remove_file(&wasm_path).unwrap();
    let engine = moonrun::Engine::default();
    let first_module = engine.compile("first-http-module", &wasm).unwrap();
    let second_module = engine.compile("second-http-module", &wasm).unwrap();
    let first_engine = engine.clone();
    let first_run = tokio::task::spawn_blocking(move || {
        first_engine.run(
            &first_module,
            moonrun::RunOptions::default()
                .with_args([first_readiness_port.to_string(), "first".to_owned()]),
        )
    });
    let second_run = tokio::task::spawn_blocking(move || {
        engine.run(
            &second_module,
            moonrun::RunOptions::default()
                .with_args([second_readiness_port.to_string(), "second".to_owned()]),
        )
    });

    let (first_port, second_port) = tokio::join!(
        wait_for_guest_port(&first_readiness, "first module"),
        wait_for_guest_port(&second_readiness, "second module")
    );
    let (first_response, second_response) = tokio::join!(request(first_port), request(second_port));
    assert!(first_response.starts_with("HTTP/1.1 200"));
    assert!(first_response.contains("first"));
    assert!(second_response.starts_with("HTTP/1.1 200"));
    assert!(second_response.contains("second"));

    assert_eq!(
        first_run.await.unwrap().unwrap(),
        moonrun::RunOutcome::Completed
    );
    assert_eq!(
        second_run.await.unwrap().unwrap(),
        moonrun::RunOutcome::Completed
    );
}

async fn wait_for_guest_port(listener: &TcpListener, module: &str) -> u16 {
    tokio::time::timeout(Duration::from_secs(10), async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut port = String::new();
        stream.read_to_string(&mut port).await.unwrap();
        port.parse().unwrap()
    })
    .await
    .unwrap_or_else(|_| panic!("{module} did not report readiness"))
}

async fn request(port: u16) -> String {
    tokio::time::timeout(Duration::from_secs(5), async move {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    })
    .await
    .unwrap_or_else(|_| panic!("HTTP instance on port {port} did not respond"))
}

#[test]
fn test_moonrun_async_host_exit_returns_guest_exit_code() {
    let dir = TestDir::new("test_async_exit.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(dir.join("_build/wasm/debug/build/main/main.wasm"))
        .assert()
        .code(9)
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn test_moonrun_async_host_signal_termination_is_scoped_to_wasm_run() {
    let dir = TestDir::new("test_async_signal_termination.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let output = std::process::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(dir.join("_build/wasm/debug/build/main/main.wasm"))
        .output()
        .expect("run wasm signal termination fixture");

    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(output.status.signal(), Some(libc::SIGINT));
    }
    #[cfg(windows)]
    assert_eq!(output.status.code(), Some(0xC000_013A_u32 as i32));
}

#[test]
fn test_moonrun_async_main_preserves_signal_termination() {
    use std::io::{BufRead, Read};
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    use std::sync::mpsc::{RecvTimeoutError, sync_channel};
    use std::time::{Duration, Instant};

    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let case_dir = case_root.join("test_async_signal_main.in");
    let dir = tempfile::Builder::new()
        .prefix("test_async_signal_main.")
        .tempdir_in(&case_root)
        .expect("create temp fixture");
    moon_test_util::test_dir::copy_tree(&case_dir, dir.path(), false).expect("copy test fixture");

    moon_cmd()
        .current_dir(dir.path())
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir
        .path()
        .join("_build/wasm/debug/build/moon/async_signal_main/main/main.wasm");
    let mut command = std::process::Command::new(snapbox::cmd::cargo_bin!("moonrun"));
    command
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg(wasm_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP);
    let mut child = command.spawn().expect("start async wasm main");
    let deadline = Instant::now() + Duration::from_secs(10);
    let (ready_tx, ready_rx) = sync_channel(1);
    let stdout_thread = std::thread::spawn({
        let stdout = child.stdout.take().expect("capture stdout");
        move || {
            let mut stdout = std::io::BufReader::new(stdout);
            let mut ready = String::new();
            let result = stdout.read_line(&mut ready);
            let _ = ready_tx.send((result, ready));
            stdout
        }
    });
    let ready = match ready_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok((Ok(_), ready)) => ready,
        Ok((Err(error), _)) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            panic!("failed to read async wasm main readiness: {error}");
        }
        Err(RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            panic!("async wasm main did not become ready before timeout");
        }
        Err(RecvTimeoutError::Disconnected) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            panic!("async wasm main readiness reader stopped unexpectedly");
        }
    };
    let mut stdout = stdout_thread.join().expect("join readiness reader");
    if ready != "ready\n" {
        let _ = child.kill();
        let _ = child.wait();
        panic!("unexpected async wasm main readiness output: {ready:?}");
    }

    #[cfg(unix)]
    assert_eq!(
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) },
        0
    );
    #[cfg(windows)]
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent(
                windows_sys::Win32::System::Console::CTRL_BREAK_EVENT,
                child.id(),
            )
        },
        0
    );
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for async wasm main") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("async wasm main did not terminate after cancellation signal");
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    let mut remaining_stdout = String::new();
    stdout
        .read_to_string(&mut remaining_stdout)
        .expect("read remaining stdout");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("capture stderr")
        .read_to_string(&mut stderr)
        .expect("read stderr");
    assert_eq!(remaining_stdout, "");
    assert_eq!(stderr, "");
    #[cfg(unix)]
    assert_eq!(status.signal(), Some(libc::SIGINT));
    #[cfg(windows)]
    assert_eq!(status.code(), Some(0xC000_013A_u32 as i32));
}

#[test]
fn test_moon_run_with_read_bytes_from_stdin() {
    let dir = TestDir::new("test_read_bytes.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

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
fn test_moon_run_with_is_windows() {
    let dir = TestDir::new("test_os_platform_detection");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

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
fn test_moon_run_with_async_host_imports() {
    let dir = TestDir::new("test_async_host.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("ok\n");
}

#[test]
fn test_moon_run_with_sqlite_ffi_imports() {
    let dir = TestDir::new("test_sqlite_ffi.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");
    std::fs::create_dir(dir.join("allowed")).unwrap();
    std::fs::create_dir(dir.join("denied")).unwrap();
    let policy_file = dir.join("policy.toml");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&dir)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg("--policy")
        .arg(&policy_file)
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("ok\n")
        .stderr_eq(snapbox::str![[r#"
Sandbox policy blocked file read: "denied/database.sqlite"

"#]]);

    assert!(dir.join("allowed/database.sqlite").exists());
    assert!(!dir.join("denied/database.sqlite").exists());

    let leaked_wasm = dir.join("_build/wasm/debug/build/leak/leak.wasm");
    let assert = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&dir)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .env("RUST_BACKTRACE", "0")
        .arg(&leaked_wasm)
        .assert()
        .failure()
        .stdout_eq("leaked\n");
    let stderr = std::str::from_utf8(&assert.get_output().stderr).unwrap();
    assert!(
        stderr.contains("moonrun Runtime leaked host state: sqlite(databases=1)"),
        "expected SQLite leak assertion in stderr, got:\n{stderr}"
    );

    let invalid_handle_wasm =
        dir.join("_build/wasm/debug/build/invalid_handle/invalid_handle.wasm");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&invalid_handle_wasm)
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: moonbitlang/sqlite.sqlite3_close failed: InvalidHandle
[..]
"#]]);
}

#[test]
fn test_sqlite_binding_order() {
    let dir = TestDir::new("test_sqlite_ffi.in");

    moon_cmd()
        .current_dir(&dir)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .args(["test", "main", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq("Total tests: 3, passed: 3, failed: 0.\n");
}

#[test]
fn test_moon_run_with_async_stdio_imports() {
    let dir = TestDir::new("test_async_stdio.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg(&wasm_file)
        .stdin("stdio input\n")
        .assert()
        .success()
        .stdout_eq("stdout-ok\n")
        .stderr_eq("stderr-ok\n");
}

#[test]
fn test_moon_run_with_memory_sanitizer_imports() {
    let dir = TestDir::new("test_memory_sanitizer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("ok\n");
}

#[test]
fn test_moon_run_memory_sanitizer_reports_demangled_alloc_stack() {
    let dir = TestDir::new("test_memory_sanitizer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/duplicate_alloc/duplicate_alloc.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: moonbit:ffi/memory-sanitizer.register-object-alloc failed: object 2048 is already live with size 16
previous allocation stack:
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.host_register_object_alloc
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.register_object_alloc
    at @moonbit/ffi-memory-sanitizer-test/duplicate_alloc.allocate_once
    at @__moonbit_main
    at <anonymous>
...
"#]]);
}

#[test]
fn test_moon_run_memory_sanitizer_reports_double_free() {
    let dir = TestDir::new("test_memory_sanitizer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/double_free/double_free.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: moonbit:ffi/memory-sanitizer.register-object-free failed: invalid object 4096
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.host_register_object_free
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.register_object_free
    at @moonbit/ffi-memory-sanitizer-test/double_free.free_twice
    at @__moonbit_main
...
"#]]);
}

#[test]
fn test_moon_run_memory_sanitizer_reports_leaks() {
    let dir = TestDir::new("test_memory_sanitizer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/leak/leak.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: moonrun memory sanitizer detected 1 leaked object (16 bytes)
leaked object 8192 (16 bytes)
allocation stack:
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.host_register_object_alloc
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.register_object_alloc
    at @moonbit/ffi-memory-sanitizer-test/leak.leak_object
    at @__moonbit_main
    at <anonymous>
...
"#]]);
}

#[test]
fn test_moon_run_policy_with_workspace_async_fs() {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let case_dir = case_root.join("test_policy_workspace.in");
    let dir = tempfile::Builder::new()
        .prefix("test_policy_workspace.")
        .tempdir_in(&case_root)
        .expect("create temp fixture");
    moon_test_util::test_dir::copy_tree(&case_dir, dir.path(), false).expect("copy test fixture");

    moon_cmd()
        .current_dir(dir.path())
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = |package: &str| {
        std::fs::canonicalize(dir.path().join(format!(
            "_build/wasm/debug/build/moon/policy_workspace/{package}/{package}.wasm"
        )))
        .unwrap()
    };
    let fs_allow_wasm = wasm_file("fs_allow");
    let fs_deny_read_wasm = wasm_file("fs_deny_read");
    let env_get_wasm = wasm_file("env_get");
    let env_mutate_wasm = wasm_file("env_mutate");
    let listen_implicit_bind_wasm = wasm_file("listen_implicit_bind");
    let process_env_wasm = wasm_file("process_env");
    let policy_file = std::fs::canonicalize(dir.path().join("policy.toml")).unwrap();
    let deny_all_policy_file = std::fs::canonicalize(dir.path().join("deny-all.toml")).unwrap();
    let process_any_args_policy_file =
        std::fs::canonicalize(dir.path().join("process-any-args.toml")).unwrap();
    let process_prefix_deny_policy_file =
        std::fs::canonicalize(dir.path().join("process-prefix-deny.toml")).unwrap();

    #[cfg(not(windows))]
    let fs_deny_read_stdout = snapbox::str![[r#"
OSError("[..]@fs.open()[..]denied/secret.txt[..]Permission denied")

"#]];
    #[cfg(windows)]
    let fs_deny_read_stdout = snapbox::str![[r#"
OSError("[..]@fs.open()[..]denied/secret.txt[..]Access is denied.")

"#]];
    #[cfg(not(windows))]
    let env_mutate_stdout = "runtime override\nmissing\n/first/\n/second/\n/tmp/\n";
    #[cfg(windows)]
    let env_mutate_stdout = "runtime override\nmissing\nC:/First\\\nC:/Second\\\nmissing\n";

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg("--policy")
        .arg(&policy_file)
        .arg(&fs_allow_wasm)
        .assert()
        .success()
        .stdout_eq("workspace input\n|workspace sandbox policy\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .arg("--policy")
        .arg(&policy_file)
        .arg(&env_get_wasm)
        .assert()
        .success()
        .stdout_eq("configured by policy\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .arg("--policy")
        .arg(&policy_file)
        .arg(&env_mutate_wasm)
        .assert()
        .success()
        .stdout_eq(snapbox::Data::text(env_mutate_stdout).raw());

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env("MOONRUN_HIDDEN_ENV", "host secret")
        .arg("--policy")
        .arg(&policy_file)
        .arg(&process_env_wasm)
        .assert()
        .success()
        .stdout_eq(if cfg!(windows) {
            "MOONRUN_POLICY_ENV=configured by policy\n"
        } else {
            "configured by policy|\n"
        });

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env("MOONRUN_HIDDEN_ENV", "host secret")
        .arg("--policy")
        .arg(&process_any_args_policy_file)
        .arg(&process_env_wasm)
        .assert()
        .success()
        .stdout_eq(if cfg!(windows) {
            "MOONRUN_POLICY_ENV=configured by policy\n"
        } else {
            "configured by policy|\n"
        });

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env("MOONRUN_POLICY_ENV", "host value")
        .arg("--policy")
        .arg(&deny_all_policy_file)
        .arg(&env_get_wasm)
        .assert()
        .success()
        .stdout_eq("missing\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .arg("--policy")
        .arg(&process_prefix_deny_policy_file)
        .arg(&process_env_wasm)
        .assert()
        .success()
        .stdout_eq("spawn denied\n")
        .stderr_eq("Sandbox policy blocked process spawn\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg("--policy")
        .arg(&deny_all_policy_file)
        .arg(&listen_implicit_bind_wasm)
        .assert()
        .success()
        .stdout_eq("listen denied\n")
        .stderr_eq(snapbox::str![[r#"
Sandbox policy blocked network bind: "0.0.0.0:0"

"#]]);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .arg("--policy")
        .arg(&deny_all_policy_file)
        .arg(&process_env_wasm)
        .assert()
        .success()
        .stdout_eq("spawn denied\n")
        .stderr_eq("Sandbox policy blocked process spawn\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg("--policy")
        .arg(&policy_file)
        .arg(&fs_deny_read_wasm)
        .assert()
        .failure()
        .stdout_eq(fs_deny_read_stdout)
        .stderr_eq(snapbox::str![[r#"
Sandbox policy blocked file read: "denied/secret.txt"

"#]]);
}

#[test]
fn test_moon_run_async_host_leak_check_env() {
    let dir = TestDir::new("test_async_host_leak_check.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env_remove(MOONBIT_ASYNC_CHECK_FD_LEAK)
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("leaked\n")
        .stderr_eq("");

    let assert = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .env("RUST_BACKTRACE", "0")
        .arg(&wasm_file)
        .assert()
        .failure()
        .stdout_eq("leaked\n");
    let stderr = std::str::from_utf8(&assert.get_output().stderr).unwrap();
    assert!(
        stderr.contains("moonrun Runtime leaked host state: async(polls=1"),
        "expected async host leak assertion in stderr, got:\n{stderr}"
    );
}

#[test]
fn test_moon_run_with_async_host_invalid_c_buffer_traps() {
    let dir = TestDir::new("test_async_host_invalid_c_buffer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: moonbitlang/async.c_buffer/c_buffer_get failed: Badf
[..]
"#]]);
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
