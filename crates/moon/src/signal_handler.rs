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

//! Signal handling for proper child process termination
//! 
//! This module provides signal propagation from the parent moon process
//! to all child processes (especially moonrun) to ensure clean termination
//! when receiving signals like SIGINT, SIGTERM, or SIGABRT.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use tokio::process::Child as TokioChild;
use tracing::{error, warn, info};

#[cfg(unix)]
use libc::{self, pid_t};

/// Flag to indicate if we're shutting down to avoid signal handling loops
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Global registry for tracking child processes that should receive signals
use std::sync::OnceLock;

static CHILD_PROCESS_REGISTRY: OnceLock<ChildProcessRegistry> = OnceLock::new();

fn get_registry() -> &'static ChildProcessRegistry {
    CHILD_PROCESS_REGISTRY.get_or_init(|| ChildProcessRegistry::new())
}

/// Information about a tracked child process
struct ChildProcessInfo {
    /// System process ID
    pid: u32,
    /// Whether this child has already been terminated
    terminated: bool,
}

/// Registry to track all child processes that need signal propagation
struct ChildProcessRegistry {
    children: Arc<Mutex<HashMap<u32, ChildProcessInfo>>>,
    next_id: std::sync::atomic::AtomicU32,
}

impl ChildProcessRegistry {
    fn new() -> Self {
        Self {
            children: Arc::new(Mutex::new(HashMap::new())),
            next_id: std::sync::atomic::AtomicU32::new(1),
        }
    }

    /// Register a child process for signal propagation
    fn register_child(&self, child: &Child) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let pid = child.id();
        
        if let Ok(mut children) = self.children.lock() {
            children.insert(id, ChildProcessInfo {
                pid,
                terminated: false,
            });
            info!("Registered child process {} with PID {} for signal propagation", id, pid);
        }
        
        id
    }

    /// Register a tokio child process for signal propagation
    fn register_tokio_child(&self, child: &TokioChild) -> Option<u32> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let pid = child.id()?;
        
        if let Ok(mut children) = self.children.lock() {
            children.insert(id, ChildProcessInfo {
                pid,
                terminated: false,
            });
            info!("Registered tokio child process {} with PID {} for signal propagation", id, pid);
            return Some(id);
        }
        
        None
    }

    /// Unregister a child process (typically when it exits normally)
    fn unregister_child(&self, id: u32) {
        if let Ok(mut children) = self.children.lock() {
            children.remove(&id);
            info!("Unregistered child process {}", id);
        }
    }

    /// Send termination signals to all registered child processes
    fn terminate_all_children(&self) {
        if SHUTTING_DOWN.load(Ordering::SeqCst) {
            return; // Avoid recursive termination
        }
        
        SHUTTING_DOWN.store(true, Ordering::SeqCst);
        
        let children_to_terminate = {
            if let Ok(children) = self.children.lock() {
                children.iter().map(|(id, info)| (*id, info.pid)).collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        };

        info!("Terminating {} child processes", children_to_terminate.len());

        #[cfg(unix)]
        for (id, pid) in children_to_terminate {
            unsafe {
                // Try different signals in order of preference
                if libc::kill(pid as pid_t, libc::SIGTERM) == 0 {
                    info!("Sent SIGTERM to child process {} (PID: {})", id, pid);
                    
                    // Give it a moment to terminate gracefully
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    
                    // If still alive, send SIGKILL
                    if self.is_process_alive(pid) {
                        warn!("Child process {} (PID: {}) still alive, sending SIGKILL", id, pid);
                        libc::kill(pid as pid_t, libc::SIGKILL);
                    }
                } else {
                    error!("Failed to send SIGTERM to child process {} (PID: {})", id, pid);
                }
            }
        }

        #[cfg(not(unix))]
        {
            // On non-Unix platforms, we can only log
            warn!("Signal propagation not implemented on this platform");
        }
    }

    /// Check if a process is still alive by sending signal 0
    #[cfg(unix)]
    fn is_process_alive(&self, pid: u32) -> bool {
        unsafe {
            libc::kill(pid as pid_t, 0) == 0
        }
    }

    #[cfg(not(unix))]
    fn is_process_alive(&self, _pid: u32) -> bool {
        false // Not implemented on non-Unix platforms
    }
}

/// Register a child process for signal propagation
pub fn register_child(child: &Child) -> u32 {
    get_registry().register_child(child)
}

/// Register a tokio child process for signal propagation
pub fn register_tokio_child(child: &TokioChild) -> Option<u32> {
    get_registry().register_tokio_child(child)
}

/// Unregister a child process (call when it exits normally)
pub fn unregister_child(id: u32) {
    get_registry().unregister_child(id)
}

/// Setup signal handlers for common termination signals
pub fn setup_signal_handlers() -> Result<()> {
    info!("Setting up signal handlers for child process propagation");
    
    // Setup Ctrl-C handler (cross-platform)
    ctrlc::set_handler(move || {
        info!("Received Ctrl-C, terminating child processes");
        get_registry().terminate_all_children();
        std::process::exit(130); // Standard exit code for SIGINT
    })?;
    
    // Setup Unix-specific signal handlers
    #[cfg(unix)]
    {
        use signal_hook::consts::signal::*;
        use signal_hook::iterator::Signals;
        
        std::thread::spawn(move || {
            let mut signals = Signals::new([
                SIGTERM,
                SIGABRT,
                SIGHUP,
            ]).expect("Failed to create signal iterator");
            
            for signal in &mut signals {
                match signal {
                    SIGTERM => {
                        info!("Received SIGTERM, terminating child processes");
                        get_registry().terminate_all_children();
                        std::process::exit(143); // Standard exit code for SIGTERM
                    }
                    SIGABRT => {
                        info!("Received SIGABRT, terminating child processes");
                        get_registry().terminate_all_children();
                        std::process::exit(134); // Standard exit code for SIGABRT
                    }
                    SIGHUP => {
                        info!("Received SIGHUP, terminating child processes");
                        get_registry().terminate_all_children();
                        std::process::exit(129); // Standard exit code for SIGHUP
                    }
                    _ => {
                        warn!("Received unexpected signal: {}", signal);
                    }
                }
            }
        });
    }
    
    Ok(())
}

/// Wrapper for tokio::process::Child that automatically handles signal registration
pub struct SignalAwareChild {
    inner: TokioChild,
    registry_id: Option<u32>,
}

impl SignalAwareChild {
    /// Create a new signal-aware child from a tokio process
    pub fn new(child: TokioChild) -> Self {
        let registry_id = register_tokio_child(&child);
        Self {
            inner: child,
            registry_id,
        }
    }

    /// Get the inner child process
    pub fn inner(&self) -> &TokioChild {
        &self.inner
    }

    /// Get the inner child process mutably
    pub fn inner_mut(&mut self) -> &mut TokioChild {
        &mut self.inner
    }

    /// Wait for the child to finish
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus, std::io::Error> {
        let result = self.inner.wait().await;
        
        // Unregister from signal registry since we're waiting for it to finish
        if let Some(id) = self.registry_id.take() {
            unregister_child(id);
        }
        
        result
    }

    /// Try to wait for the child to finish without blocking
    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>, std::io::Error> {
        let result = self.inner.try_wait()?;
        
        // If the process has finished, unregister it
        if result.is_some() {
            if let Some(id) = self.registry_id.take() {
                unregister_child(id);
            }
        }
        
        Ok(result)
    }
}

impl Drop for SignalAwareChild {
    fn drop(&mut self) {
        // Unregister from signal registry when the wrapper is dropped
        if let Some(id) = self.registry_id.take() {
            unregister_child(id);
        }
    }
}

/// Convert a tokio::process::Child to a SignalAwareChild
pub fn make_signal_aware(child: TokioChild) -> SignalAwareChild {
    SignalAwareChild::new(child)
}