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
use crate::async_host::AsyncHostError;
use crate::process::{HostProcess, Job, SpawnOptions};
use crate::runtime::{Executable, Runtime};
use std::os::unix::fs::PermissionsExt;

fn with_parent(engine: &Engine, directory: &Path, body: impl FnOnce(&HostProcess)) {
    engine
        .run_inline(
            RunOptions::default()
                .with_virtualize(true)
                .with_environment([("PATH", directory.as_os_str())])
                .with_working_directory(WorkingDirectory::Fixed(directory.to_owned()))
                .with_stdio(
                    tempfile::tempfile().unwrap(),
                    tempfile::tempfile().unwrap(),
                    tempfile::tempfile().unwrap(),
                ),
            crate::signal_channel().1,
            |_, options, signals, _| {
                let runtime = Runtime::new(options.runtime, Executable::unavailable(), signals)?;
                body(runtime.async_host().process());
                Ok(RunOutcome::Completed)
            },
        )
        .unwrap();
}

fn spawn_job(program: &str, args: &[&str]) -> Job {
    let mut mask = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::sigemptyset(&mut mask) }, 0);
    Job::spawn_unix(
        program.into(),
        std::iter::once(program)
            .chain(args.iter().copied())
            .map(Into::into)
            .collect(),
        vec!["VALUE=chosen".into(), "PATH=/not-the-caller-path".into()],
        None,
        None,
        None,
        None,
        SpawnOptions {
            child_signal_mask: mask,
        },
    )
}

#[test]
fn native_identity_lookup_and_unread_completion() {
    let engine = Engine::default();
    let directory = tempfile::tempdir().unwrap();
    let probe = directory.path().join("probe");
    std::fs::write(
        &probe,
        "#!/bin/sh\nprintf '%s' \"$$\" > pid\n[ \"$VALUE\" = chosen ] || exit 91\nexit 23\n",
    )
    .unwrap();
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755)).unwrap();
    with_parent(&engine, directory.path(), |process| {
        let mut spawn = spawn_job("probe", &[]);
        process.prepare_job(&mut spawn).unwrap();
        let pid = spawn.run().unwrap() as i32;
        assert!(engine.snapshot().iter().any(|entry|
            entry.pid.get() == pid && entry.parent.unwrap().get() == process.pid()));
        let mut wait = process.wait_job(None, None, pid).unwrap();
        assert_eq!(wait.run().unwrap(), 23);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("pid")).unwrap(),
            pid.to_string()
        );
        // The OS child has been reaped, but the unread result is still owned by
        // this parent. Late control must not use the now-reusable OS PID.
        assert_eq!(
            unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
        assert!(
            !engine
                .terminate(ProcessId(pid), RunTermination::Exit(99))
                .unwrap()
        );
        drop(wait);
        assert_eq!(
            process
                .context()
                .unwrap()
                .read_exit(pid, |_| Err::<(), _>(AsyncHostError::Fault)),
            Err(AsyncHostError::Fault)
        );
        assert_eq!(process.context().unwrap().read_exit(pid, Ok), Ok(23));
        assert_eq!(process.kill(pid), Err(AsyncHostError::PermissionDenied));
    });
    assert!(engine.snapshot().is_empty());
}

#[test]
fn native_signal_and_cancelled_wait_keep_child_ownership() {
    // Exercise stop/continue racing the monitor's initial exit observation.
    for signal in [libc::SIGKILL, libc::SIGTERM].into_iter().cycle().take(16) {
        let engine = Engine::default();
        let directory = tempfile::tempdir().unwrap();
        with_parent(&engine, directory.path(), |process| {
            let mut spawn = spawn_job("/bin/sleep", &["10"]);
            process.prepare_job(&mut spawn).unwrap();
            let pid = spawn.run().unwrap() as i32;
            assert_eq!(
                process.kill(std::process::id() as i32),
                Err(AsyncHostError::PermissionDenied)
            );
            let mut wait = process.wait_job(None, None, pid).unwrap();
            wait.cancellation_override().unwrap().cancel().unwrap();
            assert_eq!(wait.run(), Err(AsyncHostError::Native(libc::EINTR)));
            drop(wait);
            // Native children retain native signal semantics, including
            // stop/continue, which are unsupported for Wasm executions.
            process.terminate(pid, libc::SIGSTOP).unwrap();
            process.terminate(pid, libc::SIGCONT).unwrap();
            process.terminate(pid, signal).unwrap();
            let mut wait = process.wait_job(None, None, pid).unwrap();
            assert_eq!(wait.run().unwrap(), -i64::from(signal));
            wait.consume_wait_result();
            assert!(matches!(
                process.wait_job(None, None, pid),
                Err(AsyncHostError::PermissionDenied)
            ));
        });
        assert!(engine.snapshot().is_empty());
    }
}

#[test]
fn native_children_are_adopted_and_shutdown_reaps_os_status() {
    let engine = Engine::default();
    let directory = tempfile::tempdir().unwrap();
    with_parent(&engine, directory.path(), |process| {
        let mut spawn = spawn_job("/bin/sleep", &["10"]);
        process.prepare_job(&mut spawn).unwrap();
        spawn.run().unwrap();
    });
    let children = engine.snapshot();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].parent, None);
    assert!(engine.request_shutdown(RunTermination::Exit(99)));
    assert_eq!(
        engine.wait(children[0].pid).unwrap(),
        RunOutcome::KilledBySignal(libc::SIGKILL)
    );
    assert!(engine.snapshot().is_empty());
}

#[test]
fn native_spawn_errors_do_not_admit_children() {
    let engine = Engine::default();
    let directory = tempfile::tempdir().unwrap();
    with_parent(&engine, directory.path(), |process| {
        let mut spawn = spawn_job("missing", &[]);
        process.prepare_job(&mut spawn).unwrap();
        assert_eq!(spawn.run(), Err(AsyncHostError::Native(libc::ENOENT)));
        assert_eq!(engine.snapshot().len(), 1);
    });
    assert!(engine.snapshot().is_empty());
}

#[test]
fn shutdown_racing_native_spawn_includes_or_rejects_the_child() {
    for _ in 0..16 {
        let engine = Engine::default();
        let directory = tempfile::tempdir().unwrap();
        with_parent(&engine, directory.path(), |process| {
            let mut spawn = spawn_job("/bin/sleep", &["10"]);
            process.prepare_job(&mut spawn).unwrap();
            let barrier = std::sync::Barrier::new(2);
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    barrier.wait();
                    spawn.run()
                });
                barrier.wait();
                engine.request_shutdown(RunTermination::Exit(99));
                match worker.join().unwrap() {
                    Ok(pid) => assert!(
                        engine
                            .snapshot()
                            .iter()
                            .any(|entry| entry.pid.get() == pid as i32)
                    ),
                    Err(error) => assert_eq!(error, AsyncHostError::PermissionDenied),
                }
            });
        });
        for child in engine.snapshot() {
            assert_eq!(child.parent, None);
            assert_eq!(
                engine.wait(child.pid).unwrap(),
                RunOutcome::KilledBySignal(libc::SIGKILL)
            );
        }
        assert!(engine.snapshot().is_empty());
    }
}
