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

//! A custom Wasm limit tightens the current OS thread stack; it never allocates
//! a larger one. Validate actual bounds and reserve room for host/error handling.
use super::{StoreData, api};
const HOST_RESERVE: usize = 128 * 1024;
pub(super) struct Limit<'a> {
    store: &'a api::Store<StoreData>,
    previous: Option<usize>,
}
impl<'a> Limit<'a> {
    pub(super) fn new(
        store: &'a api::Store<StoreData>,
        kib: Option<usize>,
    ) -> anyhow::Result<Self> {
        let Some(kib) = kib else {
            return Ok(Self {
                store,
                previous: None,
            });
        };
        let bytes = kib
            .checked_mul(1024)
            .filter(|size| *size > 0)
            .ok_or_else(|| anyhow::anyhow!("wee8 stack size must be nonzero and fit in bytes"))?;
        let marker = 0u8;
        let current = &marker as *const u8 as usize;
        let (low, high) = bounds()?;
        let limit = current
            .checked_sub(bytes)
            .ok_or_else(|| anyhow::anyhow!("wee8 stack size exceeds the native thread stack"))?;
        anyhow::ensure!(
            current < high && limit >= low.saturating_add(HOST_RESERVE),
            "wee8 stack size exceeds the native thread stack (including host reserve)"
        );
        // All supported targets have downward-growing stacks. This boundary
        // lies in the live thread stack, above the host reserve. The store and
        // guard stay on this thread; Drop restores the old limit before teardown.
        let previous = unsafe { store.set_stack_limit(limit)? };
        Ok(Self {
            store,
            previous: Some(previous),
        })
    }
}
impl Drop for Limit<'_> {
    fn drop(&mut self) {
        if let Some(previous) = self.previous {
            unsafe { self.store.set_stack_limit(previous) }
                .expect("restore inactive store stack limit");
        }
    }
}
#[cfg(target_os = "macos")]
fn bounds() -> anyhow::Result<(usize, usize)> {
    unsafe {
        let thread = libc::pthread_self();
        let high = libc::pthread_get_stackaddr_np(thread) as usize;
        let size = libc::pthread_get_stacksize_np(thread);
        Ok((
            high.checked_sub(size)
                .ok_or_else(|| anyhow::anyhow!("invalid thread stack bounds"))?,
            high,
        ))
    }
}
#[cfg(target_os = "linux")]
fn bounds() -> anyhow::Result<(usize, usize)> {
    unsafe {
        let mut attr = std::mem::MaybeUninit::uninit();
        let error = libc::pthread_getattr_np(libc::pthread_self(), attr.as_mut_ptr());
        if error != 0 {
            return Err(std::io::Error::from_raw_os_error(error).into());
        }
        let mut attr = attr.assume_init();
        let mut address = std::ptr::null_mut();
        let mut size = 0;
        let error = libc::pthread_attr_getstack(&attr, &mut address, &mut size);
        libc::pthread_attr_destroy(&mut attr);
        if error != 0 {
            return Err(std::io::Error::from_raw_os_error(error).into());
        }
        let low = address as usize;
        Ok((
            low,
            low.checked_add(size)
                .ok_or_else(|| anyhow::anyhow!("invalid thread stack bounds"))?,
        ))
    }
}
#[cfg(windows)]
fn bounds() -> anyhow::Result<(usize, usize)> {
    let mut low = 0;
    let mut high = 0;
    unsafe {
        windows_sys::Win32::System::Threading::GetCurrentThreadStackLimits(&mut low, &mut high)
    };
    Ok((low, high))
}
