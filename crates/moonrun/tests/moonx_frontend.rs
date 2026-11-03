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

//! Frontend completion must not require an artifact, helper, or compiler.

use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::Path;

use moonrun::{Engine, RunOptions, RunOutcome};

#[test]
fn frontend_exits_without_preparation() {
    let Ok(root) = std::env::var("MOONRUN_FRONTEND_TEST_ROOT") else {
        let directory = tempfile::tempdir().unwrap();
        // Isolate process-global temporary-directory selection from other tests.
        let unavailable = directory.path().join("unavailable");
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "frontend_exits_without_preparation",
                "--nocapture",
            ])
            .env("MOONRUN_FRONTEND_TEST_ROOT", directory.path())
            .env("TMPDIR", &unavailable)
            .env("TMP", &unavailable)
            .env("TEMP", &unavailable)
            .timeout(std::time::Duration::from_secs(30))
            .assert()
            .success();
        return;
    };
    let root = Path::new(&root);
    assert!(
        tempfile::tempdir().is_err(),
        "handoff storage must be unavailable"
    );
    let moonx = root.join(format!("moonx{}", std::env::consts::EXE_SUFFIX));
    assert!(!moonx.exists(), "no external frontend may run");
    let engine = Engine::default();
    for (args, code, fragment) in [
        (vec!["--help"], 0, "Run a standalone .mbtx file"),
        (vec!["--daemon", "--help"], 0, "Run a standalone .mbtx file"),
        (vec!["--version"], 0, "moonx "),
        (vec![], 2, "Usage:"),
        (vec!["--target", "native"], 2, "invalid value"),
        (vec!["--unknown-option"], 2, "unexpected argument"),
        (
            vec!["--moonrun-artifact-handoff", "untrusted", "script.mbtx"],
            2,
            "unexpected argument",
        ),
    ] {
        let mut stdin = tempfile::tempfile_in(root).unwrap();
        stdin.write_all(b"untouched").unwrap();
        stdin.rewind().unwrap();
        let stdout = tempfile::tempfile_in(root).unwrap();
        let stderr = tempfile::tempfile_in(root).unwrap();
        let result = engine.run_moon(
            &moonx,
            &args.iter().map(Into::into).collect::<Vec<_>>(),
            RunOptions::default().with_stdio(
                stdin.try_clone().unwrap(),
                stdout.try_clone().unwrap(),
                stderr.try_clone().unwrap(),
            ),
        );
        assert_eq!(result.unwrap(), RunOutcome::Exited(code), "{args:?}");
        assert_eq!(stdin.stream_position().unwrap(), 0);
        let stdout = contents(stdout);
        let stderr = contents(stderr);
        let (output, other) = if code == 0 {
            (stdout, stderr)
        } else {
            (stderr, stdout)
        };
        assert!(output.contains(fragment), "{args:?}: {output}");
        assert_eq!(other, "");
    }

    // These flags belong to the guest. A real execution request must still try
    // preparation, which cannot succeed without either storage or a helper.
    for flag in ["--help", "--version", "--daemon"] {
        let error = engine
            .run_moon(
                &moonx,
                &["script.mbtx".into(), flag.into()],
                RunOptions::default(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("artifact handoff"), "{error:#}");
    }

    // Reject an unsupported combination before spawning a build helper.
    #[cfg(feature = "v8")]
    {
        let error = engine
            .run_moon(
                &moonx,
                &["--daemon".into(), "script.mbtx".into()],
                RunOptions::default().with_virtualize(true),
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("IPC virtual process sessions"),
            "{error:#}"
        );
    }
}

fn contents(mut file: File) -> String {
    file.rewind().unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    contents
}

#[cfg(feature = "v8")]
#[test]
fn cancellation_before_frontend_completion_keeps_streams_untouched() {
    use moonrun::{RunTermination, termination_channel};

    let engine = Engine::default();
    for virtualize in [false, true] {
        for flag in ["--help", "--unknown-option"] {
            let (terminator, receiver) = termination_channel();
            assert!(terminator.terminate(RunTermination::Exit(7)));
            let stdout = tempfile::tempfile().unwrap();
            let stderr = tempfile::tempfile().unwrap();
            assert_eq!(
                engine
                    .run_moon(
                        Path::new("moonx"),
                        &[flag.into()],
                        RunOptions::default()
                            .with_virtualize(virtualize)
                            .with_termination_receiver(receiver)
                            .with_stdio(
                                tempfile::tempfile().unwrap(),
                                stdout.try_clone().unwrap(),
                                stderr.try_clone().unwrap(),
                            ),
                    )
                    .unwrap(),
                RunOutcome::Exited(7),
            );
            assert_eq!(contents(stdout), "");
            assert_eq!(contents(stderr), "");
            assert!(!terminator.terminate(RunTermination::Exit(8)));
            assert!(engine.snapshot().is_empty());
        }
    }
}

#[test]
fn frontend_write_failures_preserve_cli_exit_status() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("read-only-output");
    std::fs::write(&output, b"unchanged").unwrap();
    let engine = Engine::default();
    for (flag, code) in [("--help", 0), ("--unknown-option", 2)] {
        // Clap's native CLI exit ignores diagnostic-output errors. The virtual
        // frontend must not convert the same failure into a runtime error.
        assert_eq!(
            engine
                .run_moon(
                    &directory.path().join("moonx"),
                    &[flag.into()],
                    RunOptions::default().with_stdio(
                        File::open(&output).unwrap(),
                        File::open(&output).unwrap(),
                        File::open(&output).unwrap(),
                    ),
                )
                .unwrap(),
            RunOutcome::Exited(code),
        );
        assert_eq!(std::fs::read(&output).unwrap(), b"unchanged");
    }
}

#[cfg(all(unix, feature = "v8"))]
#[test]
fn daemon_preparation_preserves_engine_shutdown() {
    use moonrun::RunTermination;
    use std::io::BufRead;
    use std::os::unix::{fs::PermissionsExt, net::UnixStream};
    // Isolate fixture creation from other tests that fork with open write fds.
    const CHILD: &str = "MOONRUN_DAEMON_PREPARATION_TEST";
    if std::env::var_os(CHILD).is_none() {
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "daemon_preparation_preserves_engine_shutdown"])
            .env(CHILD, "1")
            .timeout(std::time::Duration::from_secs(20))
            .assert()
            .success();
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("moonx");
    std::fs::write(
        &helper,
        "#!/bin/sh\nprintf 'ready\\n'\nread release\nexit 19\n",
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    for frontend_flag in [false, true] {
        let engine = Engine::default();
        let (mut controller, child) = UnixStream::pair().unwrap();
        controller
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let child_in: std::os::fd::OwnedFd = child.try_clone().unwrap().into();
        let child_out: std::os::fd::OwnedFd = child.into();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let args = if frontend_flag {
                    vec!["--daemon".into(), "script.mbtx".into()]
                } else {
                    vec!["script.mbtx".into()]
                };
                let options = RunOptions::default().with_stdio(
                    File::from(child_in),
                    File::from(child_out),
                    tempfile::tempfile().unwrap(),
                );
                engine.run_moon(
                    &helper,
                    &args,
                    if frontend_flag {
                        options
                    } else {
                        options.with_ipc(true)
                    },
                )
            });
            let mut ready = String::new();
            let read = std::io::BufReader::new(&controller).read_line(&mut ready);
            let admitted = engine.snapshot().len();
            engine.request_shutdown(RunTermination::Exit(42));
            // Always release the helper before asserting, including on failure.
            controller.write_all(b"release\n").unwrap();
            let result = worker.join().unwrap();
            read.unwrap();
            assert_eq!(ready, "ready\n");
            assert_eq!(
                result.unwrap(),
                RunOutcome::Exited(42),
                "frontend flag: {frontend_flag}"
            );
            assert_eq!(admitted, 1);
        });
        assert!(engine.snapshot().is_empty());
    }
}
