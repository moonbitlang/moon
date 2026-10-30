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
        let limit = crate::native_stack::checked_limit(bytes)?;
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
