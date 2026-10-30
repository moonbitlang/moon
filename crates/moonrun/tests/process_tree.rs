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

use moonrun::{Engine, GuestProcessMode, ProcessError};

#[cfg(not(feature = "v8"))]
#[test]
fn unsupported_backend_cannot_admit_virtual_processes() {
    use moonrun::{RunOptions, RunOutcome};

    let engine = Engine::default();
    let module = engine
        .compile(
            "return.wasm",
            wat::parse_str(r#"(module (func (export "_start")))"#).unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
    let error = engine
        .run(
            &module,
            RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual),
        )
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<ProcessError>(),
        Some(ProcessError::UnsupportedBackend)
    ));
    assert!(matches!(
        engine.spawn(
            None,
            &module,
            RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual),
        ),
        Err(ProcessError::UnsupportedBackend)
    ));
    assert!(engine.snapshot().is_empty());
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}

#[cfg(feature = "v8")]
mod v8 {
    use super::*;
    use moonrun::{
        ProcessInfo, ProcessState, RunOptions, RunOutcome, RunTermination, WorkingDirectory,
    };
    use std::sync::{Arc, Barrier, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn lifecycle() {
        let Ok(case) = std::env::var("MOONRUN_PROCESS_TREE_CASE") else {
            // Bound deadlocks and failed interrupts outside the test runner.
            for case in [
                "tree",
                "waiters",
                "completion",
                "environment",
                "stack",
                "default_stack",
                "tiny_stack",
            ] {
                snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "v8::lifecycle", "--nocapture"])
                    .env("MOONRUN_PROCESS_TREE_CASE", case)
                    .timeout(Duration::from_secs(15))
                    .assert()
                    .success();
            }
            return;
        };
        eprintln!("virtual process case: {case}");
        let engine = match case.as_str() {
            "stack" => Engine::new(moonrun::EngineConfig::default().with_stack_size(4096)),
            "tiny_stack" => Engine::new(moonrun::EngineConfig::default().with_stack_size(1)),
            _ => Engine::default(),
        };
        let tree = engine.clone();
        if case == "tiny_stack" {
            // Both execution placements must return an ordinary error. Repeat
            // to check that exception handling leaves the Engine usable.
            for _ in 0..2 {
                let result = engine
                    .compile(
                        "recursive.wasm",
                        wat::parse_str(
                            r#"(module
                            (func $recurse (result i32) call $recurse i32.const 1 i32.add)
                            (func (export "_start") call $recurse drop))"#,
                        )
                        .unwrap(),
                    )
                    .and_then(|module| {
                        let error = engine.run(&module, RunOptions::default()).unwrap_err();
                        assert!(format!("{error:#}").contains("Maximum call stack size exceeded"));
                        let pid = tree.spawn(
                            None,
                            &module,
                            RunOptions::default()
                                .with_guest_process_mode(GuestProcessMode::Virtual),
                        )?;
                        Ok(tree.wait(pid)?)
                    });
                let error = result.unwrap_err();
                assert!(format!("{error:#}").contains("Maximum call stack size exceeded"));
                assert!(tree.snapshot().is_empty());
            }
            return;
        }
        let module = engine
            .compile(
                "loop.wasm",
                wat::parse_str(r#"(module (func (export "_start") (loop br 0)))"#).unwrap(),
            )
            .unwrap();
        match case.as_str() {
            "stack" | "default_stack" => {
                let module = engine
                    .compile(
                        "recursive.wasm",
                        wat::parse_str(
                            r#"(module
                    (func $recurse (result i32) call $recurse i32.const 1 i32.add)
                    (func (export "_start") call $recurse drop))"#,
                        )
                        .unwrap(),
                    )
                    .unwrap();
                let pid = tree
                    .spawn(
                        None,
                        &module,
                        RunOptions::default()
                            .with_guest_process_mode(GuestProcessMode::Virtual)
                            .without_stack_trace(),
                    )
                    .unwrap();
                assert!(matches!(
                    tree.wait(pid),
                    Err(ProcessError::RunFailed(error))
                        if error.to_string().contains("Maximum call stack size exceeded")
                ));
            }
            "tree" => {
                let parent = tree
                    .spawn(
                        None,
                        &module,
                        RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual),
                    )
                    .unwrap();
                let child = tree
                    .spawn(
                        Some(parent),
                        &module,
                        RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual),
                    )
                    .unwrap();
                let grandchild = tree
                    .spawn(
                        Some(child),
                        &module,
                        RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual),
                    )
                    .unwrap();
                assert_eq!(
                    tree.snapshot(),
                    vec![
                        ProcessInfo {
                            pid: parent,
                            parent: None,
                            state: ProcessState::Running
                        },
                        ProcessInfo {
                            pid: child,
                            parent: Some(parent),
                            state: ProcessState::Running
                        },
                        ProcessInfo {
                            pid: grandchild,
                            parent: Some(child),
                            state: ProcessState::Running
                        },
                    ]
                );
                assert!(parent.get() >= 1 << 30);
                let other = Engine::default();
                let foreign = other
                    .spawn(
                        None,
                        &module,
                        RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual),
                    )
                    .unwrap();
                assert!(matches!(
                    tree.terminate(foreign, RunTermination::Exit(1)),
                    Err(ProcessError::NotFound(_))
                ));
                assert!(matches!(
                    tree.spawn(
                        Some(foreign),
                        &module,
                        RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual)
                    ),
                    Err(ProcessError::NotFound(_))
                ));
                assert!(tree.terminate(parent, RunTermination::Exit(7)).unwrap());
                let deadline = Instant::now() + Duration::from_secs(5);
                while tree.snapshot()[0].state != ProcessState::Exited {
                    assert!(Instant::now() < deadline, "parent did not finish");
                    thread::yield_now();
                }
                assert!(matches!(
                    tree.spawn(
                        Some(parent),
                        &module,
                        RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual)
                    ),
                    Err(ProcessError::ParentNotRunning(_))
                ));
                assert!(!tree.terminate(parent, RunTermination::Exit(8)).unwrap());
                assert_eq!(tree.wait(parent).unwrap(), RunOutcome::Exited(7));
                assert_eq!(
                    tree.snapshot(),
                    vec![
                        ProcessInfo {
                            pid: child,
                            parent: None,
                            state: ProcessState::Running
                        },
                        ProcessInfo {
                            pid: grandchild,
                            parent: Some(child),
                            state: ProcessState::Running
                        },
                    ]
                );
                assert!(matches!(
                    tree.terminate(parent, RunTermination::Exit(9)),
                    Err(ProcessError::NotFound(_))
                ));
                assert!(matches!(tree.wait(parent), Err(ProcessError::NotFound(_))));
                for (owner, pid) in [(&tree, child), (&tree, grandchild), (&other, foreign)] {
                    assert!(
                        owner
                            .terminate(pid, RunTermination::KilledBySignal(9))
                            .unwrap()
                    );
                    assert_eq!(owner.wait(pid).unwrap(), RunOutcome::KilledBySignal(9));
                }
                assert!(tree.snapshot().is_empty());
                assert!(other.snapshot().is_empty());
            }
            "waiters" => {
                let pid = tree
                    .spawn(
                        None,
                        &module,
                        RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual),
                    )
                    .unwrap();
                let barrier = Arc::new(Barrier::new(3));
                let (send, receive) = mpsc::channel();
                let waiters: Vec<_> = (0..2)
                    .map(|_| {
                        let tree = tree.clone();
                        let barrier = Arc::clone(&barrier);
                        let send = send.clone();
                        thread::spawn(move || {
                            barrier.wait();
                            send.send(tree.wait(pid)).unwrap();
                        })
                    })
                    .collect();
                barrier.wait();
                assert!(matches!(
                    receive.recv_timeout(Duration::from_secs(5)).unwrap(),
                    Err(ProcessError::AlreadyWaiting(_))
                ));
                // The first waiter is still blocked; admission and termination
                // must continue, including spawning a child of that process.
                let child = tree
                    .spawn(
                        Some(pid),
                        &module,
                        RunOptions::default().with_guest_process_mode(GuestProcessMode::Virtual),
                    )
                    .unwrap();
                assert!(tree.terminate(pid, RunTermination::Exit(2)).unwrap());
                assert_eq!(
                    receive
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap()
                        .unwrap(),
                    RunOutcome::Exited(2)
                );
                assert_eq!(tree.snapshot()[0].parent, None);
                assert!(tree.terminate(child, RunTermination::Exit(3)).unwrap());
                assert_eq!(tree.wait(child).unwrap(), RunOutcome::Exited(3));
                for waiter in waiters {
                    waiter.join().unwrap();
                }
            }
            "completion" => {
                let module = engine
                    .compile(
                        "return.wasm",
                        wat::parse_str(r#"(module (func (export "_start")))"#).unwrap(),
                    )
                    .unwrap();
                let mut previous = None;
                for _ in 0..32 {
                    let pid = tree
                        .spawn(
                            None,
                            &module,
                            RunOptions::default()
                                .with_guest_process_mode(GuestProcessMode::Virtual),
                        )
                        .unwrap();
                    assert!(previous.is_none_or(|previous| pid > previous));
                    previous = Some(pid);
                    let accepted = tree.terminate(pid, RunTermination::Exit(8)).unwrap();
                    assert_eq!(
                        tree.wait(pid).unwrap(),
                        if accepted {
                            RunOutcome::Exited(8)
                        } else {
                            RunOutcome::Completed
                        }
                    );
                    assert!(tree.snapshot().is_empty());
                }
                let pid = tree
                    .spawn(
                        None,
                        &module,
                        RunOptions::default()
                            .with_guest_process_mode(GuestProcessMode::Virtual)
                            .with_test_args("{"),
                    )
                    .unwrap();
                assert!(matches!(tree.wait(pid), Err(ProcessError::RunFailed(_))));
                assert!(tree.snapshot().is_empty());
                let (terminator, receiver) = moonrun::termination_channel();
                assert!(matches!(
                    tree.spawn(
                        None,
                        &module,
                        RunOptions::default()
                            .with_guest_process_mode(GuestProcessMode::Virtual)
                            .with_termination_receiver(receiver)
                    ),
                    Err(ProcessError::TerminationAlreadyConfigured)
                ));
                assert!(!terminator.terminate(RunTermination::Exit(1)));
                assert!(tree.snapshot().is_empty());
            }
            "environment" => {
                const KEY: &str = "MOONRUN_TREE_PRIVATE_ENV_TEST";
                assert!(std::env::var_os(KEY).is_none());
                let directory = tempfile::tempdir().unwrap();
                let cwd = dunce::canonicalize(directory.path()).unwrap();
                let module = engine.compile("environment.wasm", wat::parse_str(format!(r#"(module
                    (import "_" "{KEY}" (global $key (ref extern)))
                    (import "_" "MOONRUN_PROCESS_TREE_CASE" (global $inherited (ref extern)))
                    (import "_" "environment" (global $value (ref extern)))
                    (import "_" {cwd:?} (global $cwd (ref extern)))
                    (import "__moonbit_fs_unstable" "set_env_var" (func $set (param externref externref)))
                    (import "__moonbit_fs_unstable" "get_env_var_exists" (func $exists (param externref) (result i32)))
                    (import "__moonbit_fs_unstable" "get_env_var" (func $get (param externref) (result externref)))
                    (import "__moonbit_fs_unstable" "current_dir" (func $cwd_get (result externref)))
                    (import "wasm:js-string" "equals" (func $equals (param externref externref) (result i32)))
                    (func (export "_start")
                        global.get $key call $exists if unreachable end
                        global.get $key global.get $value call $set
                        global.get $key call $exists i32.eqz if unreachable end
                        global.get $inherited call $get global.get $value call $equals i32.eqz if unreachable end
                        call $cwd_get global.get $cwd call $equals i32.eqz if unreachable end))"#,
                    cwd = cwd.to_str().unwrap())).unwrap()).unwrap();
                // Execution placement must not change the virtual Env/cwd
                // contract, and inline teardown must leave no waitable entry.
                assert_eq!(
                    engine
                        .run(
                            &module,
                            RunOptions::default()
                                .with_guest_process_mode(GuestProcessMode::Virtual)
                                .with_working_directory(WorkingDirectory::Fixed(cwd.clone())),
                        )
                        .unwrap(),
                    RunOutcome::Completed
                );
                assert!(tree.snapshot().is_empty());
                let pids: Vec<_> = (0..2)
                    .map(|_| {
                        tree.spawn(
                            None,
                            &module,
                            RunOptions::default()
                                .with_guest_process_mode(GuestProcessMode::Virtual)
                                .with_working_directory(WorkingDirectory::Fixed(cwd.clone())),
                        )
                        .unwrap()
                    })
                    .collect();
                for pid in pids {
                    assert_eq!(tree.wait(pid).unwrap(), RunOutcome::Completed);
                }
                assert!(std::env::var_os(KEY).is_none());
                // Explicit policy provisioning must not gain the host snapshot.
                let directory = tempfile::tempdir().unwrap();
                let policy = directory.path().join("empty-env.toml");
                std::fs::write(&policy, "").unwrap();
                let module = engine.compile("empty-env.wasm", wat::parse_str(r#"(module
                    (import "wasi_snapshot_preview1" "environ_sizes_get" (func $sizes (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (func (export "_start")
                        i32.const 0 i32.const 4 call $sizes if unreachable end
                        i32.const 0 i32.load if unreachable end
                        i32.const 4 i32.load if unreachable end))"#).unwrap()).unwrap();
                let pid = tree
                    .spawn(
                        None,
                        &module,
                        RunOptions::default()
                            .with_guest_process_mode(GuestProcessMode::Virtual)
                            .with_policy_file(policy),
                    )
                    .unwrap();
                assert_eq!(tree.wait(pid).unwrap(), RunOutcome::Completed);
            }
            _ => panic!("unknown case {case}"),
        }
    }
}
