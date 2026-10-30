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

//! Native operating-system process execution.
//!
//! This is the Ambient implementation behind the process module. It preserves
//! native lookup for Ambient requests and consumes explicit lookup plans for
//! owned environments. The parent module owns policy and child provenance.

use std::ffi::OsString;
#[cfg(unix)]
use std::ffi::{CStr, CString};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, AsRawSocket};

use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::thread_pool::with_cancellable_region;
use crate::async_sys::internal::fd_util;
use crate::async_sys::ported_fns;
use crate::policy::PolicyInheritance;
#[cfg(any(windows, target_os = "linux"))]
use crate::resource::Resource;
use crate::resource::ResourceRef;

use super::job::SpawnOptions;
use crate::resource::ResourcePublication;

#[cfg(target_os = "linux")]
mod fork_exec;
pub(super) mod lookup;

#[cfg(unix)]
#[derive(Debug)]
pub(super) enum Executable {
    Ambient(OsString),
    Prepared(super::lookup::LookupPlan),
}

#[cfg(windows)]
#[derive(Debug)]
pub(super) enum Executable {
    Ambient,
    Prepared(std::path::PathBuf),
}

type RawFile = fd_util::stub::RawFd;

ported_fns! {
    #[ported(
        runtime = "thread_pool.c",
        original = "spawn_job_worker"
    )]
    #[allow(clippy::too_many_arguments)]
    #[cfg(unix)]
    pub(super) fn run_spawn_job_unix(
        executable: Executable,
        args: Vec<OsString>,
        env: Vec<OsString>,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        options: SpawnOptions,
        policy_inheritance: Option<PolicyInheritance>,
        result: &mut Option<ResourcePublication>,
    ) -> AsyncHostResult<i64> {
        spawn_process_unix(
            executable,
            args,
            env,
            stdio,
            cwd,
            options,
            policy_inheritance,
            result,
        )
    }

    #[ported(
        runtime = "thread_pool.c",
        original = "spawn_job_worker"
    )]
    #[allow(clippy::too_many_arguments)]
    #[cfg(windows)]
    pub(super) fn run_spawn_job_windows(
        executable: Executable,
        command_line: OsString,
        env: Vec<u16>,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        options: SpawnOptions,
        policy_inheritance: Option<PolicyInheritance>,
        result: &mut Option<ResourcePublication>,
    ) -> AsyncHostResult<i64> {
        spawn_process_windows(
            executable,
            command_line,
            env,
            stdio,
            cwd,
            options,
            policy_inheritance,
            result,
        )
    }

    #[ported(
        runtime = "thread_pool.c",
        original = "wait_for_process_job_worker"
    )]
    pub(super) fn run_wait_for_process_job(
        handle: Option<ResourceRef>,
        pid: i32,
        #[cfg(unix)] defer_reap: bool,
        #[cfg(windows)] cancel: Option<ResourceRef>,
    ) -> AsyncHostResult<i64> {
        wait_for_process(
            handle,
            pid,
            #[cfg(unix)]
            defer_reap,
            #[cfg(windows)]
            cancel,
        )
    }
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
fn spawn_process_unix(
    executable: Executable,
    args: Vec<OsString>,
    mut env: Vec<OsString>,
    stdio: [Option<ResourceRef>; 3],
    cwd: Option<OsString>,
    options: SpawnOptions,
    policy_inheritance: Option<PolicyInheritance>,
    result: &mut Option<ResourcePublication>,
) -> AsyncHostResult<i64> {
    #[cfg(not(target_os = "linux"))]
    let _ = result;

    // Creating the anonymous policy file can touch temporary storage, so it
    // belongs to the spawn job. Failure occurs before OS process creation and
    // is returned through this job's errno.
    let policy_transfer = match policy_inheritance {
        Some(inheritance) => Some(inheritance.open_transfer().map_err(policy_transfer_error)?),
        None => None,
    };
    let policy_transfer_fd = policy_transfer
        .as_ref()
        .map(|transfer| transfer.as_raw_fd());
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    let posix_spawn_addchdir = match cwd.as_ref() {
        Some(_) => resolve_posix_spawn_addchdir(),
        None => None,
    };

    // Linux posix_spawn creates a child even when exec fails. Search in one
    // forked child instead of creating a new process for every PATH candidate.
    #[cfg(target_os = "linux")]
    let needs_fork_exec = matches!(&executable, Executable::Prepared(plan) if plan.is_search());
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    let needs_fork_exec = needs_fork_exec || (cwd.is_some() && posix_spawn_addchdir.is_none());
    #[cfg(target_os = "linux")]
    if needs_fork_exec {
        if let Some(fd) = policy_transfer_fd {
            crate::async_sys::process::overwrite_process_env_var(
                &mut env,
                std::ffi::OsStr::new(moonutil::constants::MOONRUN_INHERITED_POLICY),
                std::ffi::OsStr::new(&fd.to_string()),
            );
        }
        let pid = fork_exec::spawn(
            executable,
            args,
            env,
            stdio,
            cwd,
            options.child_signal_mask,
            policy_transfer_fd,
        )
        .map_err(AsyncHostError::Native)?;
        if let Ok(pidfd) = crate::async_sys::process::open_pid_handle(pid) {
            *result = Some(ResourcePublication::Unpublished(Resource::new(pidfd)));
        }
        return Ok(i64::from(pid));
    }

    let policy_transfer_child = if let Some(fd) = policy_transfer_fd {
        let child_fd = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
        if child_fd < 0 {
            return Err(last_native_error());
        }
        // Reserve a distinct descriptor number in the parent. The spawn
        // action below replaces this duplicate in the child, which both keeps
        // the number race-free and gives the child a non-CLOEXEC descriptor.
        Some(unsafe { std::fs::File::from_raw_fd(child_fd) })
    } else {
        None
    };
    if let Some(file) = policy_transfer_child.as_ref() {
        crate::async_sys::process::overwrite_process_env_var(
            &mut env,
            std::ffi::OsStr::new(moonutil::constants::MOONRUN_INHERITED_POLICY),
            std::ffi::OsStr::new(&file.as_raw_fd().to_string()),
        );
    }

    let (paths, native_lookup, search) = match executable {
        Executable::Ambient(program) => {
            let path = unix_cstring(program)?;
            let search = !path.as_bytes().contains(&b'/');
            (vec![path], true, search)
        }
        Executable::Prepared(plan) => {
            let paths = plan
                .candidates()
                .iter()
                .map(|path| unix_cstring(path.as_os_str().to_owned()))
                .collect::<AsyncHostResult<Vec<_>>>()?;
            (paths, false, plan.is_search())
        }
    };
    let mut argv_storage = Vec::with_capacity(args.len());
    for arg in args {
        argv_storage.push(unix_cstring(arg)?);
    }
    let mut argv = argv_storage
        .iter()
        .map(|arg| arg.as_ptr().cast_mut())
        .collect::<Vec<_>>();
    argv.push(std::ptr::null_mut());

    let env_storage = unix_env(env)?;
    let mut envp = env_storage
        .iter()
        .map(|entry| entry.as_ptr().cast_mut())
        .collect::<Vec<_>>();
    envp.push(std::ptr::null_mut());

    let cwd = cwd.map(unix_cstring).transpose()?;
    let stdio_fds = duplicate_stdio_fds(&stdio)?;

    let mut attr = unsafe { std::mem::zeroed::<libc::posix_spawnattr_t>() };
    let mut file_actions = unsafe { std::mem::zeroed::<libc::posix_spawn_file_actions_t>() };
    let mut attr_initialized = false;
    let mut file_actions_initialized = false;

    let spawn_result = (|| -> Result<libc::pid_t, i32> {
        check_spawn_errno(unsafe { libc::posix_spawnattr_init(&mut attr) })?;
        attr_initialized = true;

        let flags = (libc::POSIX_SPAWN_SETSIGMASK | libc::POSIX_SPAWN_SETSIGDEF) as libc::c_short;
        check_spawn_errno(unsafe { libc::posix_spawnattr_setflags(&mut attr, flags) })?;

        check_spawn_errno(unsafe {
            libc::posix_spawnattr_setsigmask(&mut attr, &options.child_signal_mask)
        })?;

        let mut all_signals = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        if unsafe { libc::sigfillset(&mut all_signals) } != 0 {
            return Err(last_native_errno());
        }
        check_spawn_errno(unsafe { libc::posix_spawnattr_setsigdefault(&mut attr, &all_signals) })?;

        check_spawn_errno(unsafe { libc::posix_spawn_file_actions_init(&mut file_actions) })?;
        file_actions_initialized = true;
        for (target, fd) in stdio_fds.iter().enumerate() {
            if let Some(fd) = fd {
                check_spawn_errno(unsafe {
                    libc::posix_spawn_file_actions_adddup2(
                        &mut file_actions,
                        *fd,
                        target as libc::c_int,
                    )
                })?;
            }
        }
        if let (Some(source), Some(target)) = (policy_transfer_fd, policy_transfer_child.as_ref()) {
            check_spawn_errno(unsafe {
                libc::posix_spawn_file_actions_adddup2(
                    &mut file_actions,
                    source,
                    target.as_raw_fd(),
                )
            })?;
        }
        if let Some(cwd) = cwd.as_ref() {
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            check_spawn_errno(invoke_add_chdir_file_action(
                posix_spawn_addchdir,
                &mut file_actions,
                cwd.as_c_str(),
            ))?;
            #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
            check_spawn_errno(add_chdir_file_action(&mut file_actions, cwd.as_c_str()))?;
        }

        let mut saw_eacces = false;
        #[cfg(not(target_os = "macos"))]
        let mut last_error = libc::ENOENT;
        for path in &paths {
            let mut pid = 0;
            let spawn = if native_lookup && search {
                libc::posix_spawnp
            } else {
                libc::posix_spawn
            };
            let error = unsafe {
                spawn(
                    &mut pid,
                    path.as_ptr(),
                    &file_actions,
                    &attr,
                    argv.as_ptr(),
                    envp.as_ptr(),
                )
            };
            if error == 0 {
                return Ok(pid);
            }
            if native_lookup || !search {
                return Err(error);
            }
            // Darwin's PATH search interprets executable text files with sh.
            // Direct paths retain posix_spawn's ENOEXEC result on every OS.
            #[cfg(target_os = "macos")]
            if error == libc::ENOEXEC {
                let mut shell_argv = Vec::with_capacity(argv_storage.len() + 3);
                shell_argv.push(
                    argv_storage
                        .first()
                        .map_or(c"sh".as_ptr(), |arg| arg.as_ptr())
                        .cast_mut(),
                );
                shell_argv.push(path.as_ptr().cast_mut());
                shell_argv.extend(
                    argv_storage
                        .iter()
                        .skip(1)
                        .map(|arg| arg.as_ptr().cast_mut()),
                );
                shell_argv.push(std::ptr::null_mut());
                check_spawn_errno(unsafe {
                    libc::posix_spawn(
                        &mut pid,
                        c"/bin/sh".as_ptr(),
                        &file_actions,
                        &attr,
                        shell_argv.as_ptr(),
                        envp.as_ptr(),
                    )
                })?;
                return Ok(pid);
            }
            // Retry only after attempting the actual image. An access check
            // would miss failures such as a script's unavailable interpreter.
            #[cfg(target_os = "macos")]
            match error {
                libc::E2BIG | libc::ENOMEM | libc::ETXTBSY => return Err(error),
                libc::ELOOP | libc::ENAMETOOLONG | libc::ENOENT | libc::ENOTDIR => {}
                error => {
                    // Darwin distinguishes an inaccessible PATH directory
                    // from an existing image that failed to execute.
                    let mut metadata = unsafe { std::mem::zeroed::<libc::stat>() };
                    if unsafe { libc::stat(path.as_ptr(), &mut metadata) } == 0 {
                        if error != libc::EACCES {
                            return Err(error);
                        }
                        saw_eacces = true;
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                match error {
                    libc::EACCES => saw_eacces = true,
                    libc::ENOENT
                    | libc::ENOTDIR
                    | libc::ESTALE
                    | libc::ENODEV
                    | libc::ETIMEDOUT => {}
                    error => return Err(error),
                }
                last_error = error;
            }
        }
        #[cfg(target_os = "macos")]
        let last_error = libc::ENOENT;
        Err(if saw_eacces { libc::EACCES } else { last_error })
    })();

    if attr_initialized {
        unsafe {
            libc::posix_spawnattr_destroy(&mut attr);
        }
    }
    if file_actions_initialized {
        unsafe {
            libc::posix_spawn_file_actions_destroy(&mut file_actions);
        }
    }
    close_stdio_fds(&stdio_fds);

    match spawn_result {
        Ok(pid) => {
            #[cfg(target_os = "linux")]
            if let Ok(pidfd) = crate::async_sys::process::open_pid_handle(pid) {
                *result = Some(ResourcePublication::Unpublished(Resource::new(pidfd)));
            }
            Ok(i64::from(pid))
        }
        Err(error) => Err(AsyncHostError::Native(error)),
    }
}

#[cfg(unix)]
fn unix_cstring(value: OsString) -> AsyncHostResult<CString> {
    use std::os::unix::ffi::OsStrExt;

    CString::new(value.as_os_str().as_bytes()).map_err(|_| AsyncHostError::Inval)
}

#[cfg(unix)]
fn unix_env(env: Vec<OsString>) -> AsyncHostResult<Vec<CString>> {
    let mut entries = Vec::new();
    for entry in env {
        entries.push(unix_cstring(entry)?);
    }
    Ok(entries)
}

#[cfg(unix)]
fn duplicate_stdio_fds(stdio: &[Option<ResourceRef>; 3]) -> AsyncHostResult<[Option<RawFile>; 3]> {
    let mut fds = [None, None, None];
    for (index, resource) in stdio.iter().enumerate() {
        let Some(resource) = resource else {
            continue;
        };
        let fd = unsafe { libc::fcntl(resource.as_file()?.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if fd < 0 {
            let error = last_native_error();
            close_stdio_fds(&fds);
            return Err(error);
        }
        fds[index] = Some(fd);
    }
    Ok(fds)
}

#[cfg(unix)]
fn close_stdio_fds(fds: &[Option<RawFile>; 3]) {
    for fd in fds.iter().flatten() {
        unsafe {
            libc::close(*fd);
        }
    }
}

#[cfg(unix)]
fn check_spawn_errno(ret: libc::c_int) -> Result<(), i32> {
    if ret == 0 { Ok(()) } else { Err(ret) }
}

#[cfg(target_os = "linux")]
type PosixSpawnAddChdir =
    unsafe extern "C" fn(*mut libc::posix_spawn_file_actions_t, *const libc::c_char) -> libc::c_int;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn resolve_posix_spawn_addchdir() -> Option<PosixSpawnAddChdir> {
    static ADDCHDIR: std::sync::OnceLock<Option<PosixSpawnAddChdir>> = std::sync::OnceLock::new();

    *ADDCHDIR.get_or_init(|| {
        // glibc exposed this as an extension before the chdir action was
        // standardized without the `_np` suffix.
        for name in [
            c"posix_spawn_file_actions_addchdir",
            c"posix_spawn_file_actions_addchdir_np",
        ] {
            let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
            if !symbol.is_null() {
                // POSIX specifies that a symbol returned by dlsym may be
                // converted to the corresponding function pointer.
                return Some(unsafe {
                    std::mem::transmute::<*mut libc::c_void, PosixSpawnAddChdir>(symbol)
                });
            }
        }
        None
    })
}

#[cfg(all(target_os = "linux", not(target_env = "gnu")))]
fn add_chdir_file_action(
    file_actions: *mut libc::posix_spawn_file_actions_t,
    cwd: &CStr,
) -> libc::c_int {
    static ADD_CHDIR: std::sync::OnceLock<Option<PosixSpawnAddChdir>> = std::sync::OnceLock::new();
    let add_chdir = *ADD_CHDIR.get_or_init(|| {
        let symbol = unsafe {
            libc::dlsym(
                libc::RTLD_DEFAULT,
                c"posix_spawn_file_actions_addchdir_np".as_ptr(),
            )
        };
        if symbol.is_null() {
            None
        } else {
            Some(unsafe { std::mem::transmute::<*mut libc::c_void, PosixSpawnAddChdir>(symbol) })
        }
    });
    invoke_add_chdir_file_action(add_chdir, file_actions, cwd)
}

#[cfg(target_os = "linux")]
fn invoke_add_chdir_file_action(
    add_chdir: Option<PosixSpawnAddChdir>,
    file_actions: *mut libc::posix_spawn_file_actions_t,
    cwd: &CStr,
) -> libc::c_int {
    match add_chdir {
        Some(add_chdir) => unsafe { add_chdir(file_actions, cwd.as_ptr()) },
        None => libc::ENOSYS,
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn posix_spawn_file_actions_addchdir_np(
        file_actions: *mut libc::posix_spawn_file_actions_t,
        cwd: *const libc::c_char,
    ) -> libc::c_int;
}

#[cfg(target_os = "macos")]
fn add_chdir_file_action(
    file_actions: *mut libc::posix_spawn_file_actions_t,
    cwd: &CStr,
) -> libc::c_int {
    unsafe { posix_spawn_file_actions_addchdir_np(file_actions, cwd.as_ptr()) }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn add_chdir_file_action(
    _file_actions: *mut libc::posix_spawn_file_actions_t,
    _cwd: &CStr,
) -> libc::c_int {
    libc::ENOSYS
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn spawn_process_windows(
    executable: Executable,
    command_line: OsString,
    mut env_block: Vec<u16>,
    stdio: [Option<ResourceRef>; 3],
    cwd: Option<OsString>,
    options: SpawnOptions,
    policy_inheritance: Option<PolicyInheritance>,
    result: &mut Option<ResourcePublication>,
) -> AsyncHostResult<i64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
        CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
        InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
        PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES,
        STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
    };

    let application = match executable {
        Executable::Ambient => None,
        Executable::Prepared(path) => {
            let mut application = path.as_os_str().encode_wide().collect::<Vec<_>>();
            if application.is_empty() || application.contains(&0) {
                return Err(AsyncHostError::Inval);
            }
            application.push(0);
            Some(application)
        }
    };
    let mut command_line = command_line.encode_wide().collect::<Vec<_>>();
    // These are Win32 strings, as in async's native stubs. Leave empty input
    // and embedded terminators to CreateProcessW; in particular the public
    // async cwd parameter can contain NUL. Interpreted executable names are
    // validated separately, before selecting an explicit application image.
    command_line.push(0);
    let cwd = cwd.map(|cwd| cwd.encode_wide().chain([0]).collect::<Vec<_>>());

    // As on Unix, materialize the transfer on the worker before duplicating
    // child handles. A failure therefore cannot leave a child or inherited
    // handle behind.
    let policy_transfer = match policy_inheritance {
        Some(inheritance) => Some(inheritance.open_transfer().map_err(policy_transfer_error)?),
        None => None,
    };
    let job_object = if options.is_orphan {
        None
    } else {
        global_job_object()?
    };

    let mut inherited_handles =
        Vec::with_capacity(stdio.len() + usize::from(policy_transfer.is_some()));
    let mut stdio_handles = [std::ptr::null_mut(); 3];
    for (slot, resource) in stdio_handles.iter_mut().zip(&stdio) {
        let Some(resource) = resource else { continue };
        let raw = match spawn_stdio_handle(resource) {
            Ok(raw) => raw,
            Err(error) => {
                close_handles(&inherited_handles);
                return Err(error);
            }
        };
        match duplicate_inheritable_handle(raw) {
            Ok(handle) => {
                *slot = handle;
                inherited_handles.push(handle);
            }
            Err(error) => {
                close_handles(&inherited_handles);
                return Err(error);
            }
        }
    }

    if let Some(transfer) = policy_transfer.as_ref() {
        let handle = match duplicate_inheritable_handle(transfer.as_raw_handle()) {
            Ok(handle) => handle,
            Err(error) => {
                close_handles(&inherited_handles);
                return Err(error);
            }
        };
        crate::async_sys::process::overwrite_process_env_var(
            &mut env_block,
            std::ffi::OsStr::new(moonutil::constants::MOONRUN_INHERITED_POLICY),
            std::ffi::OsStr::new(&(handle as usize).to_string()),
        );
        inherited_handles.push(handle);
    }

    let mut startup_info = unsafe { std::mem::zeroed::<STARTUPINFOEXW>() };
    startup_info.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup_info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup_info.StartupInfo.hStdInput = stdio_handles[0];
    startup_info.StartupInfo.hStdOutput = stdio_handles[1];
    startup_info.StartupInfo.hStdError = stdio_handles[2];

    let attr_count = if job_object.is_some() { 2 } else { 1 };
    let mut attrs_size = 0;
    unsafe {
        InitializeProcThreadAttributeList(std::ptr::null_mut(), attr_count, 0, &mut attrs_size);
    }
    let mut attrs = vec![0usize; attrs_size.div_ceil(std::mem::size_of::<usize>())];
    startup_info.lpAttributeList = attrs.as_mut_ptr().cast();
    if unsafe {
        InitializeProcThreadAttributeList(
            startup_info.lpAttributeList,
            attr_count,
            0,
            &mut attrs_size,
        )
    } == 0
    {
        let error = last_native_error();
        close_handles(&inherited_handles);
        return Err(error);
    }

    // Win32 rejects an empty explicit handle list with ERROR_BAD_LENGTH.
    if !inherited_handles.is_empty()
        && unsafe {
            UpdateProcThreadAttribute(
                startup_info.lpAttributeList,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                inherited_handles.as_ptr().cast(),
                inherited_handles.len() * std::mem::size_of::<HANDLE>(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
    {
        let error = last_native_error();
        unsafe {
            DeleteProcThreadAttributeList(startup_info.lpAttributeList);
        }
        close_handles(&inherited_handles);
        return Err(error);
    }

    let job_list = job_object.map(|job_object| [job_object as HANDLE]);
    let job_list_attr_used = if let Some(job_list) = job_list.as_ref() {
        (unsafe {
            UpdateProcThreadAttribute(
                startup_info.lpAttributeList,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                job_list.as_ptr().cast(),
                std::mem::size_of_val(job_list),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        }) != 0
    } else {
        false
    };

    let mut process_info = unsafe { std::mem::zeroed::<PROCESS_INFORMATION>() };
    let mut create_flags =
        CREATE_NEW_PROCESS_GROUP | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT;
    if options.no_console_window {
        create_flags |= CREATE_NO_WINDOW;
    }
    if job_object.is_some() && !job_list_attr_used {
        create_flags |= CREATE_SUSPENDED;
    }
    let created = unsafe {
        CreateProcessW(
            application
                .as_ref()
                .map_or(std::ptr::null(), |path| path.as_ptr()),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            i32::from(!inherited_handles.is_empty()),
            create_flags,
            env_block.as_ptr().cast(),
            cwd.as_ref().map_or(std::ptr::null(), |cwd| cwd.as_ptr()),
            (&raw mut startup_info.StartupInfo).cast(),
            &mut process_info,
        )
    };
    let create_error = if created == 0 {
        Some(last_native_error())
    } else {
        None
    };

    unsafe {
        DeleteProcThreadAttributeList(startup_info.lpAttributeList);
    }
    close_handles(&inherited_handles);

    if let Some(error) = create_error {
        return Err(error);
    }

    if let Some(job_object) = job_object
        && !job_list_attr_used
    {
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        if unsafe { AssignProcessToJobObject(job_object, process_info.hProcess) } == 0 {
            let error = last_native_error();
            unsafe {
                TerminateProcess(process_info.hProcess, 1);
                CloseHandle(process_info.hThread);
                CloseHandle(process_info.hProcess);
            }
            return Err(error);
        }
        if unsafe { ResumeThread(process_info.hThread) } == u32::MAX {
            let error = last_native_error();
            unsafe {
                TerminateProcess(process_info.hProcess, 1);
                CloseHandle(process_info.hThread);
                CloseHandle(process_info.hProcess);
            }
            return Err(error);
        }
    }

    unsafe {
        CloseHandle(process_info.hThread);
    }
    *result = Some(ResourcePublication::Unpublished(Resource::new(
        process_info.hProcess,
    )));
    Ok(i64::from(process_info.dwProcessId))
}

#[cfg(windows)]
fn spawn_stdio_handle(resource: &Resource) -> AsyncHostResult<RawFile> {
    if resource.resource_class().is_socket() {
        // Windows permits sockets in STARTUPINFO standard-handle fields and
        // supports inheriting them as handles. Keep that ABI conversion at the
        // process-spawn seam without representing sockets as file handles.
        Ok(resource.as_socket()?.as_raw_socket() as RawFile)
    } else {
        Ok(resource.as_file()?.as_raw_handle())
    }
}

#[cfg(windows)]
fn global_job_object() -> AsyncHostResult<Option<RawFile>> {
    static GLOBAL_JOB_OBJECT: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    static GLOBAL_JOB_OBJECT_INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    if let Some(handle) = GLOBAL_JOB_OBJECT.get() {
        return Ok(handle.map(|handle| handle as RawFile));
    }

    let _init = GLOBAL_JOB_OBJECT_INIT.lock().unwrap();
    if let Some(handle) = GLOBAL_JOB_OBJECT.get() {
        return Ok(handle.map(|handle| handle as RawFile));
    }

    let handle = create_global_job_object()?;
    let _ = GLOBAL_JOB_OBJECT.set(handle.map(|handle| handle as usize));
    Ok(handle)
}

#[cfg(windows)]
fn create_global_job_object() -> AsyncHostResult<Option<RawFile>> {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED};
    use windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let job = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
    if job.is_null() {
        return Err(last_native_error());
    }

    let mut info = unsafe { std::mem::zeroed::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_BREAKAWAY_OK
        | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK
        | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

    if unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const info).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        let error = last_native_error();
        unsafe {
            CloseHandle(job);
        }
        return Err(error);
    }

    if unsafe {
        windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(job, GetCurrentProcess())
    } == 0
    {
        let error = last_native_error();
        unsafe {
            CloseHandle(job);
        }
        if error.errno() == ERROR_ACCESS_DENIED as i32 {
            return Ok(None);
        }
        return Err(error);
    }

    Ok(Some(job))
}

#[cfg(unix)]
fn wait_for_process(
    handle: Option<ResourceRef>,
    pid: i32,
    defer_reap: bool,
) -> AsyncHostResult<i64> {
    #[cfg(not(target_os = "linux"))]
    let _ = handle;

    #[cfg(target_os = "linux")]
    if let Some(handle) = handle {
        return wait_for_process_pidfd(handle.as_fd()?.as_raw_fd(), defer_reap);
    }
    wait_for_process_pid(pid, defer_reap)
}

#[cfg(target_os = "linux")]
fn wait_for_process_pidfd(pidfd: RawFile, defer_reap: bool) -> AsyncHostResult<i64> {
    with_cancellable_region(|| {
        let mut siginfo = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
        // Policy mode reaps only after atomically revoking PID authority.
        let flags = libc::WEXITED | if defer_reap { libc::WNOWAIT } else { 0 };
        if unsafe { libc::waitid(libc::P_PIDFD, pidfd as libc::id_t, &mut siginfo, flags) } < 0 {
            return Err(last_native_error());
        }
        Ok(i64::from(
            crate::async_sys::process::unix_siginfo_exit_code(siginfo.si_code, unsafe {
                siginfo.si_status()
            }),
        ))
    })
}

#[cfg(unix)]
fn wait_for_process_pid(pid: i32, defer_reap: bool) -> AsyncHostResult<i64> {
    with_cancellable_region(|| {
        if !defer_reap {
            let mut status = 0;
            let ret = unsafe { libc::waitpid(pid, &mut status, 0) };
            if ret < 0 {
                return Err(last_native_error());
            }
            if ret != pid {
                return Err(AsyncHostError::Inval);
            }
            return Ok(i64::from(
                crate::async_sys::process::unix_wait_status_exit_code(status),
            ));
        }
        let mut siginfo = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
        // The host reaps after atomically revoking policy PID authority.
        if unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut siginfo,
                libc::WEXITED | libc::WNOWAIT,
            )
        } < 0
        {
            return Err(last_native_error());
        }
        if unsafe { siginfo.si_pid() } != pid {
            return Err(AsyncHostError::Inval);
        }
        Ok(i64::from(
            crate::async_sys::process::unix_siginfo_exit_code(siginfo.si_code, unsafe {
                siginfo.si_status()
            }),
        ))
    })
}

#[cfg(windows)]
pub(super) fn make_wait_for_process_cancel() -> AsyncHostResult<Resource> {
    use windows_sys::Win32::System::Threading::CreateEventW;

    let cancel = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    if cancel.is_null() {
        Err(last_native_error())
    } else {
        Ok(Resource::new(cancel))
    }
}

#[cfg(windows)]
pub(super) fn cancel_wait_for_process(cancel: &ResourceRef) -> AsyncHostResult<()> {
    use windows_sys::Win32::System::Threading::SetEvent;

    if unsafe { SetEvent(cancel.as_handle()?.as_raw_handle()) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn wait_for_process(
    handle: Option<ResourceRef>,
    pid: i32,
    cancel: Option<ResourceRef>,
) -> AsyncHostResult<i64> {
    let cancel = cancel.ok_or(AsyncHostError::Badf)?;
    if let Some(handle) = handle {
        return wait_for_process_handle(handle.as_handle()?.as_raw_handle(), &cancel);
    }

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let handle = unsafe {
        OpenProcess(
            SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid as u32,
        )
    };
    if handle.is_null() {
        return Err(last_native_error());
    }
    let result = wait_for_process_handle(handle, &cancel);
    unsafe {
        CloseHandle(handle);
    }
    result
}

#[cfg(windows)]
fn wait_for_process_handle(handle: RawFile, cancel: &ResourceRef) -> AsyncHostResult<i64> {
    use windows_sys::Win32::Foundation::{
        ERROR_OPERATION_ABORTED, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, INFINITE, WaitForMultipleObjects, WaitForSingleObject,
    };

    let handles = [handle, cancel.as_handle()?.as_raw_handle()];
    let wait = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
    if wait == WAIT_OBJECT_0 + 1 {
        match unsafe { WaitForSingleObject(handle, 0) } {
            WAIT_OBJECT_0 => {}
            WAIT_TIMEOUT => return Err(AsyncHostError::Native(ERROR_OPERATION_ABORTED as i32)),
            WAIT_FAILED => return Err(last_native_error()),
            _ => return Err(last_native_error()),
        }
    } else if wait == WAIT_FAILED || wait != WAIT_OBJECT_0 {
        return Err(last_native_error());
    }
    let mut code = 0;
    if unsafe { GetExitCodeProcess(handle, &mut code) } == 0 {
        return Err(last_native_error());
    }
    Ok(i64::from(code))
}

#[cfg(windows)]
fn duplicate_inheritable_handle(raw: RawFile) -> AsyncHostResult<RawFile> {
    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let process = unsafe { GetCurrentProcess() };
    let mut duplicate: HANDLE = std::ptr::null_mut();
    if unsafe {
        DuplicateHandle(
            process,
            raw,
            process,
            &mut duplicate,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        Err(last_native_error())
    } else {
        Ok(duplicate)
    }
}

#[cfg(windows)]
fn close_handles(handles: &[RawFile]) {
    use windows_sys::Win32::Foundation::CloseHandle;

    for handle in handles {
        unsafe {
            CloseHandle(*handle);
        }
    }
}

fn policy_transfer_error(error: std::io::Error) -> AsyncHostError {
    error
        .raw_os_error()
        .map_or(AsyncHostError::Io, AsyncHostError::Native)
}

#[cfg(windows)]
fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(unsafe { windows_sys::Win32::Foundation::GetLastError() as i32 })
}

#[cfg(unix)]
fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_native_errno())
}

#[cfg(target_os = "linux")]
fn last_native_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

#[cfg(target_os = "macos")]
fn last_native_errno() -> i32 {
    unsafe { *libc::__error() }
}

#[cfg(test)]
mod policy_transfer_error_tests {
    use super::*;

    #[test]
    fn preserves_native_errors_for_the_spawn_job() {
        assert_eq!(
            policy_transfer_error(std::io::Error::from_raw_os_error(1234)),
            AsyncHostError::Native(1234)
        );
    }

    #[test]
    fn reports_non_native_transfer_failures_as_io() {
        assert_eq!(
            policy_transfer_error(std::io::Error::other("policy transfer failed")),
            AsyncHostError::Io
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn prepared(program: &OsStr, path: Option<&OsStr>, cwd: &Path) -> Executable {
        Executable::Prepared(super::super::lookup::unix(program, path, Some(cwd)).unwrap())
    }

    fn spawn_for_test(
        executable: Executable,
        args: Vec<OsString>,
        cwd: &Path,
    ) -> AsyncHostResult<i64> {
        let mut child_signal_mask = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::sigemptyset(&mut child_signal_mask) }, 0);
        run_spawn_job_unix(
            executable,
            args,
            vec!["PATH=/not-the-frozen-search-path".into()],
            [None, None, None],
            Some(cwd.as_os_str().to_owned()),
            SpawnOptions { child_signal_mask },
            None,
            &mut None,
        )
    }

    #[test]
    fn frozen_search_retries_after_an_unavailable_interpreter() {
        #[cfg(target_os = "linux")]
        {
            if std::env::var_os("MOONRUN_SINGLE_FORK_SEARCH").is_none() {
                // atfork callbacks cannot be unregistered. Keep the process
                // counter isolated from unrelated tests and their children.
                snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "process::ambient::tests::frozen_search_retries_after_an_unavailable_interpreter",
                    ])
                    .env("MOONRUN_SINGLE_FORK_SEARCH", "1")
                    .timeout(std::time::Duration::from_secs(15))
                    .assert()
                    .success();
                return;
            }
        }
        #[cfg(target_os = "linux")]
        static FORKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        #[cfg(target_os = "linux")]
        unsafe extern "C" fn count_fork() {
            FORKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let path = std::env::join_paths([&first, &second]).unwrap();
        // Freeze lookup before either executable exists. Only execution can
        // decide that the first image has no usable interpreter.
        let executable = prepared(OsStr::new("program"), Some(&path), directory.path());
        for (target, contents) in [
            (
                first.join("program"),
                "#!/moonrun-missing-interpreter\nexit 99\n",
            ),
            (second.join("program"), "#!/bin/sh\nexit 23\n"),
        ] {
            std::fs::write(&target, contents).unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(target_os = "linux")]
        assert_eq!(
            unsafe { libc::pthread_atfork(None, Some(count_fork), None) },
            0
        );
        let pid = spawn_for_test(executable, vec!["program".into()], directory.path()).unwrap();
        assert_eq!(run_wait_for_process_job(None, pid as i32, false), Ok(23));
        #[cfg(target_os = "linux")]
        assert_eq!(FORKS.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn ambient_lookup_uses_parent_path_and_child_working_directory() {
        if std::env::var_os("MOONRUN_AMBIENT_LOOKUP_TEST").is_none() {
            let parent_directory = tempfile::tempdir().unwrap();
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process::ambient::tests::ambient_lookup_uses_parent_path_and_child_working_directory",
                ])
                .env("MOONRUN_AMBIENT_LOOKUP_TEST", "1")
                .env("PATH", "tools")
                .current_dir(parent_directory.path())
                .timeout(std::time::Duration::from_secs(15))
                .assert()
                .success();
            return;
        }
        let parent_directory = std::env::current_dir().unwrap();
        let directory = tempfile::tempdir().unwrap();
        for (root, exit) in [(parent_directory.as_path(), 23), (directory.path(), 42)] {
            let tools = root.join("tools");
            std::fs::create_dir(&tools).unwrap();
            let script = tools.join("program");
            std::fs::write(&script, format!("#!/bin/sh\nexit {exit}\n")).unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // Compare the native operation directly. HostProcess virtualization
        // has separate tests: a child chdir alone cannot implement every
        // platform's caller-cwd behavior.
        let cwd = unix_cstring(directory.path().as_os_str().to_owned()).unwrap();
        let mut actions = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe { libc::posix_spawn_file_actions_init(&mut actions) },
            0
        );
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let setup =
            invoke_add_chdir_file_action(resolve_posix_spawn_addchdir(), &mut actions, &cwd);
        #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
        let setup = add_chdir_file_action(&mut actions, &cwd);
        if setup == libc::ENOSYS {
            // Older libc has no native chdir action to use as the reference;
            // the fork_exec unit tests cover its compatibility implementation.
            unsafe {
                libc::posix_spawn_file_actions_destroy(&mut actions);
            }
            return;
        }
        assert_eq!(setup, 0);
        let mut pid = 0;
        let argv = [c"program".as_ptr().cast_mut(), std::ptr::null_mut()];
        let envp = [
            c"PATH=/not-the-parent-path".as_ptr().cast_mut(),
            std::ptr::null_mut(),
        ];
        let error = unsafe {
            libc::posix_spawnp(
                &mut pid,
                c"program".as_ptr(),
                &actions,
                std::ptr::null(),
                argv.as_ptr(),
                envp.as_ptr(),
            )
        };
        unsafe {
            libc::posix_spawn_file_actions_destroy(&mut actions);
        }
        assert_eq!(error, 0);
        let native = run_wait_for_process_job(None, pid, false).unwrap();
        assert!(matches!(native, 23 | 42));
        let pid = spawn_for_test(
            Executable::Ambient("program".into()),
            vec!["program".into()],
            directory.path(),
        )
        .unwrap();
        assert_eq!(
            run_wait_for_process_job(None, pid as i32, false),
            Ok(native)
        );
        // Successful relative lookup uses the child's directory with both
        // native Ambient cwd and a Fixed Runtime cwd.
        for (working_directory, child_cwd) in [
            (
                crate::WorkingDirectory::Fixed(directory.path().to_owned()),
                None,
            ),
            (
                crate::WorkingDirectory::Ambient,
                Some(directory.path().as_os_str().to_owned()),
            ),
        ] {
            let process = super::super::HostProcess::new(
                None,
                None,
                std::sync::Arc::new(working_directory),
                std::sync::Arc::new(crate::runtime::Stdio::Ambient),
                None,
                std::sync::Arc::new(crate::runtime::Env::ambient()),
            );
            let mut child_signal_mask = unsafe { std::mem::zeroed() };
            assert_eq!(unsafe { libc::sigemptyset(&mut child_signal_mask) }, 0);
            let mut job = super::super::Job::spawn_unix(
                "program".into(),
                vec!["program".into()],
                vec!["PATH=/not-the-parent-path".into()],
                None,
                None,
                None,
                child_cwd,
                SpawnOptions { child_signal_mask },
            );
            process.prepare_job(&mut job).unwrap();
            let pid = job.run().unwrap();
            process.finish_job(&job, pid, 0).unwrap();
            assert_eq!(
                run_wait_for_process_job(None, pid as i32, false),
                Ok(native)
            );
        }
        #[cfg(target_os = "linux")]
        {
            // Exercise the old-glibc cwd fallback even on a newer libc.
            let mut mask = unsafe { std::mem::zeroed() };
            assert_eq!(unsafe { libc::sigemptyset(&mut mask) }, 0);
            let pid = fork_exec::spawn(
                Executable::Ambient("program".into()),
                vec!["program".into()],
                vec!["PATH=/not-the-parent-path".into()],
                [None, None, None],
                Some(directory.path().as_os_str().to_owned()),
                mask,
                None,
            )
            .unwrap();
            assert_eq!(run_wait_for_process_job(None, pid, false), Ok(native));
        }
    }

    #[test]
    fn search_preserves_permission_errors_and_direct_paths_do_not_search() {
        let directory = tempfile::tempdir().unwrap();
        let denied = directory.path().join("program");
        std::fs::write(&denied, "#!/bin/sh\nexit 99\n").unwrap();
        std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o644)).unwrap();
        let path =
            std::env::join_paths([directory.path(), &directory.path().join("missing")]).unwrap();
        let executable = prepared(OsStr::new("program"), Some(&path), directory.path());
        assert_eq!(
            spawn_for_test(executable, vec!["program".into()], directory.path()),
            Err(AsyncHostError::Native(libc::EACCES))
        );
        let executable = prepared(
            OsStr::new("missing/program"),
            Some(directory.path().as_os_str()),
            directory.path(),
        );
        assert_eq!(
            spawn_for_test(executable, vec!["program".into()], directory.path()),
            Err(AsyncHostError::Native(libc::ENOENT))
        );
    }

    #[test]
    fn executable_text_keeps_the_platform_search_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("program");
        std::fs::write(&script, "exit \"$1\"\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        for program in ["./program", "program"] {
            let executable = prepared(
                OsStr::new(program),
                Some(directory.path().as_os_str()),
                directory.path(),
            );
            let result = spawn_for_test(
                executable,
                vec!["custom-argv-zero".into(), "23".into()],
                directory.path(),
            );
            if cfg!(target_os = "macos") && program == "program" {
                assert_eq!(
                    run_wait_for_process_job(None, result.unwrap() as i32, false),
                    Ok(23)
                );
            } else {
                assert_eq!(result, Err(AsyncHostError::Native(libc::ENOEXEC)));
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn missing_addchdir_api_reports_enosys() {
        assert_eq!(
            invoke_add_chdir_file_action(None, std::ptr::null_mut(), c"."),
            libc::ENOSYS
        );
    }

    #[cfg(unix)]
    #[test]
    fn wait_for_unknown_process_reports_native_error() {
        // The current process is not its own child and cannot consume a child
        // spawned by another test, unlike the `-1` waitpid wildcard.
        let non_child_pid = unsafe { libc::getpid() };
        let err = run_wait_for_process_job(None, non_child_pid, false).unwrap_err();
        assert_eq!(err, AsyncHostError::Native(libc::ECHILD));
    }

    #[test]
    fn wait_for_process_fallback_returns_negative_signal() {
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork failed: {}", last_native_errno());
        if pid == 0 {
            unsafe {
                libc::raise(libc::SIGTERM);
                libc::_exit(1);
            }
        }

        assert_eq!(
            run_wait_for_process_job(None, pid, false),
            Ok(-i64::from(libc::SIGTERM))
        );
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn ambient_spawn_preserves_native_string_handling() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{
            CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetExitCodeProcess,
            PROCESS_INFORMATION, STARTUPINFOW, WaitForSingleObject,
        };

        let directory = tempfile::tempdir().unwrap();
        let command = format!("\"{}\" /d /c exit 23", std::env::var("COMSPEC").unwrap());
        let mut terminated_cwd = directory.path().as_os_str().to_owned();
        terminated_cwd.push("\0ignored");
        for (command_line, cwd) in [
            (OsString::new(), None),
            (OsString::from(format!("{command}\0ignored")), None),
            (OsString::from(&command), Some(terminated_cwd)),
            (OsString::from(&command), Some(OsString::new())),
        ] {
            let mut native_command = command_line.encode_wide().chain([0]).collect::<Vec<_>>();
            let native_cwd = cwd
                .as_ref()
                .map(|cwd| cwd.encode_wide().chain([0]).collect::<Vec<_>>());
            let env = [0u16, 0];
            let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut native: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
            let created = unsafe {
                CreateProcessW(
                    std::ptr::null(),
                    native_command.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                    env.as_ptr().cast(),
                    native_cwd
                        .as_ref()
                        .map_or(std::ptr::null(), |cwd| cwd.as_ptr()),
                    &startup,
                    &mut native,
                )
            };
            let expected = if created == 0 {
                Err(AsyncHostError::Native(unsafe { GetLastError() } as i32))
            } else {
                assert_eq!(
                    unsafe { WaitForSingleObject(native.hProcess, 10_000) },
                    WAIT_OBJECT_0
                );
                let mut exit = 0;
                assert_ne!(unsafe { GetExitCodeProcess(native.hProcess, &mut exit) }, 0);
                unsafe {
                    CloseHandle(native.hThread);
                    CloseHandle(native.hProcess);
                }
                Ok(exit)
            };
            let mut result = None;
            let actual = spawn_process_windows(
                Executable::Ambient,
                command_line,
                vec![0, 0],
                [None, None, None],
                cwd,
                SpawnOptions {
                    no_console_window: true,
                    is_orphan: true,
                },
                None,
                &mut result,
            )
            .map(|_| {
                let Some(ResourcePublication::Unpublished(process)) = result else {
                    panic!("successful spawn publishes a process handle");
                };
                let handle = process.as_file().unwrap().as_raw_handle();
                assert_eq!(
                    unsafe { WaitForSingleObject(handle, 10_000) },
                    WAIT_OBJECT_0
                );
                let mut exit = 0;
                assert_ne!(unsafe { GetExitCodeProcess(handle, &mut exit) }, 0);
                exit
            });
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn spawn_without_standard_handles() {
        use std::os::windows::io::AsRawHandle;
        use std::sync::Arc;
        use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Console::{
            STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
        };
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};

        const CHILD: &str = "MOONRUN_TEST_ABSENT_STDIO";
        if std::env::var_os(CHILD).is_none() {
            // Changing process stdio must not affect other tests or libtest's
            // own output. Exercise both Win32 representations in isolated runs.
            for missing in ["null", "invalid"] {
                assert!(
                    std::process::Command::new(std::env::current_exe().unwrap())
                        .args([
                            "--exact",
                            "process::ambient::windows_tests::spawn_without_standard_handles"
                        ])
                        .env(CHILD, missing)
                        .status()
                        .unwrap()
                        .success()
                );
            }
            return;
        }
        let missing = if std::env::var(CHILD).unwrap() == "null" {
            std::ptr::null_mut()
        } else {
            INVALID_HANDLE_VALUE
        };
        for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            assert_ne!(unsafe { SetStdHandle(stream, missing) }, 0);
        }
        let process = super::super::HostProcess::new(
            None,
            None,
            Arc::new(crate::WorkingDirectory::Ambient),
            Arc::new(crate::runtime::Stdio::Ambient),
            None,
            Arc::new(crate::runtime::Env::ambient()),
        );

        // Cover an empty inheritance list and a partial stdio set, with and
        // without the child Job Object attribute.
        for is_orphan in [false, true] {
            for redirect_stdout in [false, true] {
                let output = tempfile::tempfile().unwrap();
                let stdout = redirect_stdout
                    .then(|| std::sync::Arc::new(Resource::stdio_file(output.as_raw_handle())));
                let command = std::env::var("COMSPEC").unwrap();
                let mut job = super::super::Job::spawn_windows(
                    format!("\"{command}\" /d /c exit 23").into(),
                    vec![0, 0],
                    None,
                    stdout,
                    None,
                    None,
                    SpawnOptions {
                        no_console_window: true,
                        is_orphan,
                    },
                );
                process.prepare_job(&mut job).unwrap();
                assert!(job.run().unwrap() > 0);
                let Some(ResourcePublication::Unpublished(process)) =
                    job.take_spawn_result().unwrap()
                else {
                    panic!("spawn must publish its process handle");
                };
                let handle = process.as_file().unwrap().as_raw_handle();
                assert_eq!(
                    unsafe { WaitForSingleObject(handle, 10_000) },
                    WAIT_OBJECT_0
                );
                let mut exit_code = 0;
                assert_ne!(unsafe { GetExitCodeProcess(handle, &mut exit_code) }, 0);
                assert_eq!(exit_code, 23);
            }
        }
    }

    #[test]
    fn spawn_stdio_accepts_socket_resource() {
        assert_eq!(crate::async_sys::internal::event_loop::io::init_wsa(), 0);
        let raw_socket = crate::async_sys::socket::make_tcp_socket(4).unwrap();
        let resource = Resource::tcp_socket(raw_socket, 4);

        assert_eq!(
            spawn_stdio_handle(&resource).unwrap() as usize,
            raw_socket as usize
        );

        drop(resource);
        assert_eq!(crate::async_sys::internal::event_loop::io::cleanup_wsa(), 0);
    }
}
