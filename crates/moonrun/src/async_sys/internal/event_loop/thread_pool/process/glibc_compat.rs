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

//! glibc fallback for process spawning when `posix_spawn` cannot change cwd.

use std::ffi::{CString, OsStr, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use super::ResourceRef;

pub(super) fn spawn_with_command(
    path: OsString,
    args: Vec<OsString>,
    env: Vec<OsString>,
    stdio: [Option<ResourceRef>; 3],
    cwd: OsString,
    child_signal_mask: libc::sigset_t,
    parent_path: Option<OsString>,
) -> Result<libc::pid_t, i32> {
    let mut stdio_fds = duplicate_stdio_fds(&stdio)?;
    let parent_path_search = ParentPathSearch::new(&path, &args, &env, parent_path.as_deref())?;

    let mut command = Command::new(path);
    if let Some((arg0, args)) = args.split_first() {
        command.arg0(arg0).args(args);
    }
    command.env_clear();
    for entry in env {
        let (key, value) = split_env_entry(&entry)?;
        command.env(key, value);
    }
    command.current_dir(cwd);
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
            for signal in 1..=max_signal {
                if signal == libc::SIGKILL || signal == libc::SIGSTOP {
                    continue;
                }
                if libc::sigaction(signal, &default_action, std::ptr::null_mut()) != 0 {
                    let error = last_errno();
                    // NPTL hides its reserved real-time signals below
                    // SIGRTMIN and makes sigaction reject them with EINVAL.
                    if error != libc::EINVAL || signal >= min_realtime_signal {
                        return Err(io::Error::from_raw_os_error(error));
                    }
                }
            }

            if libc::sigprocmask(libc::SIG_SETMASK, &child_signal_mask, std::ptr::null_mut()) != 0 {
                return Err(io::Error::from_raw_os_error(last_errno()));
            }
            // pre_exec runs after current_dir, so relative parent PATH entries
            // are interpreted from the child's cwd just like posix_spawnp.
            if let Some(search) = parent_path_search.as_ref() {
                return search.exec();
            }
            Ok(())
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

fn split_env_entry(entry: &OsStr) -> Result<(&OsStr, &OsStr), i32> {
    let bytes = entry.as_bytes();
    let separator = bytes
        .iter()
        .position(|byte| *byte == b'=')
        .ok_or(libc::EINVAL)?;
    Ok((
        OsStr::from_bytes(&bytes[..separator]),
        OsStr::from_bytes(&bytes[separator + 1..]),
    ))
}

struct ParentPathSearch {
    candidates: Vec<CString>,
    _argv_storage: Vec<CString>,
    argv: Vec<*const libc::c_char>,
    _env_storage: Vec<CString>,
    envp: Vec<*const libc::c_char>,
}

// SAFETY: the raw pointers refer to immutable CString buffers owned by the
// same value. Moving ParentPathSearch does not move those heap allocations,
// and the pointer arrays remain read-only until execve consumes them.
unsafe impl Send for ParentPathSearch {}
unsafe impl Sync for ParentPathSearch {}

impl ParentPathSearch {
    fn new(
        path: &OsStr,
        args: &[OsString],
        env: &[OsString],
        parent_path: Option<&OsStr>,
    ) -> Result<Option<Self>, i32> {
        if path.as_bytes().contains(&b'/') {
            return Ok(None);
        }
        if path.is_empty() {
            return Err(libc::ENOENT);
        }

        // glibc's posix_spawnp searches the calling process's PATH rather
        // than PATH from envp, and falls back to CS_PATH when it is unset.
        let parent_path = parent_path.unwrap_or_else(|| OsStr::new("/bin:/usr/bin"));
        let mut candidates = Vec::new();
        for directory in parent_path.as_bytes().split(|byte| *byte == b':') {
            let mut candidate = Vec::with_capacity(directory.len() + path.as_bytes().len() + 1);
            candidate.extend_from_slice(directory);
            if !directory.is_empty() {
                candidate.push(b'/');
            }
            candidate.extend_from_slice(path.as_bytes());
            candidates.push(CString::new(candidate).map_err(|_| libc::EINVAL)?);
        }

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

        Ok(Some(Self {
            candidates,
            _argv_storage: argv_storage,
            argv,
            _env_storage: env_storage,
            envp,
        }))
    }

    fn exec(&self) -> io::Result<()> {
        let mut saw_eacces = false;
        for candidate in &self.candidates {
            unsafe {
                libc::execve(candidate.as_ptr(), self.argv.as_ptr(), self.envp.as_ptr());
            }
            match last_errno() {
                libc::EACCES => saw_eacces = true,
                libc::ENOENT | libc::ESTALE | libc::ENOTDIR | libc::ENODEV | libc::ETIMEDOUT => {}
                error => return Err(io::Error::from_raw_os_error(error)),
            }
        }
        Err(io::Error::from_raw_os_error(if saw_eacces {
            libc::EACCES
        } else {
            libc::ENOENT
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
    use std::io::Read;
    use std::os::fd::FromRawFd;
    use std::os::unix::fs::symlink;
    use std::sync::Arc;

    use super::super::Resource;
    use super::*;

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

        let pid = spawn_with_command(
            OsString::from("moonrun-sh"),
            vec![
                OsString::from("moonrun-sh"),
                OsString::from("-c"),
                OsString::from("printf '%s|%s' \"$(pwd)\" \"$PATH\""),
            ],
            vec![OsString::from("PATH=/child/path")],
            [None, Some(stdout.clone()), None],
            tmp.path().as_os_str().to_owned(),
            empty_signal_mask(),
            Some(tmp.path().as_os_str().to_owned()),
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

        let spawn_result = spawn_with_command(
            OsString::from("/bin/sh"),
            vec![
                OsString::from("/bin/sh"),
                OsString::from("-c"),
                OsString::from("kill -USR1 $$; exit 99"),
            ],
            Vec::new(),
            [None, None, None],
            tmp.path().as_os_str().to_owned(),
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

        let error = spawn_with_command(
            OsString::from("/moonrun-does-not-exist"),
            vec![OsString::from("/moonrun-does-not-exist")],
            Vec::new(),
            [None, Some(stdout.clone()), None],
            tmp.path().as_os_str().to_owned(),
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

        let pid = spawn_with_command(
            OsString::from("/bin/sh"),
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
            tmp.path().as_os_str().to_owned(),
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
