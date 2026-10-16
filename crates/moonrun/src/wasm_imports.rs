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

//! Shared registration for synchronous host functions with Wasm numeric arguments.
//!
//! WASI, async and SQLite supply their definitions through this interface.
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
    #[error("{0}")]
    Operation(String),
    #[error(transparent)]
    Host(#[from] anyhow::Error),
}

impl ImportError {
    pub(crate) fn at(self, module: &str, name: &str) -> Self {
        match self {
            Self::Operation(error) => {
                Self::Host(anyhow::anyhow!("{module}.{name} failed: {error}"))
            }
            error => error,
        }
    }
}

pub(crate) struct Caller<'a, T> {
    pub(crate) data: &'a T,
    pub(crate) memory: Result<&'a mut [u8], ImportError>,
    pub(crate) termination: &'a crate::run_termination::TerminationRequest,
}

/// Numeric Wasm types have signless integer encodings. Floating-point values
/// cross the adapter as raw bits, preserving signed zero and avoiding integer
/// conversion through JavaScript numbers.
#[derive(Clone, Copy)]
pub(crate) enum WasmType {
    I32,
    I64,
    F32,
    F64,
}

pub(crate) trait WasmParam: Copy + 'static {
    const TYPE: WasmType;
    fn from_bits(value: u64) -> Self;
    fn into_bits(self) -> u64;
}

macro_rules! integer {
    ($ty:ty, $kind:ident) => {
        impl WasmParam for $ty {
            const TYPE: WasmType = WasmType::$kind;
            fn from_bits(value: u64) -> Self {
                value as Self
            }
            fn into_bits(self) -> u64 {
                self as u64
            }
        }
    };
}
integer!(i32, I32);
integer!(u32, I32);
integer!(i64, I64);
integer!(u64, I64);

impl WasmParam for f32 {
    const TYPE: WasmType = WasmType::F32;
    fn from_bits(value: u64) -> Self {
        f32::from_bits(value as u32)
    }
    fn into_bits(self) -> u64 {
        u64::from(self.to_bits())
    }
}

impl WasmParam for f64 {
    const TYPE: WasmType = WasmType::F64;
    fn from_bits(value: u64) -> Self {
        f64::from_bits(value)
    }
    fn into_bits(self) -> u64 {
        self.to_bits()
    }
}

/// The Rust return type determines the Wasm signature. A host function returns
/// one numeric value or no value; references and multi-result host calls remain
/// outside this interface.
pub(crate) trait WasmResult: 'static {
    const TYPE: Option<WasmType>;
    fn into_bits(self) -> Option<u64>;
}

impl<T: WasmParam> WasmResult for T {
    const TYPE: Option<WasmType> = Some(T::TYPE);
    fn into_bits(self) -> Option<u64> {
        Some(WasmParam::into_bits(self))
    }
}

impl WasmResult for () {
    const TYPE: Option<WasmType> = None;
    fn into_bits(self) -> Option<u64> {
        None
    }
}

/// The internal argument tuple supplies parameter types and order.
/// The adapter-provided `read` decodes the requested Wasm type: values
/// retain their numeric bit pattern, including all 64 integer bits.
/// Decoding finishes before borrowing guest memory and needs no parameter Vec.
pub(crate) trait WasmParams: Copy + 'static {
    const TYPES: &'static [WasmType];
    fn decode<Error>(
        read: impl FnMut(usize, WasmType) -> Result<u64, Error>,
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
            fn decode<Error>(mut read: impl FnMut(usize, WasmType) -> Result<u64, Error>) -> Result<Self, Error> {
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

impl WasmParams for () {
    const TYPES: &'static [WasmType] = &[];
    fn decode<Error>(_: impl FnMut(usize, WasmType) -> Result<u64, Error>) -> Result<Self, Error> {
        Ok(())
    }
}

impl<T, Func, R: WasmResult> HostFunction<T, (), R> for Func
where
    Func: Fn(Caller<'_, T>) -> Result<R, ImportError> + Send + Sync + 'static,
{
    fn call(&self, caller: Caller<'_, T>, (): ()) -> Result<R, ImportError> {
        self(caller)
    }
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
host_function!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9);
host_function!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7, I: 8, J: 9, K: 10);

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
