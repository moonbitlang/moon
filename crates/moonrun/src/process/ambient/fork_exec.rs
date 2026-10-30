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

//! Linux single-child executable search and older glibc cwd fallback.

use std::ffi::{CString, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use super::Executable;
use crate::resource::ResourceRef;

// This fallback takes the same native spawn inputs as the Ambient Process job.
// Wrapping them would duplicate that representation only to satisfy Clippy.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn(
    executable: Executable,
    args: Vec<OsString>,
    env: Vec<OsString>,
    stdio: [Option<ResourceRef>; 3],
    cwd: Option<OsString>,
    child_signal_mask: libc::sigset_t,
    inherited_policy_fd: Option<libc::c_int>,
) -> Result<libc::pid_t, i32> {
    let mut stdio_fds = duplicate_stdio_fds(&stdio)?;
    let plan = match executable {
        Executable::Prepared(plan) => plan,
        Executable::Ambient(program) => {
            // This compatibility branch must emulate posix_spawnp's parent
            // PATH, not Command's search through the child's environment.
            // Keep relative paths relative: exec resolves them after child
            // chdir, even when the inherited directory has been unlinked.
            super::super::lookup::unix(&program, std::env::var_os("PATH").as_deref(), None)
                .map_err(|error| match error {
                    super::super::lookup::LookupError::EmptyProgram => libc::ENOENT,
                    super::super::lookup::LookupError::InvalidInput => libc::EINVAL,
                })?
        }
    };
    let prepared_exec = PreparedExec::new(&plan, &args, &env)?;

    // Command supplies fork, setup, and error reporting only: pre_exec always
    // execs or returns an error. PreparedExec owns the raw argv/envp, including
    // duplicate environment entries and entries without '=' accepted by the
    // native spawn ABI. Do not reinterpret them as a Command environment map.
    let mut command = Command::new(&plan.candidates()[0]);
    command.env_clear();
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    if let Some(stdin) = stdio_fds[0].take() {
        command.stdin(Stdio::from(stdin));
    }
    if let Some(stdout) = stdio_fds[1].take() {
        command.stdout(Stdio::from(stdout));
    }
    if let Some(stderr) = stdio_fds[2].take() {
        command.stderr(Stdio::from(stderr));
    }

    let mut default_action = unsafe { std::mem::zeroed::<libc::sigaction>() };
    default_action.sa_sigaction = libc::SIG_DFL;
    if unsafe { libc::sigemptyset(&mut default_action.sa_mask) } != 0 {
        return Err(last_errno());
    }
    let min_realtime_signal = libc::SIGRTMIN();
    let max_signal = libc::SIGRTMAX();

    // SAFETY: the closure only calls async-signal-safe functions, touches data
    // captured by value before fork, and constructs errors without allocation.
    unsafe {
        command.pre_exec(move || {
            if let Some(fd) = inherited_policy_fd {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::from_raw_os_error(last_errno()));
                }
            }
            for signal in 1..=max_signal {
                if signal == libc::SIGKILL || signal == libc::SIGSTOP {
                    continue;
                }
                if libc::sigaction(signal, &default_action, std::ptr::null_mut()) != 0 {
                    let error = last_errno();
                    // The libc thread implementation reserves signals below
                    // SIGRTMIN and makes sigaction reject them with EINVAL.
                    if error != libc::EINVAL || signal >= min_realtime_signal {
                        return Err(io::Error::from_raw_os_error(error));
                    }
                }
            }

            if libc::sigprocmask(libc::SIG_SETMASK, &child_signal_mask, std::ptr::null_mut()) != 0 {
                return Err(io::Error::from_raw_os_error(last_errno()));
            }
            // Candidates and pointer arrays were prepared before fork. Execute
            // here so Command never performs a second PATH search.
            prepared_exec.exec()
        });
    }

    // Command invokes pre_exec only after fork. Block signals in the calling
    // worker first so the child inherits a fully blocked mask from its first
    // instruction. spawn() does not return until the child has either execed
    // or reported a setup error, so restoring the worker mask afterwards
    // cannot race with the pre_exec hook.
    let spawn_result = {
        let _mask_guard = SignalMaskGuard::block_all()?;
        command.spawn()
    };
    let child = spawn_result.map_err(io_error_code)?;
    Ok(child.id() as libc::pid_t)
}

fn duplicate_stdio_fds(stdio: &[Option<ResourceRef>; 3]) -> Result<[Option<OwnedFd>; 3], i32> {
    let mut duplicates = std::array::from_fn(|_| None);
    for (index, resource) in stdio.iter().enumerate() {
        let Some(resource) = resource else {
            continue;
        };

        // Command takes ownership of its Stdio descriptors. Keep the
        // Resource-owned descriptor untouched and give Command an independent
        // CLOEXEC duplicate. Requiring a descriptor >= 3 also prevents the
        // child's ordered dup2(0), dup2(1), dup2(2) setup from overwriting a
        // later stream's source descriptor.
        let fd = unsafe {
            libc::fcntl(
                resource
                    .as_file()
                    .map_err(|error| error.errno())?
                    .as_raw_fd(),
                libc::F_DUPFD_CLOEXEC,
                3,
            )
        };
        if fd < 0 {
            return Err(last_errno());
        }
        // SAFETY: fcntl returned a new descriptor owned by this function.
        duplicates[index] = Some(unsafe { OwnedFd::from_raw_fd(fd) });
    }
    Ok(duplicates)
}

struct PreparedExec {
    candidates: Vec<CString>,
    is_search: bool,
    _argv_storage: Vec<CString>,
    argv: Vec<*const libc::c_char>,
    _env_storage: Vec<CString>,
    envp: Vec<*const libc::c_char>,
}

// SAFETY: the raw pointers refer to immutable CString buffers owned by the
// same value. Moving PreparedExec does not move those heap allocations,
// and the pointer arrays remain read-only until execve consumes them.
unsafe impl Send for PreparedExec {}
unsafe impl Sync for PreparedExec {}

impl PreparedExec {
    fn new(
        executable: &super::super::lookup::LookupPlan,
        args: &[OsString],
        env: &[OsString],
    ) -> Result<Self, i32> {
        let candidates = executable
            .candidates()
            .iter()
            .map(|path| CString::new(path.as_os_str().as_bytes()).map_err(|_| libc::EINVAL))
            .collect::<Result<Vec<_>, _>>()?;

        let argv_storage = args
            .iter()
            .map(|arg| CString::new(arg.as_bytes()).map_err(|_| libc::EINVAL))
            .collect::<Result<Vec<_>, _>>()?;
        let mut argv = argv_storage
            .iter()
            .map(|arg| arg.as_ptr())
            .collect::<Vec<_>>();
        argv.push(std::ptr::null());

        let env_storage = env
            .iter()
            .map(|entry| CString::new(entry.as_bytes()).map_err(|_| libc::EINVAL))
            .collect::<Result<Vec<_>, _>>()?;
        let mut envp = env_storage
            .iter()
            .map(|entry| entry.as_ptr())
            .collect::<Vec<_>>();
        envp.push(std::ptr::null());

        Ok(Self {
            candidates,
            is_search: executable.is_search(),
            _argv_storage: argv_storage,
            argv,
            _env_storage: env_storage,
            envp,
        })
    }

    fn exec(&self) -> io::Result<()> {
        let mut saw_eacces = false;
        let mut last_error = libc::ENOENT;
        for candidate in &self.candidates {
            unsafe {
                libc::execve(candidate.as_ptr(), self.argv.as_ptr(), self.envp.as_ptr());
            }
            last_error = last_errno();
            if !self.is_search {
                return Err(io::Error::from_raw_os_error(last_error));
            }
            match last_error {
                libc::EACCES => saw_eacces = true,
                libc::ENOENT | libc::ENOTDIR => {}
                #[cfg(target_env = "gnu")]
                libc::ESTALE | libc::ENODEV | libc::ETIMEDOUT => {}
                error => return Err(io::Error::from_raw_os_error(error)),
            }
        }
        Err(io::Error::from_raw_os_error(if saw_eacces {
            libc::EACCES
        } else {
            last_error
        }))
    }
}

struct SignalMaskGuard {
    old_mask: libc::sigset_t,
}

impl SignalMaskGuard {
    fn block_all() -> Result<Self, i32> {
        let mut all_signals = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        if unsafe { libc::sigfillset(&mut all_signals) } != 0 {
            return Err(last_errno());
        }

        let mut old_mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        let error =
            unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &all_signals, &mut old_mask) };
        if error != 0 {
            return Err(error);
        }
        Ok(Self { old_mask })
    }
}

impl Drop for SignalMaskGuard {
    fn drop(&mut self) {
        let error = unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.old_mask, std::ptr::null_mut())
        };
        if error != 0 {
            // Continuing with every signal blocked would corrupt the worker's
            // cancellation and completion behavior.
            std::process::abort();
        }
    }
}

fn io_error_code(error: io::Error) -> i32 {
    error.raw_os_error().unwrap_or_else(|| match error.kind() {
        io::ErrorKind::InvalidInput => libc::EINVAL,
        io::ErrorKind::NotFound => libc::ENOENT,
        io::ErrorKind::PermissionDenied => libc::EACCES,
        _ => libc::EIO,
    })
}

fn last_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::io::Read;
    use std::os::fd::FromRawFd;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;
    use std::sync::Arc;

    use super::*;
    use crate::resource::Resource;

    fn prepared(program: &OsStr, path: Option<&OsStr>, cwd: &std::path::Path) -> Executable {
        Executable::Prepared(super::super::super::lookup::unix(program, path, Some(cwd)).unwrap())
    }

    #[test]
    fn preserves_the_native_environment_array() {
        const CHILD: &str = "MOONRUN_RAW_ENV_CHILD";
        let env = vec![
            OsString::from("MOONRUN_RAW_ENV_CHILD=1"),
            OsString::from("MOONRUN_DUP=first"),
            OsString::from("MOONRUN_DUP=second"),
            // Native inherited environ can contain entries without '='. The
            // async C environment copier preserves these entries as well.
            OsString::from("MOONRUN_BARE"),
            OsString::from("=empty-key"),
        ];
        if std::env::var_os(CHILD).is_some() {
            unsafe extern "C" {
                static environ: *const *const libc::c_char;
            }
            // Inspect the raw array after exec, in an isolated test process
            // with no environment writers. env(1) and Rust's vars_os() can
            // filter entries, so neither is an oracle for the spawn ABI.
            let mut actual = Vec::new();
            unsafe {
                let mut entry = environ;
                while !(*entry).is_null() {
                    actual.push(
                        OsStr::from_bytes(std::ffi::CStr::from_ptr(*entry).to_bytes()).to_owned(),
                    );
                    entry = entry.add(1);
                }
            }
            assert_eq!(actual, env);
            std::process::exit(0);
        }
        let program = std::env::current_exe().unwrap();
        let args = vec![
            program.as_os_str().to_owned(),
            "--exact".into(),
            "process::ambient::fork_exec::tests::preserves_the_native_environment_array".into(),
            "--nocapture".into(),
        ];
        let tmp = tempfile::tempdir().unwrap();
        for executable in [
            None,
            Some(Executable::Ambient(program.as_os_str().to_owned())),
            Some(prepared(program.as_os_str(), None, tmp.path())),
        ] {
            let pid = match executable {
                None => native_spawn(program.as_os_str(), &args, &env, None),
                Some(executable) => spawn(
                    executable,
                    args.clone(),
                    env.clone(),
                    [None, None, None],
                    None,
                    empty_signal_mask(),
                    None,
                ),
            }
            .unwrap();
            assert_eq!(wait(pid), 0);
        }
    }

    #[test]
    fn accepts_an_empty_argv_like_native_spawn() {
        // The public async process API inserts cmd as argv[0]. The raw ABI can
        // still supply an empty array; Linux accepts it without synthesizing
        // the executable path in this layer.
        let native = wait(native_spawn(OsStr::new("/bin/true"), &[], &[], None).unwrap());
        assert_eq!(native, 0);
        let tmp = tempfile::tempdir().unwrap();
        for executable in [
            Executable::Ambient("/bin/true".into()),
            prepared(OsStr::new("/bin/true"), None, tmp.path()),
        ] {
            let pid = spawn(
                executable,
                Vec::new(),
                Vec::new(),
                [None, None, None],
                None,
                empty_signal_mask(),
                None,
            )
            .unwrap();
            assert_eq!(wait(pid), native);
        }
    }

    #[test]
    fn returns_native_enoexec_without_starting_a_shell() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("no-shebang");
        std::fs::write(&script, "exit 23\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let args = vec![script.as_os_str().to_owned()];
        let native = native_spawn(script.as_os_str(), &args, &[], None);
        assert_eq!(native, Err(libc::ENOEXEC));
        for executable in [
            Executable::Ambient(script.as_os_str().to_owned()),
            prepared(script.as_os_str(), None, tmp.path()),
            prepared(
                OsStr::new("no-shebang"),
                Some(tmp.path().as_os_str()),
                tmp.path(),
            ),
        ] {
            assert_eq!(
                spawn(
                    executable,
                    args.clone(),
                    Vec::new(),
                    [None, None, None],
                    None,
                    empty_signal_mask(),
                    None,
                ),
                native
            );
        }
    }

    #[test]
    fn ambient_spawn_does_not_require_a_name_for_the_current_directory() {
        if std::env::var_os("MOONRUN_UNLINKED_CWD_FALLBACK").is_none() {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process::ambient::fork_exec::tests::ambient_spawn_does_not_require_a_name_for_the_current_directory",
                ])
                .env("MOONRUN_UNLINKED_CWD_FALLBACK", "1")
                .env("PATH", "../bin")
                .timeout(std::time::Duration::from_secs(15))
                .assert()
                .success();
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let cwd = directory.path().join("removed");
        let bin = directory.path().join("bin");
        std::fs::create_dir(&cwd).unwrap();
        std::fs::create_dir(&bin).unwrap();
        symlink("/bin/true", bin.join("moonrun-true")).unwrap();
        std::env::set_current_dir(&cwd).unwrap();
        std::fs::remove_dir(&cwd).unwrap();
        assert!(std::env::current_dir().is_err());
        for program in ["/bin/true", "moonrun-true"] {
            let pid = spawn(
                Executable::Ambient(program.into()),
                vec![program.into()],
                vec!["PATH=/not-the-parent-path".into()],
                [None, None, None],
                Some(".".into()),
                empty_signal_mask(),
                None,
            )
            .unwrap();
            let status = wait(pid);
            assert!(libc::WIFEXITED(status));
            assert_eq!(libc::WEXITSTATUS(status), 0);
        }
    }

    #[test]
    fn retries_frozen_candidates_but_preserves_direct_exec_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("first");
        let second = tmp.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        for (directory, contents) in [
            (&first, "#!/moonrun-missing-interpreter\nexit 99\n"),
            (&second, "#!/bin/sh\nexit 23\n"),
        ] {
            let script = directory.join("program");
            std::fs::write(&script, contents).unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::join_paths([&first, &second]).unwrap();
        for program in ["program", "first/program"] {
            let executable = prepared(OsStr::new(program), Some(&path), tmp.path());
            let result = spawn(
                executable,
                vec!["program".into()],
                vec!["PATH=/not-the-frozen-search-path".into()],
                [None, None, None],
                Some(tmp.path().as_os_str().to_owned()),
                empty_signal_mask(),
                None,
            );
            if program == "program" {
                let status = wait(result.unwrap());
                assert!(libc::WIFEXITED(status));
                assert_eq!(libc::WEXITSTATUS(status), 23);
            } else {
                assert_eq!(result, Err(libc::ENOENT));
            }
        }
    }

    #[test]
    fn uses_parent_path_for_lookup_and_honors_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        symlink("/bin/sh", tmp.path().join("moonrun-sh")).unwrap();
        let mut pipe = [-1; 2];
        assert_eq!(
            unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        let mut reader = unsafe { std::fs::File::from_raw_fd(pipe[0]) };
        let stdout = Arc::new(Resource::new(pipe[1]));

        let pid = spawn(
            prepared(
                OsStr::new("moonrun-sh"),
                Some(tmp.path().as_os_str()),
                tmp.path(),
            ),
            vec![
                OsString::from("moonrun-sh"),
                OsString::from("-c"),
                OsString::from("printf '%s|%s' \"$(pwd)\" \"$PATH\""),
            ],
            vec![OsString::from("PATH=/child/path")],
            [None, Some(stdout.clone()), None],
            Some(tmp.path().as_os_str().to_owned()),
            empty_signal_mask(),
            None,
        )
        .unwrap();
        assert!(unsafe { libc::fcntl(stdout.as_file().unwrap().as_raw_fd(), libc::F_GETFD) } >= 0);
        drop(stdout);

        let mut output = String::new();
        reader.read_to_string(&mut output).unwrap();
        let status = wait(pid);
        assert!(libc::WIFEXITED(status));
        assert_eq!(libc::WEXITSTATUS(status), 0);
        assert_eq!(output, format!("{}|/child/path", tmp.path().display()));
    }

    #[test]
    fn restores_parent_mask_and_sets_child_mask() {
        let tmp = tempfile::tempdir().unwrap();
        let mut blocked = empty_signal_mask();
        assert_eq!(unsafe { libc::sigaddset(&mut blocked, libc::SIGUSR1) }, 0);
        let mut old_mask = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, &mut old_mask) },
            0
        );
        let restore_mask = ThreadSignalMaskRestore(old_mask);

        let spawn_result = spawn(
            prepared(OsStr::new("/bin/sh"), None, tmp.path()),
            vec![
                OsString::from("/bin/sh"),
                OsString::from("-c"),
                OsString::from("kill -USR1 $$; exit 99"),
            ],
            Vec::new(),
            [None, None, None],
            Some(tmp.path().as_os_str().to_owned()),
            empty_signal_mask(),
            None,
        );
        let mut current_mask = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe {
                libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut current_mask)
            },
            0
        );
        assert_eq!(
            unsafe { libc::sigismember(&current_mask, libc::SIGUSR1) },
            1
        );
        drop(restore_mask);

        let status = wait(spawn_result.unwrap());
        assert!(libc::WIFSIGNALED(status));
        assert_eq!(libc::WTERMSIG(status), libc::SIGUSR1);
    }

    #[test]
    fn reports_exec_error_without_consuming_stdio() {
        let tmp = tempfile::tempdir().unwrap();
        let mut pipe = [-1; 2];
        assert_eq!(
            unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        let _reader = unsafe { std::fs::File::from_raw_fd(pipe[0]) };
        let stdout = Arc::new(Resource::new(pipe[1]));

        let error = spawn(
            prepared(OsStr::new("/moonrun-does-not-exist"), None, tmp.path()),
            vec![OsString::from("/moonrun-does-not-exist")],
            Vec::new(),
            [None, Some(stdout.clone()), None],
            Some(tmp.path().as_os_str().to_owned()),
            empty_signal_mask(),
            None,
        )
        .unwrap_err();

        assert_eq!(error, libc::ENOENT);
        assert!(unsafe { libc::fcntl(stdout.as_file().unwrap().as_raw_fd(), libc::F_GETFD) } >= 0);
    }

    #[test]
    fn passes_argv_environment_and_shared_stdio() {
        let tmp = tempfile::tempdir().unwrap();
        let mut pipe = [-1; 2];
        assert_eq!(
            unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        let mut reader = unsafe { std::fs::File::from_raw_fd(pipe[0]) };
        let stdout = Arc::new(Resource::new(pipe[1]));

        let pid = spawn(
            prepared(OsStr::new("/bin/sh"), None, tmp.path()),
            vec![
                OsString::from("moonrun-argv-zero"),
                OsString::from("-c"),
                OsString::from("printf '%s|%s' \"$0\" \"$MOONRUN_TEST_ENV\"; printf '|stderr' >&2"),
            ],
            vec![
                OsString::from("MOONRUN_TEST_ENV=old"),
                OsString::from("MOONRUN_TEST_ENV=expected"),
            ],
            [None, Some(stdout.clone()), Some(stdout.clone())],
            Some(tmp.path().as_os_str().to_owned()),
            empty_signal_mask(),
            None,
        )
        .unwrap();
        assert!(unsafe { libc::fcntl(stdout.as_file().unwrap().as_raw_fd(), libc::F_GETFD) } >= 0);
        drop(stdout);

        let mut output = String::new();
        reader.read_to_string(&mut output).unwrap();
        let status = wait(pid);
        assert!(libc::WIFEXITED(status));
        assert_eq!(libc::WEXITSTATUS(status), 0);
        assert_eq!(output, "moonrun-argv-zero|expected|stderr");
    }

    fn empty_signal_mask() -> libc::sigset_t {
        let mut mask = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::sigemptyset(&mut mask) }, 0);
        mask
    }

    // An independent libc reference: pass the supplied argv/envp without
    // Command's argument or environment interpretation.
    fn native_spawn(
        program: &OsStr,
        args: &[OsString],
        env: &[OsString],
        stdout: Option<libc::c_int>,
    ) -> Result<libc::pid_t, i32> {
        let program = CString::new(program.as_bytes()).unwrap();
        let args = args
            .iter()
            .map(|arg| CString::new(arg.as_bytes()).unwrap())
            .collect::<Vec<_>>();
        let env = env
            .iter()
            .map(|entry| CString::new(entry.as_bytes()).unwrap())
            .collect::<Vec<_>>();
        let argv = args
            .iter()
            .map(|arg| arg.as_ptr().cast_mut())
            .chain(std::iter::once(std::ptr::null_mut()))
            .collect::<Vec<_>>();
        let envp = env
            .iter()
            .map(|entry| entry.as_ptr().cast_mut())
            .chain(std::iter::once(std::ptr::null_mut()))
            .collect::<Vec<_>>();
        let mut actions = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe { libc::posix_spawn_file_actions_init(&mut actions) },
            0
        );
        if let Some(stdout) = stdout {
            assert_eq!(
                unsafe { libc::posix_spawn_file_actions_adddup2(&mut actions, stdout, 1) },
                0
            );
        }
        let mut pid = 0;
        let spawn_fn = if program.as_bytes().contains(&b'/') {
            libc::posix_spawn
        } else {
            libc::posix_spawnp
        };
        let error = unsafe {
            spawn_fn(
                &mut pid,
                program.as_ptr(),
                &actions,
                std::ptr::null(),
                argv.as_ptr(),
                envp.as_ptr(),
            )
        };
        assert_eq!(
            unsafe { libc::posix_spawn_file_actions_destroy(&mut actions) },
            0
        );
        if error == 0 { Ok(pid) } else { Err(error) }
    }

    fn wait(pid: libc::pid_t) -> libc::c_int {
        let mut status = 0;
        assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
        status
    }

    struct ThreadSignalMaskRestore(libc::sigset_t);

    impl Drop for ThreadSignalMaskRestore {
        fn drop(&mut self) {
            let error =
                unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut()) };
            if error != 0 {
                std::process::abort();
            }
        }
    }
}
