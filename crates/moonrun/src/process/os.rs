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

//! Native process mechanisms. Callers resolve authority and Runtime state,
//! retain input handles, and translate native results into their own contract.

use std::ffi::OsString;
#[cfg(unix)]
use std::ffi::{CStr, CString};
use std::io;
#[cfg(unix)]
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;

mod control;
#[cfg(target_os = "linux")]
mod fork_exec;
pub(super) mod lookup;
#[cfg(target_os = "linux")]
pub(crate) use control::{open_pidfd, try_wait_pidfd};
#[cfg(windows)]
pub(crate) use control::{
    open_process_for_wait, process_id, send_console_event, terminate_process, try_wait_handle,
};
#[cfg(unix)]
pub(crate) use control::{reap, send_signal, try_wait_pid};
#[cfg(unix)]
pub(super) type RawFile = std::os::fd::RawFd;
#[cfg(windows)]
pub(super) type RawFile = std::os::windows::io::RawHandle;

pub(super) struct Spawned {
    #[cfg(unix)]
    pub(super) pid: libc::pid_t,
    #[cfg(windows)]
    pub(super) pid: u32,
    #[cfg(target_os = "linux")]
    pub(super) handle: Option<OwnedFd>,
    #[cfg(windows)]
    pub(super) handle: OwnedHandle,
}

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

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpawnOptions {
    #[cfg(unix)]
    pub(crate) child_signal_mask: libc::sigset_t,
    #[cfg(windows)]
    pub(crate) no_console_window: bool,
    #[cfg(windows)]
    pub(crate) is_orphan: bool,
}

/// Inputs shared by the Unix spawn and fork/exec implementations.
#[cfg(unix)]
pub(super) struct Spawn<'a> {
    pub(super) executable: Executable,
    pub(super) args: Vec<OsString>,
    pub(super) env: Vec<OsString>,
    // Ambient stdio can name closed descriptors. Duplication validates them;
    // the caller retains any live inputs until spawn returns.
    pub(super) stdio: [Option<RawFile>; 3],
    pub(super) cwd: Option<OsString>,
    pub(super) child_signal_mask: libc::sigset_t,
    // Source and reserved duplicate (at least 3) of the same open file.
    // The child inherits the duplicate without CLOEXEC on either spawn path.
    pub(super) inherited: Option<(BorrowedFd<'a>, BorrowedFd<'a>)>,
}

#[cfg(unix)]
pub(super) fn spawn(request: Spawn<'_>) -> io::Result<Spawned> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    let posix_spawn_addchdir = match request.cwd.as_ref() {
        Some(_) => resolve_posix_spawn_addchdir(),
        None => None,
    };

    // Linux posix_spawn creates a child even when exec fails. Search in one
    // forked child instead of creating a new process for every PATH candidate.
    #[cfg(target_os = "linux")]
    let needs_fork_exec =
        matches!(&request.executable, Executable::Prepared(plan) if plan.is_search());
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    let needs_fork_exec =
        needs_fork_exec || (request.cwd.is_some() && posix_spawn_addchdir.is_none());
    #[cfg(target_os = "linux")]
    if needs_fork_exec {
        let pid = fork_exec::spawn(request).map_err(io::Error::from_raw_os_error)?;
        return Ok(Spawned {
            pid,
            handle: open_pidfd(pid).ok(),
        });
    }

    let Spawn {
        executable,
        args,
        env,
        stdio,
        cwd,
        child_signal_mask,
        inherited,
    } = request;

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
                .collect::<io::Result<Vec<_>>>()?;
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
            libc::posix_spawnattr_setsigmask(&mut attr, &child_signal_mask)
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
                        fd.as_raw_fd(),
                        target as libc::c_int,
                    )
                })?;
            }
        }
        if let Some((source, target)) = inherited {
            check_spawn_errno(unsafe {
                libc::posix_spawn_file_actions_adddup2(
                    &mut file_actions,
                    source.as_raw_fd(),
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
    drop(stdio_fds);

    let pid = spawn_result.map_err(io::Error::from_raw_os_error)?;
    Ok(Spawned {
        pid,
        #[cfg(target_os = "linux")]
        handle: open_pidfd(pid).ok(),
    })
}

#[cfg(unix)]
fn unix_cstring(value: OsString) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;

    CString::new(value.as_os_str().as_bytes()).map_err(|_| invalid_input())
}

#[cfg(unix)]
fn unix_env(env: Vec<OsString>) -> io::Result<Vec<CString>> {
    let mut entries = Vec::new();
    for entry in env {
        entries.push(unix_cstring(entry)?);
    }
    Ok(entries)
}

#[cfg(unix)]
fn duplicate_stdio_fds(stdio: &[Option<RawFile>; 3]) -> io::Result<[Option<OwnedFd>; 3]> {
    let mut duplicates = std::array::from_fn(|_| None);
    for (slot, source) in duplicates.iter_mut().zip(stdio) {
        if let Some(source) = source {
            // Keep sources above 0..=2 so ordered child dup2 actions cannot
            // overwrite a later stream's source. CLOEXEC prevents extra copies
            // from surviving exec; fork/exec transfers these owners to Command.
            let fd = unsafe { libc::fcntl(*source, libc::F_DUPFD_CLOEXEC, 3) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            *slot = Some(unsafe { OwnedFd::from_raw_fd(fd) });
        }
    }
    Ok(duplicates)
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
/// `inherited` is an inheritable handle retained by the caller through spawn.
pub(super) fn spawn(
    executable: Executable,
    command_line: OsString,
    env_block: Vec<u16>,
    stdio: [Option<RawFile>; 3],
    cwd: Option<OsString>,
    options: SpawnOptions,
    inherited: Option<RawFile>,
) -> io::Result<Spawned> {
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
                return Err(invalid_input());
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

    let job_object = if options.is_orphan {
        None
    } else {
        global_job_object()?
    };

    let mut owned_stdio = Vec::with_capacity(stdio.len());
    let mut stdio_handles = [std::ptr::null_mut(); 3];
    for (slot, raw) in stdio_handles.iter_mut().zip(&stdio) {
        if let Some(raw) = raw {
            let handle = duplicate_inheritable_handle(*raw)?;
            *slot = handle.as_raw_handle();
            owned_stdio.push(handle);
        }
    }
    let inherited_handles = owned_stdio
        .iter()
        .map(AsRawHandle::as_raw_handle)
        .chain(inherited)
        .collect::<Vec<_>>();

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
    drop(owned_stdio);

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
    Ok(Spawned {
        pid: process_info.dwProcessId,
        handle: unsafe { OwnedHandle::from_raw_handle(process_info.hProcess) },
    })
}

#[cfg(windows)]
fn global_job_object() -> io::Result<Option<RawFile>> {
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
fn create_global_job_object() -> io::Result<Option<RawFile>> {
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
        if error.raw_os_error().unwrap_or_default() == ERROR_ACCESS_DENIED as i32 {
            return Ok(None);
        }
        return Err(error);
    }

    Ok(Some(job))
}

#[cfg(target_os = "linux")]
pub(super) fn wait_pidfd(pidfd: RawFile, defer_reap: bool) -> io::Result<std::process::ExitStatus> {
    let mut siginfo = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
    // WNOWAIT retains the child identity until the caller explicitly reaps it.
    let flags = libc::WEXITED | if defer_reap { libc::WNOWAIT } else { 0 };
    if unsafe { libc::waitid(libc::P_PIDFD, pidfd as libc::id_t, &mut siginfo, flags) } < 0 {
        return Err(last_native_error());
    }
    Ok(siginfo_status(&siginfo))
}

#[cfg(unix)]
pub(super) fn wait_pid(pid: i32, defer_reap: bool) -> io::Result<std::process::ExitStatus> {
    if !defer_reap {
        let mut status = 0;
        let ret = unsafe { libc::waitpid(pid, &mut status, 0) };
        if ret < 0 {
            return Err(last_native_error());
        }
        if ret != pid {
            return Err(invalid_input());
        }
        return Ok(std::process::ExitStatus::from_raw(status));
    }
    let mut siginfo = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
    // Leave reaping to the caller while it still needs a stable child identity.
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
        return Err(invalid_input());
    }
    Ok(siginfo_status(&siginfo))
}

#[cfg(windows)]
pub(super) fn wait_handle(
    handle: RawFile,
    cancel: RawFile,
) -> io::Result<std::process::ExitStatus> {
    use windows_sys::Win32::Foundation::{
        ERROR_OPERATION_ABORTED, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, INFINITE, WaitForMultipleObjects, WaitForSingleObject,
    };

    let handles = [handle, cancel];
    let wait = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
    if wait == WAIT_OBJECT_0 + 1 {
        match unsafe { WaitForSingleObject(handle, 0) } {
            WAIT_OBJECT_0 => {}
            WAIT_TIMEOUT => {
                return Err(io::Error::from_raw_os_error(ERROR_OPERATION_ABORTED as i32));
            }
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
    Ok(std::process::ExitStatus::from_raw(code))
}

#[cfg(windows)]
pub(super) fn duplicate_inheritable_handle(raw: RawFile) -> io::Result<OwnedHandle> {
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
        Ok(unsafe { OwnedHandle::from_raw_handle(duplicate) })
    }
}

#[cfg(windows)]
fn last_native_error() -> io::Error {
    io::Error::from_raw_os_error(unsafe { windows_sys::Win32::Foundation::GetLastError() as i32 })
}

#[cfg(unix)]
fn last_native_error() -> io::Error {
    io::Error::from_raw_os_error(last_native_errno())
}

#[cfg(target_os = "linux")]
fn last_native_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

#[cfg(target_os = "macos")]
fn last_native_errno() -> i32 {
    unsafe { *libc::__error() }
}

fn invalid_input() -> io::Error {
    #[cfg(unix)]
    let code = libc::EINVAL;
    #[cfg(windows)]
    let code = windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER as i32;
    io::Error::from_raw_os_error(code)
}

#[cfg(unix)]
fn siginfo_status(info: &libc::siginfo_t) -> std::process::ExitStatus {
    let status = unsafe { info.si_status() };
    let raw = match info.si_code {
        libc::CLD_EXITED => status << 8,
        libc::CLD_DUMPED => status | 0x80,
        _ => status,
    };
    std::process::ExitStatus::from_raw(raw)
}

#[cfg(unix)]
pub(super) fn duplicate_inherited_file(fd: BorrowedFd<'_>) -> io::Result<OwnedFd> {
    let duplicate = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
    if duplicate < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(duplicate) })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn ambient_lookup_uses_parent_path_and_child_working_directory() {
        if std::env::var_os("MOONRUN_AMBIENT_LOOKUP_TEST").is_none() {
            let parent_directory = tempfile::tempdir().unwrap();
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process::os::tests::ambient_lookup_uses_parent_path_and_child_working_directory",
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
        let cwd = std::ffi::CString::new(directory.path().as_os_str().as_encoded_bytes()).unwrap();
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
        let native = wait_pid(pid, false).unwrap().code().unwrap();
        assert!(matches!(native, 23 | 42));
        let mut child_signal_mask = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::sigemptyset(&mut child_signal_mask) }, 0);
        let child = spawn(Spawn {
            executable: Executable::Ambient("program".into()),
            args: vec!["program".into()],
            env: vec!["PATH=/not-the-parent-path".into()],
            stdio: [None; 3],
            cwd: Some(directory.path().as_os_str().to_owned()),
            child_signal_mask,
            inherited: None,
        })
        .unwrap();
        assert_eq!(wait_pid(child.pid, false).unwrap().code(), Some(native));
        #[cfg(target_os = "linux")]
        {
            // Exercise the old-glibc cwd fallback even on a newer libc.
            let mut mask = unsafe { std::mem::zeroed() };
            assert_eq!(unsafe { libc::sigemptyset(&mut mask) }, 0);
            let pid = fork_exec::spawn(Spawn {
                executable: Executable::Ambient("program".into()),
                args: vec!["program".into()],
                env: vec!["PATH=/not-the-parent-path".into()],
                stdio: [None, None, None],
                cwd: Some(directory.path().as_os_str().to_owned()),
                child_signal_mask: mask,
                inherited: None,
            })
            .unwrap();
            assert_eq!(wait_pid(pid, false).unwrap().code(), Some(native));
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
}
