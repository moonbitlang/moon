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

//! Typed synchronous imports shared by all Wasm adapters.
//!
//! Like Wasmtime's `func_wrap`, Rust parameter and result types determine the
//! signature. Reference handles use the engine's roots and borrow the invocation;
//! they are never encoded as integers or retained in a second handle table.
#[cfg(all(not(feature = "v8"), any(feature = "wee8", feature = "wasmtime")))]
pub(crate) mod payload;
pub(crate) mod values;
pub(crate) use values::{ExternRef, Values};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ImportError {
    #[allow(dead_code)]
    #[error("guest memory is unavailable")]
    MemoryUnavailable,
    #[error("run termination requested")]
    Terminated,
    #[error("{0}")]
    Operation(String),
    #[error(transparent)]
    Host(#[from] anyhow::Error),
    #[cfg(feature = "v8")]
    #[error(transparent)]
    V8(#[from] crate::v8::context::ValueImportError),
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

/// A memory borrow contains no engine context, so it cannot allocate references,
/// re-enter Wasm or retain the view across guest memory growth.
pub(crate) struct MemoryCaller<'a, T> {
    pub(crate) data: &'a T,
    pub(crate) memory: Result<&'a mut [u8], ImportError>,
    pub(crate) termination: &'a crate::run_termination::TerminationRequest,
}

/// Adapter implementation of one invocation. Scoped callbacks accommodate wee8's
/// checked store borrows without exposing a RefCell guard or changing Runtime ownership.
pub(crate) trait CallContext<'call, T>: Values<'call> {
    fn with_data(&self, operation: &mut dyn FnMut(&T));
    fn with_memory(&mut self, operation: &mut dyn FnMut(MemoryCaller<'_, T>));
    fn terminate(&self, outcome: crate::run_termination::RunTermination);
}

pub(crate) struct Caller<'context, 'call, T> {
    pub(crate) context: &'context mut (dyn CallContext<'call, T> + 'context),
}
impl<'call, T> Caller<'_, 'call, T> {
    pub(crate) fn with_data<R>(&self, operation: impl FnOnce(&T) -> R) -> R {
        let mut operation = Some(operation);
        let mut result = None;
        self.context.with_data(&mut |data| {
            result = Some(operation.take().expect("one host data borrow")(data));
        });
        result.expect("adapter invokes the data borrow once")
    }
    pub(crate) fn with_memory<R>(self, operation: impl FnOnce(MemoryCaller<'_, T>) -> R) -> R {
        let mut operation = Some(operation);
        let mut result = None;
        self.context.with_memory(&mut |caller| {
            result = Some(operation.take().expect("one guest memory borrow")(caller));
        });
        result.expect("adapter invokes the memory borrow once")
    }
    pub(crate) fn values(&mut self) -> &mut dyn Values<'call> {
        self.context
    }
    pub(crate) fn terminate(&self, outcome: crate::run_termination::RunTermination) -> ImportError {
        self.context.terminate(outcome);
        ImportError::Terminated
    }
}

#[derive(Clone, Copy)]
pub(crate) enum WasmType {
    I32,
    I64,
    F32,
    F64,
    ExternRef,
}

pub(crate) enum Value<'call> {
    I32(u32),
    I64(u64),
    F32(u32),
    F64(u64),
    ExternRef(Option<ExternRef<'call>>),
}

/// `Value` rebinds the invocation lifetime. The 'static type is only a signature
/// witness used for Rust inference; no reference is extended to 'static.
pub(crate) trait WasmParam: 'static {
    type Value<'call>;
    const TYPE: WasmType;
    fn decode(value: Value<'_>) -> Self::Value<'_>;
    fn encode<'call>(value: Self::Value<'call>) -> Value<'call>;
}
macro_rules! scalar {
    ($ty:ty, $variant:ident, $bits:ty) => {
        impl WasmParam for $ty {
            type Value<'call> = Self;
            const TYPE: WasmType = WasmType::$variant;
            fn decode(value: Value<'_>) -> Self {
                match value {
                    Value::$variant(value) => value as Self,
                    _ => unreachable!("adapter validates the declared parameter type"),
                }
            }
            fn encode<'call>(value: Self) -> Value<'call> {
                Value::$variant(value as $bits)
            }
        }
    };
}
scalar!(i32, I32, u32);
scalar!(u32, I32, u32);
scalar!(i64, I64, u64);
scalar!(u64, I64, u64);
macro_rules! float {
    ($ty:ty, $variant:ident) => {
        impl WasmParam for $ty {
            type Value<'call> = Self;
            const TYPE: WasmType = WasmType::$variant;
            fn decode(value: Value<'_>) -> Self {
                match value {
                    Value::$variant(value) => Self::from_bits(value),
                    _ => unreachable!("adapter validates the declared parameter type"),
                }
            }
            fn encode<'call>(value: Self) -> Value<'call> {
                Value::$variant(value.to_bits())
            }
        }
    };
}
float!(f32, F32);
float!(f64, F64);
impl WasmParam for Option<ExternRef<'static>> {
    type Value<'call> = Option<ExternRef<'call>>;
    const TYPE: WasmType = WasmType::ExternRef;
    fn decode(value: Value<'_>) -> Self::Value<'_> {
        match value {
            Value::ExternRef(value) => value,
            _ => unreachable!("adapter validates the declared reference type"),
        }
    }
    fn encode<'call>(value: Self::Value<'call>) -> Value<'call> {
        Value::ExternRef(value)
    }
}

pub(crate) trait WasmResult: 'static {
    type Value<'call>;
    // V8 wraps JS callbacks; the other adapters declare a native Wasm signature.
    #[cfg_attr(
        not(any(feature = "wee8", all(feature = "wasmtime", not(feature = "v8")))),
        allow(dead_code)
    )]
    const TYPE: Option<WasmType>;
    fn encode<'call>(value: Self::Value<'call>) -> Option<Value<'call>>;
}
impl<T: WasmParam> WasmResult for T {
    type Value<'call> = T::Value<'call>;
    const TYPE: Option<WasmType> = Some(T::TYPE);
    fn encode<'call>(value: Self::Value<'call>) -> Option<Value<'call>> {
        Some(T::encode(value))
    }
}
impl WasmResult for () {
    type Value<'call> = ();
    const TYPE: Option<WasmType> = None;
    fn encode<'call>(_: ()) -> Option<Value<'call>> {
        None
    }
}

pub(crate) trait WasmParams: 'static {
    type Value<'call>;
    const TYPES: &'static [WasmType];
    fn decode<'call, Error>(
        read: impl FnMut(usize, WasmType) -> Result<Value<'call>, Error>,
    ) -> Result<Self::Value<'call>, Error>;
}

pub(crate) trait HostFunction<T, P: WasmParams, R: WasmResult>:
    Send + Sync + 'static
{
    fn call<'call>(
        &self,
        caller: Caller<'_, 'call, T>,
        params: P::Value<'call>,
    ) -> Result<R::Value<'call>, ImportError>;
}
// Scalars have no invocation lifetime. Keeping that identity explicit preserves
// ordinary closure inference for existing numeric definitions.
pub(crate) trait Scalar: for<'a> WasmParam<Value<'a> = Self> + Copy {}
impl Scalar for i32 {}
impl Scalar for u32 {}
impl Scalar for i64 {}
impl Scalar for u64 {}
impl Scalar for f32 {}
impl Scalar for f64 {}
pub(crate) trait ScalarResult: for<'a> WasmResult<Value<'a> = Self> {}
impl<T: Scalar> ScalarResult for T {}
impl ScalarResult for () {}

macro_rules! host_function {
    ($($param:ident: $index:tt),*) => {
        impl<$($param: WasmParam),*> WasmParams for ($($param,)*) {
            type Value<'call> = ($($param::Value<'call>,)*);
            const TYPES: &'static [WasmType] = &[$($param::TYPE,)*];
            #[allow(unused_mut, unused_variables)]
            fn decode<'call, Error>(
                mut read: impl FnMut(usize, WasmType) -> Result<Value<'call>, Error>,
            ) -> Result<Self::Value<'call>, Error> {
                Ok(($($param::decode(read($index, $param::TYPE)?),)*))
            }
        }
        impl<T: 'static, Func, R: ScalarResult, $($param: Scalar),*>
            HostFunction<T, ($($param,)*), R> for Func
        where
            Func: for<'context, 'call> Fn(
                    Caller<'context, 'call, T>, $($param),*
                ) -> Result<R, ImportError>
                + Send + Sync + 'static,
        {
            fn call<'call>(
                &self,
                caller: Caller<'_, 'call, T>,
                params: <($($param,)*) as WasmParams>::Value<'call>,
            ) -> Result<R, ImportError> {
                let _ = &params;
                self(caller, $(params.$index),*)
            }
        }
    };
}
host_function!();
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

// Reference-bearing signatures bind every reference to the engine invocation,
// independently of the temporary Caller borrow. These cover the legacy host ABI;
// no unsafe lifetime erasure or unchecked reference-to-integer conversion is used.
macro_rules! reference_type {
    ($life:lifetime, reference) => { Option<ExternRef<$life>> };
    ($life:lifetime, unit) => { () };
    ($life:lifetime, $ty:ident) => { $ty };
}
macro_rules! reference_function {
    (($($param:ident: $index:tt),*) -> $result:ident) => {
        impl<T: 'static, Func> HostFunction<
            T,
            ($(reference_type!('static, $param),)*),
            reference_type!('static, $result),
        > for Func
        where
            Func: for<'context, 'call> Fn(
                    Caller<'context, 'call, T>,
                    $(reference_type!('call, $param)),*
                ) -> Result<reference_type!('call, $result), ImportError>
                + Send + Sync + 'static,
        {
            fn call<'call>(
                &self,
                caller: Caller<'_, 'call, T>,
                params: ($(reference_type!('call, $param),)*),
            ) -> Result<reference_type!('call, $result), ImportError> {
                let _ = &params;
                self(caller, $(params.$index),*)
            }
        }
    };
}
reference_function!(() -> reference);
reference_function!((i32: 0) -> reference);
reference_function!((reference: 0) -> reference);
reference_function!((reference: 0) -> i32);
reference_function!((reference: 0) -> f64);
reference_function!((reference: 0) -> unit);
reference_function!((reference: 0, i32: 1) -> reference);
reference_function!((reference: 0, i32: 1) -> i32);
reference_function!((reference: 0, i32: 1) -> unit);
reference_function!((reference: 0, reference: 1) -> unit);
reference_function!((reference: 0, reference: 1) -> i32);

pub(crate) trait ImportRegistry<T> {
    type Error;
    fn func_wrap<P: WasmParams, R: WasmResult, F: HostFunction<T, P, R>>(
        &mut self,
        module: &'static str,
        name: &'static str,
        function: F,
    ) -> Result<(), Self::Error>;
}
