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

//! Runtime state through the HostProcess interface. Native-primitive controls
//! live in ambient's tests; pure candidate rules live in lookup's tests.

#[cfg(windows)]
mod windows;

#[cfg(unix)]
use std::ffi::{OsStr, OsString};
use std::sync::Arc;

use super::{HostProcess, Job, SpawnOptions};
#[cfg(unix)]
use crate::async_host::AsyncHostResult;
use crate::runtime::{Env, Stdio, WorkingDirectory};

fn host_process(cwd: WorkingDirectory, env: Env) -> HostProcess {
    HostProcess::new(
        None,
        None,
        Arc::new(cwd),
        Arc::new(Stdio::Ambient),
        None,
        Arc::new(env),
    )
}

#[cfg(unix)]
fn run_unix(process: &HostProcess, program: &OsStr, cwd: Option<OsString>) -> AsyncHostResult<i64> {
    let mut child_signal_mask = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::sigemptyset(&mut child_signal_mask) }, 0);
    let mut job = Job::spawn_unix(
        program.to_owned(),
        vec![program.to_owned()],
        vec!["PATH=/not-the-caller-path".into()],
        None,
        None,
        None,
        cwd,
        SpawnOptions { child_signal_mask },
    );
    process.prepare_job(&mut job)?;
    let pid = job.run()?;
    process.finish_job(&job, pid, 0)?;
    Job::wait_for_process(None, None, pid as i32, false)?.run()
}

#[cfg(unix)]
#[test]
fn fixed_cwd_uses_the_child_directory_for_relative_executables() {
    use std::os::unix::fs::PermissionsExt;
    const CHILD: &str = "MOONRUN_FIXED_LOOKUP_TEST";
    if std::env::var_os(CHILD).is_none() {
        let host = tempfile::tempdir().unwrap();
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process::tests::fixed_cwd_uses_the_child_directory_for_relative_executables",
            ])
            .env(CHILD, "1")
            .env("PATH", "tools")
            .current_dir(host.path())
            .timeout(std::time::Duration::from_secs(15))
            .assert()
            .success();
        return;
    }
    let runtime = tempfile::tempdir().unwrap();
    let child = runtime.path().join("child");
    for (root, exit) in [(runtime.path(), 29), (child.as_path(), 43)] {
        let tools = root.join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let program = tools.join("program");
        std::fs::write(&program, format!("#!/bin/sh\nexit {exit}\n")).unwrap();
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    for env in [
        Env::ambient(),
        Env::owned([("PATH".into(), "tools".into())]).unwrap(),
    ] {
        let process = host_process(WorkingDirectory::Fixed(runtime.path().to_owned()), env);
        for program in [OsStr::new("program"), OsStr::new("./tools/program")] {
            assert_eq!(run_unix(&process, program, None), Ok(29));
            assert_eq!(run_unix(&process, program, Some("child".into())), Ok(43));
        }
    }
}

#[cfg(unix)]
#[test]
fn fixed_cwd_permission_failure_does_not_consult_the_host_directory() {
    use std::os::unix::fs::PermissionsExt;
    const CHILD: &str = "MOONRUN_FIXED_DENIED_TEST";
    if std::env::var_os(CHILD).is_none() {
        let host = tempfile::tempdir().unwrap();
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process::tests::fixed_cwd_permission_failure_does_not_consult_the_host_directory",
            ])
            .env(CHILD, "1")
            .env("PATH", "tools")
            .current_dir(host.path())
            .timeout(std::time::Duration::from_secs(15))
            .assert()
            .success();
        return;
    }
    let runtime = tempfile::tempdir().unwrap();
    let tools = runtime.path().join("tools");
    std::fs::create_dir(&tools).unwrap();
    let program = tools.join("program");
    std::fs::write(&program, "#!/bin/sh\nexit 99\n").unwrap();
    std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o644)).unwrap();
    for env in [
        Env::ambient(),
        Env::owned([("PATH".into(), "tools".into())]).unwrap(),
    ] {
        let process = host_process(WorkingDirectory::Fixed(runtime.path().to_owned()), env);
        assert_eq!(
            run_unix(&process, OsStr::new("program"), None),
            Err(crate::async_host::AsyncHostError::Native(libc::EACCES))
        );
    }
}

#[cfg(unix)]
#[test]
fn owned_env_does_not_require_a_cwd_pathname_for_absolute_lookup() {
    const CHILD: &str = "MOONRUN_OWNED_UNLINKED_CWD_TEST";
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir().unwrap();
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process::tests::owned_env_does_not_require_a_cwd_pathname_for_absolute_lookup",
            ])
            .env(CHILD, directory.path())
            .current_dir(directory.path())
            .timeout(std::time::Duration::from_secs(15))
            .assert()
            .success();
        return;
    }
    std::fs::remove_dir(std::env::var_os(CHILD).unwrap()).unwrap();
    assert!(std::env::current_dir().is_err());
    let process = host_process(
        WorkingDirectory::Ambient,
        Env::owned([("PATH".into(), "/usr/bin:/bin".into())]).unwrap(),
    );
    for program in [OsStr::new("/usr/bin/true"), OsStr::new("true")] {
        for cwd in [None, Some("/".into()), Some(".".into())] {
            assert_eq!(run_unix(&process, program, cwd), Ok(0));
        }
    }
}
