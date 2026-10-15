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

//! Shared registration for synchronous host functions with Wasm i32 arguments.
//!
//! This first interface covers the numeric WASIp1 imports used by all engines.
//! An adapter owns conversion and callback retention. A host operation receives
//! only borrowed state and memory, so it cannot re-enter Wasm or retain a view
//! across memory growth. Missing memory is deferred until the operation uses it.

#[derive(Debug, thiserror::Error)]
pub(crate) enum ImportError {
    // Not every adapter has an unbound-memory state. Keep this shared error
    // available independently of which backend is selected.
    #[allow(dead_code)]
    #[error("guest memory is unavailable")]
    MemoryUnavailable,
    #[error("run termination requested")]
    Terminated,
    #[error(transparent)]
    Host(#[from] anyhow::Error),
}

pub(crate) struct Caller<'a, T> {
    pub(crate) data: &'a T,
    pub(crate) memory: Result<&'a mut [u8], ImportError>,
}

/// Only unit and i32 results are supported by this first registration interface.
/// The Rust return type determines the Wasm signature; callers cannot declare
/// one result type and accidentally return another.
pub(crate) trait WasmResult: 'static {
    const HAS_RESULT: bool;
    fn into_i32(self) -> Option<i32>;
}

impl WasmResult for i32 {
    const HAS_RESULT: bool = true;
    fn into_i32(self) -> Option<i32> {
        Some(self)
    }
}

impl WasmResult for () {
    const HAS_RESULT: bool = false;
    fn into_i32(self) -> Option<i32> {
        None
    }
}

pub(crate) type HostFunction<T, const N: usize, R> =
    fn(Caller<'_, T>, [i32; N]) -> Result<R, ImportError>;

/// The argument array fixes the Wasm arity and avoids allocating a parameter
/// vector on each call. Reference values and guest re-entry are intentionally
/// outside this interface; extend it when an actual host operation needs them.
pub(crate) trait ImportRegistry<T> {
    type Error;
    fn define<const N: usize, R: WasmResult>(
        &mut self,
        module: &'static str,
        name: &'static str,
        function: HostFunction<T, N, R>,
    ) -> Result<(), Self::Error>;
}
