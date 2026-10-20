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

//! Host-data representations used by Wasmtime and wee8. V8 keeps its native
//! strings, arrays and GC-traced builders/readers instead.
use super::{
    ImportError, Values,
    values::{Ref, Result},
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::{
    any::Any,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsString(pub(crate) Arc<[u16]>);
#[derive(Debug)]
pub(crate) struct JsBytes(pub(crate) Mutex<Vec<u8>>);
struct Strings(Vec<Vec<u16>>);
struct StringBuilder(Mutex<Vec<u16>>);
struct ByteBuilder(Mutex<Vec<u8>>);
struct Reader<T> {
    values: Vec<T>,
    next: AtomicUsize,
}
impl<T: Clone> Reader<T> {
    fn new(values: Vec<T>) -> Self {
        Self {
            values,
            next: AtomicUsize::new(0),
        }
    }
    fn read(&self) -> Option<T> {
        let index = self.next.load(Ordering::Relaxed);
        let value = self.values.get(index).cloned();
        if value.is_some() {
            self.next.store(index + 1, Ordering::Relaxed);
        }
        value
    }
}
struct Random(Mutex<StdRng>);

pub(crate) trait PayloadContext<'call> {
    fn new_payload(&mut self, data: Box<dyn Any + Send + Sync>) -> Result<Ref<'call>>;
    // Guest GC objects may be externrefs without any host payload. That is a
    // valid value for type tests; typed operations reject it after inspection.
    fn with_payload(
        &self,
        value: &Ref<'call>,
        operation: &mut dyn FnMut(Option<&dyn Any>),
    ) -> Result<()>;
}
fn read<'call, T: 'static, R>(
    context: &impl PayloadContext<'call>,
    value: &Ref<'call>,
    operation: impl FnOnce(&T) -> R,
) -> Result<R> {
    let mut operation = Some(operation);
    let mut result = None;
    context.with_payload(value, &mut |payload| {
        result = payload
            .and_then(|payload| payload.downcast_ref::<T>())
            .map(|payload| operation.take().unwrap()(payload));
    })?;
    result.ok_or_else(|| {
        ImportError::Operation("invalid host reference: unexpected externref host value".into())
    })
}
impl<'call, C: PayloadContext<'call>> Values<'call> for C {
    fn string(&mut self, value: &Ref<'call>) -> Result<String> {
        read(self, value, |value: &JsString| {
            String::from_utf16_lossy(&value.0)
        })
    }
    fn new_string(&mut self, value: &str) -> Result<Ref<'call>> {
        self.new_payload(Box::new(JsString(value.encode_utf16().collect())))
    }
    fn bytes(&mut self, value: &Ref<'call>) -> Result<Vec<u8>> {
        read(self, value, |value: &JsBytes| {
            value.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
        })
    }
    fn new_bytes(&mut self, value: Vec<u8>) -> Result<Ref<'call>> {
        self.new_payload(Box::new(JsBytes(Mutex::new(value))))
    }
    fn new_strings(&mut self, value: Vec<String>) -> Result<Ref<'call>> {
        self.new_payload(Box::new(Strings(
            value.iter().map(|s| s.encode_utf16().collect()).collect(),
        )))
    }
    fn array_len(&mut self, value: &Ref<'call>) -> Result<i32> {
        read(self, value, |value: &Strings| i32::try_from(value.0.len()))?
            .map_err(|e| ImportError::Host(e.into()))
    }
    fn array_get(&mut self, value: &Ref<'call>, index: i32) -> Result<Ref<'call>> {
        let units = read(self, value, |value: &Strings| {
            value.0.get(index as u32 as usize).cloned()
        })?
        .ok_or_else(|| ImportError::Operation("array index out of bounds".into()))?;
        self.new_payload(Box::new(JsString(units.into())))
    }
    fn is_string(&mut self, value: &Ref<'call>) -> Result<i32> {
        if value.is_none() {
            return Ok(0);
        }
        let mut result = false;
        self.with_payload(value, &mut |value| {
            result = value.is_some_and(|value| value.is::<JsString>())
        })?;
        Ok(i32::from(result))
    }
    fn string_builder(&mut self) -> Result<Ref<'call>> {
        self.new_payload(Box::new(StringBuilder(Mutex::new(Vec::new()))))
    }
    fn append_char(&mut self, value: &Ref<'call>, unit: i32) -> Result<()> {
        read(self, value, |value: &StringBuilder| {
            value
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(unit as u16)
        })
    }
    fn finish_string(&mut self, value: &Ref<'call>) -> Result<Ref<'call>> {
        let units = read(self, value, |value: &StringBuilder| {
            value.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
        })?;
        self.new_payload(Box::new(JsString(units.into())))
    }
    fn byte_builder(&mut self) -> Result<Ref<'call>> {
        self.new_payload(Box::new(ByteBuilder(Mutex::new(Vec::new()))))
    }
    fn append_byte(&mut self, value: &Ref<'call>, byte: i32) -> Result<()> {
        read(self, value, |value: &ByteBuilder| {
            value
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(byte as u8)
        })
    }
    fn finish_bytes(&mut self, value: &Ref<'call>) -> Result<Ref<'call>> {
        let bytes = read(self, value, |value: &ByteBuilder| {
            value.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
        })?;
        self.new_bytes(bytes)
    }
    fn string_reader(&mut self, value: &Ref<'call>) -> Result<Ref<'call>> {
        let units = read(self, value, |value: &JsString| value.0.to_vec())?;
        self.new_payload(Box::new(Reader::new(units)))
    }
    fn read_char(&mut self, value: &Ref<'call>) -> Result<i32> {
        read(self, value, |value: &Reader<u16>| {
            value.read().map_or(-1, i32::from)
        })
    }
    fn byte_reader(&mut self, value: &Ref<'call>) -> Result<Ref<'call>> {
        let bytes = self.bytes(value)?;
        self.new_payload(Box::new(Reader::new(bytes)))
    }
    fn read_byte(&mut self, value: &Ref<'call>) -> Result<i32> {
        read(self, value, |value: &Reader<u8>| {
            value.read().map_or(-1, i32::from)
        })
    }
    fn array_reader(&mut self, value: &Ref<'call>) -> Result<Ref<'call>> {
        let values = read(self, value, |value: &Strings| value.0.clone())?;
        self.new_payload(Box::new(Reader::new(values)))
    }
    fn read_string(&mut self, value: &Ref<'call>) -> Result<Ref<'call>> {
        let units = read(self, value, |value: &Reader<Vec<u16>>| value.read())?
            .unwrap_or_else(|| "ffi_end_of_/string_array".encode_utf16().collect());
        self.new_payload(Box::new(JsString(units.into())))
    }
    fn new_rng(&mut self, seed: i32) -> Result<Ref<'call>> {
        self.new_payload(Box::new(Random(Mutex::new(StdRng::seed_from_u64(
            seed as u64,
        )))))
    }
    fn random(&mut self, value: &Ref<'call>, upper: i32) -> Result<i32> {
        if upper <= 0 {
            return Err(ImportError::Operation(
                "random range upper bound must be positive".into(),
            ));
        }
        read(self, value, |value: &Random| {
            value
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .gen_range(0..upper)
        })
    }
    fn new_instant(&mut self) -> Result<Ref<'call>> {
        self.new_payload(Box::new(Instant::now()))
    }
    fn elapsed(&mut self, value: &Ref<'call>) -> Result<f64> {
        read(self, value, |value: &Instant| value.elapsed().as_secs_f64())
    }
}
