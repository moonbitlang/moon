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

use std::collections::HashMap;
use std::process::Child as StdChild;
use tokio::process::Child as TokioChild;
use tracing::{debug, warn, trace};
#[cfg(unix)]
use libc::{kill, SIGKILL, SIGTERM};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    OpenProcess,
    TerminateProcess,
    WaitForSingleObject,
    PROCESS_TERMINATE,
    SYNCHRONIZE,
};

pub enum ChildProcess {
    Std(StdChild),
    Tokio(TokioChild),
    TokioIdOnly(u32),
}

#[cfg(unix)]
fn pid_is_alive(pid: i32) -> bool {
    unsafe {
        if kill(pid, 0) == 0 {
            return true;
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => false,
            _ => true,
        }
    }
}

#[cfg(unix)]
async fn kill_pid_with_grace(pid: i32) -> std::io::Result<()> {
    unsafe {
        let _ = kill(pid, SIGTERM);
    }
    for _ in 0..5 {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        if !pid_is_alive(pid) {
            return Ok(());
        }
    }
    unsafe {
        let _ = kill(pid, SIGKILL);
    }
    for _ in 0..5 {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        if !pid_is_alive(pid) {
            return Ok(());
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "process did not exit after SIGTERM/SIGKILL",
    ))
}

#[cfg(unix)]
fn kill_pid_with_grace_sync(pid: i32) -> std::io::Result<()> {
    unsafe {
        let _ = kill(pid, SIGTERM);
    }
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if !pid_is_alive(pid) {
            return Ok(());
        }
    }
    unsafe {
        let _ = kill(pid, SIGKILL);
    }
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if !pid_is_alive(pid) {
            return Ok(());
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "process did not exit after SIGTERM/SIGKILL",
    ))
}

#[cfg(windows)]
fn kill_pid_with_grace(pid: u32) -> std::io::Result<()> {
    kill_pid_with_timeout(pid, 2000)
}

#[cfg(windows)]
fn kill_pid_with_grace_sync(pid: u32) -> std::io::Result<()> {
    kill_pid_with_timeout(pid, 2000)
}

#[cfg(windows)]
fn kill_pid_with_timeout(pid: u32, timeout_ms: u32) -> std::io::Result<()> {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid);
        if handle == 0 {
            return Err(std::io::Error::last_os_error());
        }

        if TerminateProcess(handle, 1) == 0 {
            let err = std::io::Error::last_os_error();
            CloseHandle(handle);
            return Err(err);
        }

        let wait_result = WaitForSingleObject(handle, timeout_ms);
        CloseHandle(handle);

        match wait_result {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_TIMEOUT => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "process did not exit after terminate",
            )),
            _ => Err(std::io::Error::last_os_error()),
        }
    }
}

impl ChildProcess {
    pub fn id(&self) -> u32 {
        match self {
            ChildProcess::Std(c) => c.id(),
            ChildProcess::Tokio(c) => c.id().expect("tokio child should have an ID"),
            ChildProcess::TokioIdOnly(id) => *id,
        }
    }

    pub async fn kill(&mut self) -> std::io::Result<()> {
        match self {
            ChildProcess::Std(c) => c.kill(),
            ChildProcess::Tokio(c) => {
                trace!("Attempting to kill tokio child");
                c.kill().await
            }
            ChildProcess::TokioIdOnly(id) => {
                trace!("Attempting to kill tokio child by ID: {}", id);
                #[cfg(unix)]
                {
                    let pid = *id as i32;
                    if let Err(e) = kill_pid_with_grace(pid).await {
                        warn!(
                            "Failed to kill child process {} (errno: {})",
                            id,
                            std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
                        );
                        return Err(e);
                    }
                    Ok(())
                }
                #[cfg(windows)]
                {
                    if let Err(e) = kill_pid_with_grace(*id) {
                        warn!(
                            "Failed to kill child process {} (errno: {})",
                            id,
                            std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
                        );
                        return Err(e);
                    }
                    Ok(())
                }
                #[cfg(not(any(unix, windows)))]
                {
                    warn!("Killing by PID is not supported on this platform");
                    Err(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "Killing by PID is not supported on this platform",
                    ))
                }
            }
        }
    }

    pub async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        match self {
            ChildProcess::Std(c) => c.wait(),
            ChildProcess::Tokio(c) => c.wait().await,
            ChildProcess::TokioIdOnly(id) => {
                trace!("Waiting for tokio child by ID: {}", id);
                // For ID-only processes, we can't really wait properly
                // Just sleep a bit to allow it to exit
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                #[cfg(unix)]
                {
                    Ok(std::process::ExitStatus::from_raw(0))
                }
                #[cfg(not(unix))]
                {
                    Ok(std::process::ExitStatus::default())
                }
            }
        }
    }
}

use once_cell::sync::OnceCell;
use std::sync::Mutex;

static REGISTRY: OnceCell<Mutex<ChildProcessRegistry>> = OnceCell::new();

pub struct ChildProcessRegistry {
    children: Mutex<HashMap<u32, ChildProcess>>,
}

impl ChildProcessRegistry {
    pub fn global() -> &'static Mutex<ChildProcessRegistry> {
        REGISTRY.get_or_init(|| {
            Mutex::new(ChildProcessRegistry {
                children: Mutex::new(HashMap::new()),
            })
        })
    }

    pub fn register_std(&self, child: StdChild) {
        let id = child.id();
        debug!("Registering std child process with PID: {}", id);
        self.children.lock().unwrap().insert(id, ChildProcess::Std(child));
    }

    pub fn register_tokio(&self, child: TokioChild) {
        let id = child.id().expect("tokio child should have an ID");
        debug!("Registering tokio child process with PID: {}", id);
        self.children.lock().unwrap().insert(id, ChildProcess::Tokio(child));
    }

    pub fn register_tokio_id(&self, id: u32) {
        debug!("Registering tokio child process ID: {}", id);
        self.children.lock().unwrap().insert(id, ChildProcess::TokioIdOnly(id));
    }

    pub fn unregister(&self, pid: u32) {
        debug!("Unregistering child process with PID: {}", pid);
        self.children.lock().unwrap().remove(&pid);
    }

    pub async fn kill_all(&self) {
        let mut children = self.children.lock().unwrap();
        let mut entries = Vec::with_capacity(children.len());
        entries.extend(children.drain());
        drop(children);

        for (pid, mut child) in entries {
            debug!("Killing child process with PID: {}", pid);
            if let Err(e) = child.kill().await {
                warn!("Failed to kill child process {}: {:?}", pid, e);
            }
        }
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        debug!("Shutting down child process registry, {} children remaining", self.children.lock().unwrap().len());
        self.kill_all().await;

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(5);

        while start.elapsed() < timeout {
            let remaining = self.children.lock().unwrap().len();
            if remaining == 0 {
                debug!("All child processes have terminated");
                return Ok(());
            }
            trace!("Waiting for {} child processes to terminate...", remaining);
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        let remaining = self.children.lock().unwrap().len();
        if remaining > 0 {
            warn!(
                "{} child processes did not terminate within {} seconds, forcing shutdown",
                remaining,
                timeout.as_secs()
            );
        }

        Ok(())
    }

    pub fn kill_all_sync(&self) {
        let mut children = self.children.lock().unwrap();
        let mut entries = Vec::with_capacity(children.len());
        entries.extend(children.drain());
        drop(children);

        for (pid, mut child) in entries {
            debug!("Killing child process with PID: {} (synchronous)", pid);
            if let Err(e) = child.kill_sync() {
                warn!("Failed to kill child process {}: {:?}", pid, e);
            }
        }
    }
}

impl ChildProcess {
    fn kill_sync(&mut self) -> std::io::Result<()> {
        match self {
            ChildProcess::Std(c) => c.kill(),
            ChildProcess::Tokio(c) => {
                trace!("Attempting to kill tokio child (synchronous)");
                c.start_kill()
            }
            ChildProcess::TokioIdOnly(id) => {
                trace!("Attempting to kill tokio child by ID (synchronous): {}", id);
                #[cfg(unix)]
                {
                    let pid = *id as i32;
                    if let Err(e) = kill_pid_with_grace_sync(pid) {
                        warn!(
                            "Failed to kill child process {} (errno: {})",
                            id,
                            std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
                        );
                        return Err(e);
                    }
                    Ok(())
                }
                #[cfg(windows)]
                {
                    if let Err(e) = kill_pid_with_grace_sync(*id) {
                        warn!(
                            "Failed to kill child process {} (errno: {})",
                            id,
                            std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
                        );
                        return Err(e);
                    }
                    Ok(())
                }
                #[cfg(not(any(unix, windows)))]
                {
                    warn!("Killing by PID is not supported on this platform");
                    Err(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "Killing by PID is not supported on this platform",
                    ))
                }
            }
        }
    }
}
