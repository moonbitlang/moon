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

use crate::{TestDir, build_graph, get_stdout_with_envs, moon_cmd};

use super::*;

#[test]
fn test_moon_coverage_report_enabled() {
    let dir = TestDir::new("test_coverage.in");
    for value in ["1", "true", "TRUE"] {
        moon_cmd(&dir)
            .env("MOON_COVE_REPORT_ENABLED", value)
            .env_remove("MOON_COVE_REPORT_OVERRIDE")
            .args(["coverage", "report", "--dry-run", "-f=summary"])
            .assert()
            .success()
            .stdout_eq("(cd [..] && [..]moonrun[..] [..]/bin/moon_cove.wasm -- -f=summary)\n");
    }
}

#[test]
fn test_moon_coverage_report_enabled_with_override() {
    let dir = TestDir::new("test_coverage.in");
    for value in ["1", "true", "TRUE"] {
        moon_cmd(&dir)
            .env("MOON_COVE_REPORT_ENABLED", value)
            .env("MOON_COVE_REPORT_OVERRIDE", "unused-legacy-reporter")
            .args(["coverage", "report", "--dry-run", "-f=summary"])
            .assert()
            .success()
            .stdout_eq("(cd [..] && [..]moonrun[..] [..]/bin/moon_cove.wasm -- -f=summary)\n");
    }
}

#[test]
fn test_moon_coverage_report_help_dry_run() {
    let dir = TestDir::new_empty();
    moon_cmd(&dir)
        .env("MOON_COVE_REPORT_ENABLED", "1")
        .env_remove("MOON_COVE_REPORT_OVERRIDE")
        .args(["coverage", "report", "--dry-run", "--help"])
        .assert()
        .success()
        .stdout_eq("(cd [..] && [..]moonrun[..] [..]/bin/moon_cove.wasm -- --help)\n");
}

#[test]
fn test_moon_coverage_report_default() {
    let dir = TestDir::new("test_coverage.in");
    moon_cmd(&dir)
        .env_remove("MOON_COVE_REPORT_ENABLED")
        .env_remove("MOON_COVE_REPORT_OVERRIDE")
        .args(["coverage", "report", "--dry-run"])
        .assert()
        .success()
        .stdout_eq("(cd [..] && [..]moonrun[..] [..]/bin/moon_cove.wasm --)\n");
}

#[test]
fn test_moon_coverage_report_default_with_override() {
    let dir = TestDir::new("test_coverage.in");
    moon_cmd(&dir)
        .env_remove("MOON_COVE_REPORT_ENABLED")
        .env("MOON_COVE_REPORT_OVERRIDE", "unused-legacy-reporter")
        .args(["coverage", "report", "--dry-run"])
        .assert()
        .success()
        .stdout_eq("(cd [..] && [..]moonrun[..] [..]/bin/moon_cove.wasm --)\n");
}

#[test]
fn test_moon_coverage_report_disabled() {
    let dir = TestDir::new("test_coverage.in");
    for value in ["0", "false", "FALSE"] {
        moon_cmd(&dir)
            .env("MOON_COVE_REPORT_ENABLED", value)
            .env_remove("MOON_COVE_REPORT_OVERRIDE")
            .args(["coverage", "report", "--dry-run", "-f=summary"])
            .assert()
            .success()
            .stdout_eq("(cd [..] && [..]moon_cove_report[..] -f=summary)\n");
    }
}

#[test]
fn test_moon_coverage_report_disabled_with_override() {
    let dir = TestDir::new("test_coverage.in");
    for value in ["0", "false", "FALSE"] {
        moon_cmd(&dir)
            .env("MOON_COVE_REPORT_ENABLED", value)
            .env("MOON_COVE_REPORT_OVERRIDE", "custom-moon-cove-report")
            .args(["coverage", "report", "--dry-run", "-f=summary"])
            .assert()
            .success()
            .stdout_eq("(cd [..] && custom-moon-cove-report -f=summary)\n");
    }
}

#[test]
fn test_moon_coverage_analyze_exit_status() {
    for (reporter, report_exit_code) in [("0", 2), ("1", 1)] {
        let dir = TestDir::new("test_coverage.in");
        for test_exit_code in [0, 2] {
            if test_exit_code != 0 {
                std::fs::write(
                    dir.join("lib/fail_test.mbt"),
                    "test \"failure\" { assert_eq(1, 2) }\n",
                )
                .unwrap();
            }

            moon_cmd(&dir)
                .env("MOON_COVE_REPORT_ENABLED", reporter)
                .env_remove("MOON_COVE_REPORT_OVERRIDE")
                .args([
                    "coverage",
                    "analyze",
                    "--test-flag=--target=wasm-gc",
                    "--",
                    "-f=invalid-format",
                ])
                .assert()
                .code(report_exit_code)
                .stderr_eq(if reporter == "0" {
                    "...\nFatal error: exception Failure(\"Unknown output format: invalid-format\")\n...\n"
                } else {
                    "...\nerror: unknown output format 'invalid-format'\n"
                });

            moon_cmd(&dir)
                .env("MOON_COVE_REPORT_ENABLED", reporter)
                .env_remove("MOON_COVE_REPORT_OVERRIDE")
                .args([
                    "coverage",
                    "analyze",
                    "--trace",
                    "--test-flag=--target=wasm-gc",
                    "--",
                    "-f=coveralls",
                    "-o=coverage.json",
                ])
                .assert()
                .code(test_exit_code)
                .stdout_eq(if test_exit_code == 0 {
                    ""
                } else {
                    "...\nTotal tests: 4, passed: 3, failed: 1.\n"
                });
            let report = std::fs::read_to_string(dir.join("coverage.json")).unwrap();
            assert!(!report.is_empty(), "reporter did not generate coverage");
            std::fs::remove_file(dir.join("coverage.json")).unwrap();
            let trace = std::fs::read_to_string(dir.join("trace.json")).unwrap();
            serde_json::from_str::<serde_json::Value>(&trace).unwrap();
        }
    }
}

#[cfg(unix)]
#[test]
fn test_moon_coverage_report_preserves_signal_dispositions() {
    use std::io::Write;
    use std::os::unix::{fs::PermissionsExt, process::CommandExt, process::ExitStatusExt};
    use std::time::{Duration, Instant};

    for analyze in [false, true] {
        let dir = TestDir::new("test_coverage.in");
        if analyze {
            std::fs::write(
                dir.join("lib/fail_test.mbt"),
                "test \"failure\" { assert_eq(1, 2) }\n",
            )
            .unwrap();
        }
        let reporter = dir.join("reporter");
        std::fs::write(
            &reporter,
            "#!/bin/sh\nprintf ready > reporter-ready\nread -r finish\nprintf done > reporter-done\n",
        )
        .unwrap();
        std::fs::set_permissions(&reporter, std::fs::Permissions::from_mode(0o755)).unwrap();

        for (ignored, group) in [(true, false), (false, false), (true, true), (false, true)] {
            for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
                // Avoid producing core dumps while still covering inherited SIGQUIT ignores.
                if signal == libc::SIGQUIT && !ignored {
                    continue;
                }
                let ready = dir.join("reporter-ready");
                let done = dir.join("reporter-done");
                let mut command = crate::moon_process_cmd(&dir);
                command
                    .args([
                        "coverage",
                        if analyze { "analyze" } else { "report" },
                        "--trace",
                    ])
                    .env("MOON_COVE_REPORT_ENABLED", "0")
                    .env("MOON_COVE_REPORT_OVERRIDE", &reporter)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    // Isolate group delivery and cleanup from the test runner.
                    .process_group(0);
                // Control both incoming dispositions only in the subprocess,
                // independently of the test runner's own inherited ignores.
                unsafe {
                    command.pre_exec(move || {
                        let disposition = if ignored {
                            libc::SIG_IGN
                        } else {
                            libc::SIG_DFL
                        };
                        if libc::signal(signal, disposition) == libc::SIG_ERR {
                            return Err(std::io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
                let mut child = command.spawn().unwrap();
                let deadline = Instant::now() + Duration::from_secs(30);
                while !ready.exists() && Instant::now() < deadline {
                    if child.try_wait().unwrap().is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                let reporter_started = ready.exists();
                // Observe the trace while the reporter is blocked on stdin,
                // proving finalization precedes reporter completion.
                let trace = std::fs::read_to_string(dir.join("trace.json"));
                let pid = child.id() as libc::pid_t;
                let sent = reporter_started
                    && unsafe { libc::kill(if group { -pid } else { pid }, signal) } == 0;
                if ignored {
                    // Successful completion proves that both Moon and the
                    // reporter retained the inherited ignored disposition.
                    let _ = child.stdin.take().unwrap().write_all(b"finish\n");
                }
                let exited =
                    sent && crate::process::wait_for_child_exit(&mut child, Duration::from_secs(2));
                // Clean up a reporter left waiting after parent-only termination,
                // or the entire group if a regression prevents Moon from exiting.
                unsafe { libc::kill(-pid, libc::SIGKILL) };
                let status = child.wait().unwrap();
                assert!(
                    reporter_started,
                    "reporter did not start (analyze={analyze})"
                );
                assert!(
                    exited,
                    "Moon did not exit: analyze={analyze}, signal={signal}, ignored={ignored}, group={group}"
                );
                serde_json::from_str::<serde_json::Value>(&trace.unwrap()).unwrap();
                if ignored {
                    assert_eq!(
                        status.code(),
                        Some(if analyze { 2 } else { 0 }),
                        "ignored signal {signal}, analyze={analyze}, group={group}"
                    );
                    assert!(
                        done.exists(),
                        "reporter did not complete after ignored signal"
                    );
                    std::fs::remove_file(done).unwrap();
                } else {
                    assert_eq!(status.signal(), Some(signal));
                }
                std::fs::remove_file(ready).unwrap();
                std::fs::remove_file(dir.join("trace.json")).unwrap();
            }
        }
    }
}

#[test]
fn test_moon_coverage_analyze_legacy() {
    let dir = TestDir::new("test_coverage.in");
    check(
        get_stdout_with_envs(
            &dir,
            [
                "coverage",
                "analyze",
                "--test-flag=--target=wasm-gc",
                "--test-flag=--sort-input",
                "--",
                "-f=caret",
            ],
            [("MOON_COVE_REPORT_ENABLED", "0")],
        ),
        expect![["
            warning: this line has no test coverage
             --> lib2/hello.mbt:2
            1\tfn hello_uncovered_1() -> String {
            2\t  \"Hello, world!\"
             \t  ^^^^^^^^^^^^^^^
            3\t}
            4\t
            5\tfn hello_uncovered_2() -> String {


            warning: this line has no test coverage
             --> lib2/hello.mbt:6
            4\t
            5\tfn hello_uncovered_2() -> String {
            6\t  \"Hello, world!\"
             \t  ^^^^^^^^^^^^^^^
            7\t}
            8\t


            warning: this line has no test coverage
             --> lib2/hello.mbt:10
             7\t}
             8\t
             9\tfn hello_uncovered_3() -> String {
            10\t  \"Hello, world!\"
              \t  ^^^^^^^^^^^^^^^
            11\t}


            warning: this line has no test coverage
             --> main/main.mbt:2
            1\tfn main {
            2\t  println(\"main\")
             \t  ^^^^^^^^^^^^^^^
            3\t}


        "]],
    );
}

#[test]
fn test_moon_coverage_analyze_dry_run() {
    let dir = TestDir::new("test_coverage.in");
    build_graph::assert(
        moon_cmd(&dir).args([
            "coverage",
            "analyze",
            "--dry-run",
            "--test-flag=--target=wasm-gc",
            "--test-flag=--nostd",
            "--test-flag=--sort-input",
        ]),
        expect![[r#"
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --async-runtime-package moonbitlang/core/async --enable-coverage --driver-kind blackbox","inputs":["./lib/hello.mbt","$MOON_HOME/bin/moon"],"outputs":["./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/lib/__blackbox_test_info.json"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name username/hello/lib --async-runtime-package moonbitlang/core/async --enable-coverage --driver-kind internal","inputs":["./lib/hello.mbt","$MOON_HOME/bin/moon"],"outputs":["./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt","./_build/wasm-gc/debug/test/lib/__internal_test_info.json"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__whitebox_test_info.json ./lib/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/lib --async-runtime-package moonbitlang/core/async --enable-coverage --driver-kind whitebox","inputs":["./lib/hello_wbtest.mbt","$MOON_HOME/bin/moon"],"outputs":["./_build/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt","./_build/wasm-gc/debug/test/lib/__whitebox_test_info.json"]}
            {"command":"moonc build-package ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -pkg-type executable -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/lib/lib.core","./_build/wasm-gc/debug/test/lib/lib.mi","./lib/hello.mbt","./lib/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib_blackbox_test:./lib -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core","./_build/wasm-gc/debug/test/lib/lib.core","./lib/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm"]}
            {"command":"moonc build-package ./lib/hello.mbt ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -pkg-type executable -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -enable-coverage -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt","./lib/hello.mbt","./lib/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.internal_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg -pkg-sources username/hello/lib:./lib -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/lib.internal_test.core","./lib/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.internal_test.wasm"]}
            {"command":"moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -pkg-type library -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -enable-coverage -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./lib/hello.mbt","./lib/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.mi","./_build/wasm-gc/debug/test/lib/lib.core"]}
            {"command":"moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./_build/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./_build/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg username/hello/lib -pkg-type executable -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -enable-coverage -whitebox-test -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt","./lib/hello.mbt","./lib/hello_wbtest.mbt","./lib/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.whitebox_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.whitebox_test.core -main username/hello/lib -o ./_build/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg -pkg-sources username/hello/lib:./lib -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/lib.whitebox_test.core","./lib/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib/lib.whitebox_test.wasm"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib2/__blackbox_test_info.json ./lib2/hello_test.mbt --doctest-only ./lib2/hello.mbt --target wasm-gc --pkg-name username/hello/lib2 --async-runtime-package moonbitlang/core/async --enable-coverage --driver-kind blackbox","inputs":["./lib2/hello.mbt","./lib2/hello_test.mbt","$MOON_HOME/bin/moon"],"outputs":["./_build/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/lib2/__blackbox_test_info.json"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib2/__internal_test_info.json ./lib2/hello.mbt --target wasm-gc --pkg-name username/hello/lib2 --async-runtime-package moonbitlang/core/async --enable-coverage --driver-kind internal","inputs":["./lib2/hello.mbt","$MOON_HOME/bin/moon"],"outputs":["./_build/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt","./_build/wasm-gc/debug/test/lib2/__internal_test_info.json"]}
            {"command":"moonc build-package ./lib2/hello_test.mbt ./_build/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib2/hello.mbt -o ./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.core -pkg username/hello/lib2_blackbox_test -pkg-type executable -i ./_build/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib2/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/lib2/lib2.core","./_build/wasm-gc/debug/test/lib2/lib2.mi","./lib2/hello.mbt","./lib2/hello_test.mbt","./lib2/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib2/lib2.core ./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.core -main username/hello/lib2_blackbox_test -o ./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.wasm -test-mode -pkg-config-path ./lib2/moon.pkg -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.core","./_build/wasm-gc/debug/test/lib2/lib2.core","./lib2/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.blackbox_test.wasm"]}
            {"command":"moonc build-package ./lib2/hello.mbt ./_build/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/lib2/lib2.internal_test.core -pkg username/hello/lib2 -pkg-type executable -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -g -O0 -source-map -enable-coverage -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib2/__generated_driver_for_internal_test.mbt","./lib2/hello.mbt","./lib2/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.internal_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib2/lib2.internal_test.core -main username/hello/lib2 -o ./_build/wasm-gc/debug/test/lib2/lib2.internal_test.wasm -test-mode -pkg-config-path ./lib2/moon.pkg -pkg-sources username/hello/lib2:./lib2 -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib2/lib2.internal_test.core","./lib2/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.internal_test.wasm"]}
            {"command":"moonc build-package ./lib2/hello.mbt -o ./_build/wasm-gc/debug/test/lib2/lib2.core -pkg username/hello/lib2 -pkg-type library -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -g -O0 -source-map -enable-coverage -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./lib2/hello.mbt","./lib2/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/lib2/lib2.mi","./_build/wasm-gc/debug/test/lib2/lib2.core"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --async-runtime-package moonbitlang/core/async --enable-coverage --driver-kind blackbox","inputs":["./main/main.mbt","$MOON_HOME/bin/moon"],"outputs":["./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/main/__blackbox_test_info.json"]}
            {"command":"moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/main/__internal_test_info.json ./main/main.mbt --target wasm-gc --pkg-name username/hello/main --async-runtime-package moonbitlang/core/async --enable-coverage --driver-kind internal","inputs":["./main/main.mbt","$MOON_HOME/bin/moon"],"outputs":["./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt","./_build/wasm-gc/debug/test/main/__internal_test_info.json"]}
            {"command":"moonc build-package ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -pkg username/hello/main_blackbox_test -pkg-type executable -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -i ./_build/wasm-gc/debug/test/lib2/lib2.mi:lib2 -i ./_build/wasm-gc/debug/test/main/main.mi:main -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/lib.core","./_build/wasm-gc/debug/test/lib/lib.mi","./_build/wasm-gc/debug/test/lib2/lib2.core","./_build/wasm-gc/debug/test/lib2/lib2.mi","./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt","./_build/wasm-gc/debug/test/main/main.core","./_build/wasm-gc/debug/test/main/main.mi","./main/main.mbt","./main/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/main/main.blackbox_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib2/lib2.core ./_build/wasm-gc/debug/test/main/main.core ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -main username/hello/main_blackbox_test -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/main:./main -pkg-sources username/hello/main_blackbox_test:./main -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/lib.core","./_build/wasm-gc/debug/test/lib2/lib2.core","./_build/wasm-gc/debug/test/main/main.blackbox_test.core","./_build/wasm-gc/debug/test/main/main.core","./main/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/main/main.blackbox_test.wasm"]}
            {"command":"moonc build-package ./main/main.mbt ./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/main/main.internal_test.core -pkg username/hello/main -pkg-type executable -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -i ./_build/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -enable-coverage -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/lib.core","./_build/wasm-gc/debug/test/lib/lib.mi","./_build/wasm-gc/debug/test/lib2/lib2.core","./_build/wasm-gc/debug/test/lib2/lib2.mi","./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt","./main/main.mbt","./main/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/main/main.internal_test.core"]}
            {"command":"moonc link-core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib2/lib2.core ./_build/wasm-gc/debug/test/main/main.internal_test.core -main username/hello/main -o ./_build/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./main/moon.pkg -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib2:./lib2 -pkg-sources username/hello/main:./main -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/lib.core","./_build/wasm-gc/debug/test/lib2/lib2.core","./_build/wasm-gc/debug/test/main/main.internal_test.core","./main/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/main/main.internal_test.wasm"]}
            {"command":"moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.core -pkg username/hello/main -pkg-type executable -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -i ./_build/wasm-gc/debug/test/lib2/lib2.mi:lib2 -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -enable-coverage -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json","inputs":["$MOON_HOME/bin/moonc","./_build/wasm-gc/debug/test/lib/lib.core","./_build/wasm-gc/debug/test/lib/lib.mi","./_build/wasm-gc/debug/test/lib2/lib2.core","./_build/wasm-gc/debug/test/lib2/lib2.mi","./main/main.mbt","./main/moon.pkg"],"outputs":["./_build/wasm-gc/debug/test/main/main.mi","./_build/wasm-gc/debug/test/main/main.core"]}
        "#]],
    );
}

// TODO: Re-enable when a published lijunchen/hello18 version uses
// moon.mod and moon.pkg instead of JSON manifests.
/*
#[test]
fn test_moon_coverage_analyze_third_party() {
    let dir = TestDir::new("third_party");
    build_graph::assert(
        moon_cmd(&dir).args([
            "coverage",
            "analyze",
            "--dry-run",
            "--test-flag=--target=wasm-gc",
            "--test-flag=--nostd",
            "--test-flag=--sort-input",
        ]),
        expect_file!["third_party_coverage_dry_run.jsonl"],
    );
}
*/
