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

use super::*;
use crate::process::{HostProcess, Job, SpawnOptions};
use crate::runtime::{Env, Stdio};
use std::ffi::OsString;
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const FIXTURE: &str = "process_tree::native::tests::native_child_fixture";

#[test]
fn native_child_fixture() {
    let Some(action) = std::env::var_os("MOONRUN_NATIVE_FIXTURE") else {
        return;
    };
    if action == "io" {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        writeln!(
            std::io::stdout(),
            "payload={input};env={};cwd={}",
            std::env::var("CHILD_VALUE").unwrap(),
            std::env::current_dir().unwrap().display()
        )
        .unwrap();
        writeln!(std::io::stderr(), "native stderr").unwrap();
        std::process::exit(23);
    }
    #[cfg(unix)]
    if action == "signal" {
        extern "C" fn stop(_: i32) {
            unsafe {
                libc::_exit(37);
            }
        }
        let mut handler = unsafe { std::mem::zeroed::<libc::sigaction>() };
        handler.sa_sigaction = stop as *const () as usize;
        assert_eq!(
            unsafe { libc::sigaction(libc::SIGTERM, &handler, std::ptr::null_mut()) },
            0
        );
    }
    let ready = std::env::var_os("READY").unwrap();
    std::fs::write(ready, b"ready").unwrap();
    // A failed test must not leave a permanent child behind.
    std::thread::sleep(Duration::from_secs(15));
    std::process::exit(99);
}

fn in_root<T>(engine: &Engine, f: impl FnOnce(ProcessContext) -> T) -> T {
    let mut result = None;
    engine
        .run_inline(
            RunOptions::default().with_virtualize(true),
            crate::signal_channel().1,
            |_, options, _, _| {
                result = Some(f(options.runtime.process_context.unwrap()));
                Ok(RunOutcome::Completed)
            },
        )
        .unwrap();
    result.unwrap()
}

fn host(context: ProcessContext, cwd: &Path, stdio: Stdio) -> HostProcess {
    HostProcess::new(
        None,
        None,
        Arc::new(WorkingDirectory::Fixed(cwd.to_owned())),
        Arc::new(stdio),
        Some(context),
        Arc::new(Env::owned([(OsString::from("PATH"), cwd.as_os_str().to_owned())]).unwrap()),
    )
}

fn fixture_job(executable: &Path, environment: Vec<(OsString, OsString)>) -> Job {
    #[cfg(unix)]
    return Job::spawn_unix(
        executable.as_os_str().to_owned(),
        vec![
            executable.as_os_str().to_owned(),
            "--exact".into(),
            FIXTURE.into(),
            "--nocapture".into(),
        ],
        environment
            .into_iter()
            .map(|(mut key, value)| {
                key.push("=");
                key.push(value);
                key
            })
            .collect(),
        None,
        None,
        None,
        None,
        SpawnOptions {
            child_signal_mask: unsafe { std::mem::zeroed() },
        },
    );
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let command = format!("\"{}\" --exact {FIXTURE} --nocapture", executable.display());
        let mut env: Vec<u16> = environment
            .into_iter()
            .flat_map(|(mut key, value)| {
                key.push("=");
                key.push(value);
                key.encode_wide().chain([0]).collect::<Vec<_>>()
            })
            .chain([0])
            .collect();
        if env.len() == 1 {
            env.push(0);
        }
        Job::spawn_windows(
            command.into(),
            env,
            None,
            None,
            None,
            None,
            SpawnOptions {
                no_console_window: false,
                is_orphan: false,
            },
        )
    }
}

fn waiting_child(process: &HostProcess, directory: &Path, label: &str) -> i32 {
    let ready = directory.join(label);
    let mut job = fixture_job(
        &std::env::current_exe().unwrap(),
        vec![
            ("MOONRUN_NATIVE_FIXTURE".into(), label.into()),
            ("READY".into(), ready.as_os_str().to_owned()),
        ],
    );
    process.prepare_job(&mut job).unwrap();
    let pid = job.run().unwrap() as i32;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(Instant::now() < deadline, "native child did not start");
        std::thread::sleep(Duration::from_millis(2));
    }
    pid
}

#[test]
fn denied_spawn_precedes_lookup_and_allowed_spawn_preserves_os_errors() {
    for allowed in [false, true] {
        let engine = Engine::default();
        let dir = tempfile::tempdir().unwrap();
        in_root(&engine, |context| {
            let (mut policy, _) = crate::policy::load_inherited_json(
                format!(r#"{{"process":{{"spawn":{allowed}}}}}"#).as_bytes(),
                &Env::ambient(),
            )
            .unwrap();
            let process = HostProcess::new(
                policy.take_process_policy(),
                policy.take_policy_inheritance(),
                Arc::new(WorkingDirectory::Fixed(dir.path().to_owned())),
                Arc::new(Stdio::Ambient),
                Some(context),
                Arc::new(Env::ambient()),
            );
            let mut job = fixture_job(&dir.path().join("missing-program"), Vec::new());
            let result = process.prepare_job(&mut job).and_then(|()| job.run());
            #[cfg(unix)]
            assert_eq!(
                result,
                Err(if allowed {
                    AsyncHostError::Native(libc::ENOENT)
                } else {
                    AsyncHostError::PermissionDenied
                })
            );
            #[cfg(windows)]
            assert!(result.is_err());
            assert_eq!(engine.snapshot().len(), 1);
        });
        assert!(engine.snapshot().is_empty());
    }
}

#[test]
fn native_state_and_status_survive_parent_exit_and_unread_spawn_result() {
    let engine = Engine::default();
    let dir = tempfile::tempdir().unwrap();
    let cwd = dunce::canonicalize(dir.path()).unwrap();
    let mut input = tempfile::tempfile().unwrap();
    input.write_all(b"hello native").unwrap();
    input.rewind().unwrap();
    let mut output = tempfile::tempfile().unwrap();
    let mut error = tempfile::tempfile().unwrap();
    let pid = in_root(&engine, |context| {
        let process = host(
            context,
            &cwd,
            Stdio::owned(
                input,
                output.try_clone().unwrap(),
                error.try_clone().unwrap(),
            ),
        );
        let mut job = fixture_job(
            &std::env::current_exe().unwrap(),
            vec![
                ("MOONRUN_NATIVE_FIXTURE".into(), "io".into()),
                ("CHILD_VALUE".into(), "child not caller".into()),
            ],
        );
        process.prepare_job(&mut job).unwrap();
        // Prepared requests pin owned descriptors after their Runtime is gone.
        drop(process);
        VirtualPid(job.run().unwrap() as i32)
    });
    assert_eq!(engine.snapshot()[0].parent, None);
    assert_eq!(engine.wait(pid).unwrap(), RunOutcome::Exited(23));
    output.rewind().unwrap();
    error.rewind().unwrap();
    let mut stdout = String::new();
    let mut stderr = String::new();
    output.read_to_string(&mut stdout).unwrap();
    error.read_to_string(&mut stderr).unwrap();
    assert!(
        stdout.contains(&format!(
            "payload=hello native;env=child not caller;cwd={}",
            cwd.display()
        )),
        "{stdout}"
    );
    assert!(stderr.contains("native stderr"), "{stderr}");
    assert!(engine.snapshot().is_empty());
}

#[test]
fn cancelling_wait_keeps_child_and_kill_only_targets_one_member() {
    let engine = Engine::default();
    let directory = tempfile::tempdir().unwrap();
    in_root(&engine, |context| {
        let process = host(context.clone(), directory.path(), Stdio::Ambient);
        let first = waiting_child(&process, directory.path(), "first");
        let second = waiting_child(&process, directory.path(), "second");
        let mut wait = context.wait(first).unwrap();
        wait.cancellation().cancel().unwrap();
        assert!(wait.run().is_err());
        drop(wait);
        context.kill(first).unwrap();
        let mut wait = context.wait(first).unwrap();
        #[cfg(unix)]
        assert_eq!(wait.run().unwrap(), -i64::from(libc::SIGKILL));
        #[cfg(windows)]
        assert_eq!(wait.run().unwrap(), 1);
        wait.consume();
        assert_eq!(
            engine
                .snapshot()
                .iter()
                .find(|e| e.pid.get() == second)
                .unwrap()
                .state,
            ProcessState::Running
        );
        assert!(
            engine
                .terminate(VirtualPid(second), RunTermination::Exit(42))
                .unwrap()
        );
        #[cfg(unix)]
        assert_eq!(
            engine.wait(VirtualPid(second)).unwrap(),
            RunOutcome::KilledBySignal(libc::SIGKILL)
        );
        #[cfg(windows)]
        assert_eq!(
            engine.wait(VirtualPid(second)).unwrap(),
            RunOutcome::Exited(42)
        );
    });
    assert!(engine.snapshot().is_empty());
}

#[cfg(unix)]
#[test]
fn native_graceful_signal_reports_the_programs_actual_exit() {
    let engine = Engine::default();
    let directory = tempfile::tempdir().unwrap();
    in_root(&engine, |context| {
        let process = host(context.clone(), directory.path(), Stdio::Ambient);
        let pid = waiting_child(&process, directory.path(), "signal");
        context.terminate(pid, libc::SIGTERM).unwrap();
        assert_eq!(
            engine.wait(VirtualPid(pid)).unwrap(),
            RunOutcome::Exited(37)
        );
        assert!(context.terminate(pid, libc::SIGTERM).is_err());
    });
}

#[test]
fn shutdown_racing_spawn_either_rejects_or_reaps_native_child() {
    for _ in 0..12 {
        let engine = Engine::default();
        let directory = tempfile::tempdir().unwrap();
        in_root(&engine, |context| {
            let process = host(context, directory.path(), Stdio::Ambient);
            let mut job = fixture_job(&std::env::current_exe().unwrap(), Vec::new());
            process.prepare_job(&mut job).unwrap();
            let gate = std::sync::Barrier::new(2);
            let spawned = std::thread::scope(|threads| {
                let worker = threads.spawn(|| {
                    gate.wait();
                    job.run()
                });
                gate.wait();
                engine.request_shutdown(RunTermination::Exit(19)).unwrap();
                worker.join().unwrap()
            });
            if let Ok(pid) = spawned {
                engine.wait(VirtualPid(pid as i32)).unwrap();
            } else {
                assert_eq!(spawned, Err(AsyncHostError::PermissionDenied));
            }
        });
        assert!(engine.snapshot().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn automatic_native_reaping_is_rejected_without_changing_host_signals() {
    const MODE: &str = "MOONRUN_NATIVE_REAP_MODE";
    let Some(mode) = std::env::var_os(MODE) else {
        for mode in ["ignore", "no-wait"] {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "process_tree::native::tests::automatic_native_reaping_is_rejected_without_changing_host_signals", "--nocapture"])
                .env(MODE, mode).timeout(Duration::from_secs(15))
                .assert().success();
        }
        return;
    };
    let mut disposition = unsafe { std::mem::zeroed::<libc::sigaction>() };
    if mode == "ignore" {
        disposition.sa_sigaction = libc::SIG_IGN;
    } else {
        disposition.sa_sigaction = libc::SIG_DFL;
        disposition.sa_flags = libc::SA_NOCLDWAIT;
    }
    assert_eq!(
        unsafe { libc::sigaction(libc::SIGCHLD, &disposition, std::ptr::null_mut()) },
        0
    );
    // Read back the installed form: Darwin adds SA_NOCLDWAIT for SIG_IGN.
    let mut installed = unsafe { std::mem::zeroed::<libc::sigaction>() };
    assert_eq!(
        unsafe { libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut installed) },
        0
    );
    let engine = Engine::default();
    let directory = tempfile::tempdir().unwrap();
    in_root(&engine, |context| {
        let process = host(context, directory.path(), Stdio::Ambient);
        let mut job = fixture_job(&std::env::current_exe().unwrap(), Vec::new());
        process.prepare_job(&mut job).unwrap();
        assert_eq!(job.run(), Err(AsyncHostError::Native(libc::ENOTSUP)));
        assert_eq!(engine.snapshot().len(), 1);
    });
    let mut after = unsafe { std::mem::zeroed::<libc::sigaction>() };
    assert_eq!(
        unsafe { libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut after) },
        0
    );
    assert_eq!(after.sa_sigaction, installed.sa_sigaction);
    assert_eq!(
        after.sa_flags & libc::SA_NOCLDWAIT,
        installed.sa_flags & libc::SA_NOCLDWAIT
    );
    assert!(engine.snapshot().is_empty());
}
