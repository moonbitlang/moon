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

//! Actual bounds of the calling native thread, independent of guest budgets.

/// Validate a downward-growing guest stack against this thread's remaining
/// native stack, leaving room for host callbacks and error handling.
pub(crate) fn checked_limit(bytes: usize) -> anyhow::Result<usize> {
    const HOST_RESERVE: usize = 128 * 1024;
    let marker = 0u8;
    let current = &marker as *const u8 as usize;
    let (low, high) = bounds()?;
    let limit = current
        .checked_sub(bytes)
        .ok_or_else(|| anyhow::anyhow!("guest stack size exceeds the native thread stack"))?;
    anyhow::ensure!(
        current < high
            && limit
                .checked_sub(HOST_RESERVE)
                .is_some_and(|end| end >= low),
        "guest stack size exceeds the native thread stack (including host reserve)"
    );
    Ok(limit)
}

#[cfg(target_os = "macos")]
pub(crate) fn bounds() -> std::io::Result<(usize, usize)> {
    unsafe {
        let thread = libc::pthread_self();
        let high = libc::pthread_get_stackaddr_np(thread) as usize;
        let size = libc::pthread_get_stacksize_np(thread);
        Ok((
            high.checked_sub(size)
                .ok_or_else(|| std::io::Error::other("invalid thread stack bounds"))?,
            high,
        ))
    }
}
#[cfg(target_os = "linux")]
pub(crate) fn bounds() -> std::io::Result<(usize, usize)> {
    unsafe {
        let mut attr = std::mem::MaybeUninit::uninit();
        let error = libc::pthread_getattr_np(libc::pthread_self(), attr.as_mut_ptr());
        if error != 0 {
            return Err(std::io::Error::from_raw_os_error(error));
        }
        let mut attr = attr.assume_init();
        let mut address = std::ptr::null_mut();
        let mut size = 0;
        let error = libc::pthread_attr_getstack(&attr, &mut address, &mut size);
        libc::pthread_attr_destroy(&mut attr);
        if error != 0 {
            return Err(std::io::Error::from_raw_os_error(error));
        }
        let low = address as usize;
        Ok((
            low,
            low.checked_add(size)
                .ok_or_else(|| std::io::Error::other("invalid thread stack bounds"))?,
        ))
    }
}
#[cfg(windows)]
pub(crate) fn bounds() -> std::io::Result<(usize, usize)> {
    let mut low = 0;
    let mut high = 0;
    unsafe {
        windows_sys::Win32::System::Threading::GetCurrentThreadStackLimits(&mut low, &mut high)
    };
    Ok((low, high))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub(crate) fn bounds() -> std::io::Result<(usize, usize)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "native thread stack bounds are unavailable on this platform",
    ))
}
