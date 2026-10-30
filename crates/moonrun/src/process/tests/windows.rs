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

use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;
use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};

use super::{Env, Job, SpawnOptions, WorkingDirectory, host_process};
use crate::resource::ResourcePublication;

#[test]
fn fixed_runtime_cwd_is_the_executable_lookup_caller() {
    const TEST: &str = "process::tests::windows::fixed_runtime_cwd_is_the_executable_lookup_caller";
    const FIXTURE: &str = "MOONRUN_WINDOWS_LOOKUP_FIXTURE";
    const EXPECTED_CWD: &str = "MOONRUN_WINDOWS_LOOKUP_EXPECTED_CWD";
    let images = [
        ("host", "moonrun-cwd-probe.exe", 11),
        ("host/tools", "moonrun-path-probe.exe", 12),
        ("runtime", "moonrun-cwd-probe.exe", 21),
        ("runtime/tools", "moonrun-path-probe.exe", 22),
        ("runtime/owned-tools", "moonrun-path-probe.exe", 23),
        ("runtime/child", "moonrun-cwd-probe.exe", 31),
        ("runtime/child/tools", "moonrun-path-probe.exe", 32),
    ];

    if let Some(expected_cwd) = std::env::var_os(EXPECTED_CWD) {
        assert_eq!(
            dunce::canonicalize(std::env::current_dir().unwrap()).unwrap(),
            dunce::canonicalize(expected_cwd).unwrap()
        );
        let root = PathBuf::from(std::env::var_os(FIXTURE).unwrap());
        let image = dunce::canonicalize(std::env::current_exe().unwrap()).unwrap();
        let exit = images
            .iter()
            .find_map(|(directory, name, code)| {
                (image == root.join(directory).join(name)).then_some(*code)
            })
            .expect("the child must execute one of the fixture images");
        std::process::exit(exit);
    }

    let Some(root) = std::env::var_os(FIXTURE) else {
        let directory = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        let executable = std::env::current_exe().unwrap();
        for (parent, name, _) in images {
            let parent = root.join(parent);
            std::fs::create_dir_all(&parent).unwrap();
            std::fs::copy(&executable, parent.join(name)).unwrap();
        }
        // Set native caller state in an isolated process, without mutating the
        // test runner's shared environment or cwd. The original executable's
        // application directory contains none of the uniquely named probes.
        snapbox::cmd::Command::new(executable)
            .args(["--exact", TEST, "--nocapture"])
            .env(FIXTURE, &root)
            .env("PATH", "tools")
            .env_remove("NoDefaultCurrentDirectoryInExePath")
            .current_dir(root.join("host"))
            .timeout(std::time::Duration::from_secs(60))
            .assert()
            .success();
        return;
    };

    let root = PathBuf::from(root);
    let runtime = root.join("runtime");
    let child = runtime.join("child");
    let cases = [
        (
            "fixed ambient env",
            WorkingDirectory::Fixed(runtime.clone()),
            Env::ambient(),
            runtime.clone(),
            [21, 22, 22],
        ),
        (
            "fixed owned env",
            WorkingDirectory::Fixed(runtime.clone()),
            Env::owned([("PATH".into(), "owned-tools".into())]).unwrap(),
            runtime.clone(),
            [21, 23, 22],
        ),
        (
            "fully ambient control",
            WorkingDirectory::Ambient,
            Env::ambient(),
            root.join("host"),
            [11, 12, 12],
        ),
    ];
    for (label, cwd, env, inherited_cwd, expected) in cases {
        let fixed = matches!(cwd, WorkingDirectory::Fixed(_));
        let process = host_process(cwd, env);
        for override_cwd in [false, true] {
            let child_cwd = override_cwd.then(|| {
                if fixed {
                    OsString::from("child")
                } else {
                    child.clone().into_os_string()
                }
            });
            // Preserve the system environment needed to load a native image,
            // while giving the child a PATH that must not drive caller lookup.
            let child_env = Env::owned(std::env::vars_os()).unwrap();
            child_env
                .set("PATH".into(), child.join("tools").into_os_string())
                .unwrap();
            child_env
                .set(
                    EXPECTED_CWD.into(),
                    if override_cwd { &child } else { &inherited_cwd }
                        .as_os_str()
                        .to_owned(),
                )
                .unwrap();
            let mut entries = child_env
                .entries()
                .into_iter()
                .map(|(name, value)| {
                    (
                        name.encode_wide().collect::<Vec<_>>(),
                        value.encode_wide().collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>();
            // CreateProcessW expects case-insensitive ordinal name order.
            entries.sort_by(|(left, _), (right, _)| {
                let order = unsafe {
                    CompareStringOrdinal(
                        left.as_ptr(),
                        left.len().try_into().unwrap(),
                        right.as_ptr(),
                        right.len().try_into().unwrap(),
                        1,
                    )
                };
                assert_ne!(order, 0);
                order.cmp(&CSTR_EQUAL)
            });
            let env_block = entries
                .into_iter()
                .flat_map(|(name, value)| {
                    name.into_iter()
                        .chain([b'=' as u16])
                        .chain(value)
                        .chain([0])
                })
                .chain([0])
                .collect::<Vec<_>>();
            for (program, expected_exit) in [
                "moonrun-cwd-probe.exe",
                "moonrun-path-probe.exe",
                ".\\tools\\moonrun-path-probe.exe",
            ]
            .into_iter()
            .zip(expected)
            {
                let mut spawn = Job::spawn_windows(
                    format!("{program} --exact {TEST} --nocapture").into(),
                    env_block.clone(),
                    None,
                    None,
                    None,
                    child_cwd.clone(),
                    SpawnOptions {
                        no_console_window: true,
                        is_orphan: true,
                    },
                );
                process.prepare_job(&mut spawn).unwrap();
                let pid = spawn.run().unwrap();
                process.finish_job(&spawn, pid, 0).unwrap();
                let Some(ResourcePublication::Unpublished(handle)) =
                    spawn.take_spawn_result().unwrap()
                else {
                    panic!("a native Windows child must return its process handle");
                };
                let mut wait = process
                    .wait_job(Some(Arc::new(handle)), None, pid as i32)
                    .unwrap();
                process.prepare_job(&mut wait).unwrap();
                let exit = wait.run().unwrap();
                process.finish_job(&wait, exit, 0).unwrap();
                assert_eq!(
                    exit, expected_exit,
                    "{label}: {program}, child cwd={child_cwd:?}"
                );
            }
        }
    }
}
