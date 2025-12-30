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
use std::sync::Arc;
use tokio::sync::{Mutex as TokioMutex, Notify};
use std::sync::atomic::{AtomicBool, Ordering};

pub enum ChildProcess {
    Std(Arc<std::sync::Mutex<StdChild>>),
    Tokio(Arc<TokioMutex<TokioChild>>),
}

impl ChildProcess {
    pub fn id(&self) -> u32 {
        match self {
            ChildProcess::Std(c) => c.lock().unwrap().id(),
            ChildProcess::Tokio(c) => c
                .blocking_lock()
                .id()
                .expect("tokio child should have an ID"),
        }
    }

    pub async fn kill(&mut self) -> std::io::Result<()> {
        match self {
            ChildProcess::Std(c) => c.lock().unwrap().kill(),
            ChildProcess::Tokio(c) => {
                trace!("Attempting to kill tokio child");
                c.lock().await.start_kill()
            }
        }
    }

    pub async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        match self {
            ChildProcess::Std(c) => c.lock().unwrap().wait(),
            ChildProcess::Tokio(c) => c.lock().await.wait().await,
        }
    }
}

use once_cell::sync::OnceCell;
use std::sync::Mutex;

static REGISTRY: OnceCell<Mutex<ChildProcessRegistry>> = OnceCell::new();
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_NOTIFY: OnceCell<Notify> = OnceCell::new();

fn shutdown_notify() -> &'static Notify {
    SHUTDOWN_NOTIFY.get_or_init(Notify::new)
}

pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    shutdown_notify().notify_waiters();
}

pub async fn wait_for_shutdown() {
    if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
        return;
    }
    shutdown_notify().notified().await;
}

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
        let handle = Arc::new(Mutex::new(child));
        self.children.lock().unwrap().insert(id, ChildProcess::Std(handle));
    }

    pub fn register_tokio(&self, child: TokioChild) -> Arc<TokioMutex<TokioChild>> {
        let id = child.id().expect("tokio child should have an ID");
        debug!("Registering tokio child process with PID: {}", id);
        let handle = Arc::new(TokioMutex::new(child));
        self.children
            .lock()
            .unwrap()
            .insert(id, ChildProcess::Tokio(handle.clone()));
        handle
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
            ChildProcess::Std(c) => c.lock().unwrap().kill(),
            ChildProcess::Tokio(c) => {
                trace!("Attempting to kill tokio child (synchronous)");
                match c.try_lock() {
                    Ok(mut child) => child.start_kill(),
                    Err(_) => {
                        warn!("Child process is busy; skipping synchronous kill");
                        Ok(())
                    }
                }
            }
        }
    }
}
