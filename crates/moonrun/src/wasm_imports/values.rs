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

//! Engine-rooted guest values. Payload conversion remains in the Wasm adapter;
//! in particular V8 strings/arrays remain native values, interoperable with its
//! JS-string builtins and ffi-bytes imports.
use super::ImportError;

pub(crate) enum ExternRef<'call> {
    #[cfg(feature = "v8")]
    V8(v8::Local<'call, v8::Value>),
    #[cfg(feature = "wee8")]
    Wee8(wee8::ExternRef<'call>),
    #[cfg(all(feature = "wasmtime", not(feature = "v8")))]
    Wasmtime(
        wasmtime::Rooted<wasmtime::ExternRef>,
        std::marker::PhantomData<&'call ()>,
    ),
}

pub(crate) type Ref<'call> = Option<ExternRef<'call>>;
pub(crate) type Result<T> = std::result::Result<T, ImportError>;

/// Conversion and access to the existing guest value representations. None of
/// these operations exposes a raw address or creates an additional rooting store.
pub(crate) trait Values<'call> {
    fn string(&mut self, value: &Ref<'call>) -> Result<String>;
    fn new_string(&mut self, value: &str) -> Result<Ref<'call>>;
    fn bytes(&mut self, value: &Ref<'call>) -> Result<Vec<u8>>;
    fn new_bytes(&mut self, value: Vec<u8>) -> Result<Ref<'call>>;
    fn new_strings(&mut self, value: Vec<String>) -> Result<Ref<'call>>;
    fn array_len(&mut self, value: &Ref<'call>) -> Result<i32>;
    fn array_get(&mut self, value: &Ref<'call>, index: i32) -> Result<Ref<'call>>;
    fn is_string(&mut self, value: &Ref<'call>) -> Result<i32>;
    fn string_builder(&mut self) -> Result<Ref<'call>>;
    fn append_char(&mut self, value: &Ref<'call>, unit: i32) -> Result<()>;
    fn finish_string(&mut self, value: &Ref<'call>) -> Result<Ref<'call>>;
    fn byte_builder(&mut self) -> Result<Ref<'call>>;
    fn append_byte(&mut self, value: &Ref<'call>, byte: i32) -> Result<()>;
    fn finish_bytes(&mut self, value: &Ref<'call>) -> Result<Ref<'call>>;
    fn string_reader(&mut self, value: &Ref<'call>) -> Result<Ref<'call>>;
    fn read_char(&mut self, value: &Ref<'call>) -> Result<i32>;
    fn byte_reader(&mut self, value: &Ref<'call>) -> Result<Ref<'call>>;
    fn read_byte(&mut self, value: &Ref<'call>) -> Result<i32>;
    fn array_reader(&mut self, value: &Ref<'call>) -> Result<Ref<'call>>;
    fn read_string(&mut self, value: &Ref<'call>) -> Result<Ref<'call>>;
    fn new_rng(&mut self, seed: i32) -> Result<Ref<'call>>;
    fn random(&mut self, value: &Ref<'call>, upper: i32) -> Result<i32>;
    fn new_instant(&mut self) -> Result<Ref<'call>>;
    fn elapsed(&mut self, value: &Ref<'call>) -> Result<f64>;
}
