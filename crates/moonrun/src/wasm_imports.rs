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

//! Shared registration for synchronous host functions with Wasm integer arguments.
//!
//! This interface covers the WASIp1 imports supported by Moonrun.
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

/// WASIp1 imports return either an i32 errno or no value.
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

/// The two integer representations used by WASIp1 host parameters.
#[derive(Clone, Copy)]
pub(crate) enum WasmType {
    I32,
    I64,
}

pub(crate) trait WasmParam: Copy + 'static {
    const TYPE: WasmType;
    fn from_bits(value: i64) -> Self;
}

impl WasmParam for i32 {
    const TYPE: WasmType = WasmType::I32;
    fn from_bits(value: i64) -> Self {
        value as i32
    }
}

impl WasmParam for i64 {
    const TYPE: WasmType = WasmType::I64;
    fn from_bits(value: i64) -> Self {
        value
    }
}

/// The internal argument tuple supplies parameter types and order.
/// The adapter-provided `read` decodes the requested Wasm type: i32 values
/// are widened to i64, and i64 values retain all bits (never through f64).
/// Decoding finishes before borrowing guest memory and needs no parameter Vec.
pub(crate) trait WasmParams: Copy + 'static {
    const TYPES: &'static [WasmType];
    fn decode<Error>(
        read: impl FnMut(usize, WasmType) -> Result<i64, Error>,
    ) -> Result<Self, Error>;
}

/// Like Wasmtime's `func_wrap`/`IntoFunc`, registration derives the signature
/// from an ordinary typed Rust callback. The tuple only connects adapter decoding
/// to invocation; import definitions do not construct or unpack it. Adapters
/// retain the closure itself, including any captures, for the callback lifetime.
/// Send + Sync permits registration with Wasmtime's thread-safe Linker; Caller
/// still borrows each run's state and memory only for this invocation.
pub(crate) trait HostFunction<T, P, R>: Send + Sync + 'static {
    fn call(&self, caller: Caller<'_, T>, params: P) -> Result<R, ImportError>;
}

macro_rules! host_function {
    ($($param:ident: $index:tt),+) => {
        impl<$($param: WasmParam),+> WasmParams for ($($param,)+) {
            const TYPES: &'static [WasmType] = &[$($param::TYPE,)+];
            fn decode<Error>(mut read: impl FnMut(usize, WasmType) -> Result<i64, Error>) -> Result<Self, Error> {
                Ok(($($param::from_bits(read($index, $param::TYPE)?),)+))
            }
        }

        impl<T, Func, R, $($param),+> HostFunction<T, ($($param,)+), R> for Func
        where
            Func: Fn(Caller<'_, T>, $($param),+) -> Result<R, ImportError> + Send + Sync + 'static,
            $($param: WasmParam,)+
            R: WasmResult,
        {
            fn call(&self, caller: Caller<'_, T>, params: ($($param,)+)) -> Result<R, ImportError> {
                self(caller, $(params.$index),+)
            }
        }
    };
}

host_function!(A: 0);
host_function!(A: 0, B: 1);
host_function!(A: 0, B: 1, C: 2);
host_function!(A: 0, B: 1, C: 2, D: 3);
host_function!(A: 0, B: 1, C: 2, D: 3, E: 4);
host_function!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);
host_function!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6);
host_function!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7);
host_function!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8);

/// Reference values and guest re-entry remain outside this interface.
pub(crate) trait ImportRegistry<T> {
    type Error;
    fn func_wrap<P: WasmParams, R: WasmResult, F: HostFunction<T, P, R>>(
        &mut self,
        module: &'static str,
        name: &'static str,
        function: F,
    ) -> Result<(), Self::Error>;
}
