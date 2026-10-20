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

//! V8-native representation and rooting of shared host values.
//! Builders and readers retain their payloads in traced internal fields. Only
//! opaque Rust RNG/clock values need finalizers; weak registrations never root them.
#[cfg(test)]
use crate::v8::builder::ObjectExt;
use crate::v8::builder::ScopeExt;
use crate::v8::context::ValueImportError;
use crate::v8::ffi_bytes::new_bytes;
use crate::wasm_imports::{
    ExternRef, Values,
    values::{Ref, Result as ImportResult},
};
use ValueImportError::{Range, Type, V8};
use anyhow::Context;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::cell::{Cell, RefCell};
#[cfg(test)]
use v8::FunctionCallbackArguments;
use v8::{HandleScope, Local, Value};
// The type object retains an allocation template. Each instance retains that
// type as its unforgeable brand, so an abandoned callback does not invalidate
// an instance still reachable from Wasm. Payload and position are direct fields.
const TYPE_TEMPLATE: usize = 0;
const HANDLE_TYPE: usize = 0;
const HANDLE_PAYLOAD: usize = 1;
const HANDLE_POSITION: usize = 2;
const HANDLE_FIELDS: usize = 3;

struct HandleType<'s>(Local<'s, v8::Object>);

impl<'s> HandleType<'s> {
    fn new(scope: &mut HandleScope<'s>) -> anyhow::Result<Self> {
        let instance = v8::ObjectTemplate::new(scope);
        instance.set_internal_field_count(HANDLE_FIELDS);
        let template = v8::ObjectTemplate::new(scope);
        template.set_internal_field_count(1);
        let kind = template
            .new_instance(scope)
            .context("failed to create a value handle type")?;
        kind.set_internal_field(TYPE_TEMPLATE, instance.into());
        Ok(Self(kind))
    }
}

struct Handle<'s>(Local<'s, v8::Object>);

impl<'s> Handle<'s> {
    fn new(
        scope: &mut HandleScope<'s>,
        kind: Local<'s, Value>,
        payload: Local<'s, Value>,
    ) -> Result<Self, ValueImportError> {
        let kind_object = Local::<v8::Object>::try_from(kind).expect("registered handle type");
        let template = Local::<v8::ObjectTemplate>::try_from(
            kind_object
                .get_internal_field(scope, TYPE_TEMPLATE)
                .expect("handle type template"),
        )
        .expect("handle type contains an object template");
        let object = template.new_instance(scope).ok_or(V8)?;
        let zero = v8::Integer::new(scope, 0);
        object.set_internal_field(HANDLE_TYPE, kind.into());
        object.set_internal_field(HANDLE_PAYLOAD, payload.into());
        object.set_internal_field(HANDLE_POSITION, zero.into());
        Ok(Self(object))
    }

    fn from_value(
        scope: &mut HandleScope<'s>,
        value: Local<'s, Value>,
        kind: Local<'s, Value>,
    ) -> Result<Self, ValueImportError> {
        let object = Local::<v8::Object>::try_from(value)
            .map_err(|_| Type("Expected a Moonrun builder or reader"))?;
        let brand = (object.internal_field_count() == HANDLE_FIELDS)
            .then(|| object.get_internal_field(scope, HANDLE_TYPE))
            .flatten()
            .and_then(|brand| Local::<Value>::try_from(brand).ok());
        if !brand.is_some_and(|brand| brand.strict_equals(kind)) {
            return Err(Type("Incorrect Moonrun builder or reader type"));
        }
        Ok(Self(object))
    }

    fn payload(&self, scope: &mut HandleScope<'s>) -> Local<'s, Value> {
        Local::try_from(
            self.0
                .get_internal_field(scope, HANDLE_PAYLOAD)
                .expect("handle payload"),
        )
        .expect("handle payload is a value")
    }

    // The private field stores an integer as a V8 Number. Supported string and
    // array sizes are exactly representable, so this conversion is lossless.
    fn position(&self, scope: &mut HandleScope<'s>) -> usize {
        Local::<v8::Number>::try_from(
            self.0
                .get_internal_field(scope, HANDLE_POSITION)
                .expect("handle position"),
        )
        .expect("handle position is a number")
        .value() as usize
    }

    fn set_position(&self, scope: &mut HandleScope<'s>, position: usize) {
        let position = v8::Number::new(scope, position as f64);
        self.0.set_internal_field(HANDLE_POSITION, position.into());
    }
}

/// A temporary view of a V8-owned builder. The backing buffer is private: guests
/// see only the opaque handle, so they cannot detach, share, or mutate its storage.
struct Builder<'s> {
    handle: Handle<'s>,
    buffer: Local<'s, v8::ArrayBuffer>,
    used: usize,
}

impl<'s> Builder<'s> {
    fn from_value(
        scope: &mut HandleScope<'s>,
        value: Local<'s, Value>,
        kind: Local<'s, Value>,
    ) -> Result<Self, ValueImportError> {
        let handle = Handle::from_value(scope, value, kind)?;
        let buffer = Local::<v8::ArrayBuffer>::try_from(handle.payload(scope))
            .expect("builder owns its buffer");
        let used = handle.position(scope);
        Ok(Self {
            handle,
            buffer,
            used,
        })
    }

    fn append(
        &mut self,
        scope: &mut HandleScope<'s>,
        bytes: &[u8],
    ) -> Result<(), ValueImportError> {
        let used = self
            .used
            .checked_add(bytes.len())
            .ok_or(Range("Invalid builder length"))?;
        let mut store = self.buffer.get_backing_store();
        if used > store.byte_length() {
            // Geometric growth avoids one allocation per appended code unit.
            let capacity = store.byte_length().saturating_mul(2).max(64).max(used);
            let buffer = v8::ArrayBuffer::new(scope, capacity);
            let replacement = buffer.get_backing_store();
            for (source, destination) in store[..self.used]
                .iter()
                .zip(replacement[..self.used].iter())
            {
                destination.set(source.get());
            }
            self.handle
                .0
                .set_internal_field(HANDLE_PAYLOAD, buffer.into());
            self.buffer = buffer;
            store = replacement;
        }
        for (destination, byte) in store[self.used..used].iter().zip(bytes) {
            destination.set(*byte);
        }
        self.handle.set_position(scope, used);
        self.used = used;
        Ok(())
    }

    fn snapshot(&self) -> Vec<u8> {
        self.buffer.get_backing_store()[..self.used]
            .iter()
            .map(Cell::get)
            .collect()
    }
}

struct Reader<'s> {
    handle: Handle<'s>,
    source: Local<'s, Value>,
    position: usize,
}

impl<'s> Reader<'s> {
    fn from_value(
        scope: &mut HandleScope<'s>,
        value: Local<'s, Value>,
        kind: Local<'s, Value>,
    ) -> Result<Self, ValueImportError> {
        let handle = Handle::from_value(scope, value, kind)?;
        let source = handle.payload(scope);
        let position = handle.position(scope);
        Ok(Self {
            handle,
            source,
            position,
        })
    }

    fn read_code_unit(&mut self, scope: &mut HandleScope<'s>) -> Result<i32, ValueImportError> {
        let string =
            Local::<v8::String>::try_from(self.source).map_err(|_| Type("Expected a string"))?;
        if self.position >= string.length() {
            return Ok(-1);
        }
        let mut unit = [0];
        string.write(
            scope,
            &mut unit,
            self.position,
            v8::WriteOptions::NO_NULL_TERMINATION,
        );
        self.position += 1;
        self.handle.set_position(scope, self.position);
        Ok(i32::from(unit[0]))
    }

    fn read_element(
        &mut self,
        scope: &mut HandleScope<'s>,
        end: Local<'s, Value>,
    ) -> Result<Local<'s, Value>, ValueImportError> {
        // Re-read the live source on each call, preserving mutations after the
        // reader was created rather than snapshotting the source array.
        let array = self.source.to_object(scope).ok_or(V8)?;
        let key = scope.string("length");
        let length = array
            .get(scope, key.into())
            .ok_or(V8)?
            .number_value(scope)
            .ok_or(V8)?;
        if self.position as f64 >= length {
            return Ok(end);
        }
        let index = v8::Number::new(scope, self.position as f64);
        self.position += 1;
        self.handle.set_position(scope, self.position);
        array.get(scope, index.into()).ok_or(V8)
    }
}

pub(super) struct State {
    string_builder: v8::Global<v8::Object>,
    byte_builder: v8::Global<v8::Object>,
    string_reader: v8::Global<v8::Object>,
    byte_reader: v8::Global<v8::Object>,
    array_reader: v8::Global<v8::Object>,
    rng: v8::Global<v8::Object>,
    instant: v8::Global<v8::Object>,
    // Weak handles own finalizer closures, not guest values. Dropping the run
    // also drops closures for objects that have not yet been collected.
    finalizers: RefCell<Vec<v8::Weak<v8::Object>>>,
}
impl State {
    pub(super) fn new(scope: &mut HandleScope) -> anyhow::Result<Self> {
        let mut brand = || {
            let kind = HandleType::new(scope)?;
            Ok::<_, anyhow::Error>(v8::Global::new(scope, kind.0))
        };
        Ok(Self {
            string_builder: brand()?,
            byte_builder: brand()?,
            string_reader: brand()?,
            byte_reader: brand()?,
            array_reader: brand()?,
            rng: brand()?,
            instant: brand()?,
            finalizers: RefCell::new(Vec::new()),
        })
    }
    fn opaque<'s, T: 'static>(
        &self,
        scope: &mut HandleScope<'s>,
        kind: Local<'s, Value>,
        payload: T,
    ) -> ImportResult<Ref<'s>> {
        let payload = Box::new(payload);
        let pointer = (&*payload as *const T).cast_mut().cast();
        let external = v8::External::new(scope, pointer);
        let handle = Handle::new(scope, kind, external.into())?;
        let weak =
            v8::Weak::with_guaranteed_finalizer(scope, handle.0, Box::new(move || drop(payload)));
        let mut finalizers = self.finalizers.borrow_mut();
        finalizers.retain(|weak| !weak.is_empty());
        finalizers.push(weak);
        Ok(Some(ExternRef::V8(handle.0.into())))
    }
}
fn local<'s>(scope: &mut HandleScope<'s>, value: &Ref<'s>) -> Local<'s, Value> {
    match value {
        Some(ExternRef::V8(value)) => *value,
        None => v8::null(scope).into(),
    }
}
impl<'s, T> Values<'s> for super::wasm_imports::Context<'_, 's, T> {
    fn string(&mut self, value: &Ref<'s>) -> ImportResult<String> {
        let value = local(self.scope, value);
        Ok(value.to_rust_string_lossy(self.scope))
    }
    fn new_string(&mut self, value: &str) -> ImportResult<Ref<'s>> {
        Ok(Some(ExternRef::V8(self.scope.string(value).into())))
    }
    fn bytes(&mut self, value: &Ref<'s>) -> ImportResult<Vec<u8>> {
        let value = Local::<v8::Uint8Array>::try_from(local(self.scope, value))
            .map_err(|_| Type("Expected a Uint8Array"))?;
        let mut bytes = vec![0; value.byte_length()];
        value.copy_contents(&mut bytes);
        Ok(bytes)
    }
    fn new_bytes(&mut self, value: Vec<u8>) -> ImportResult<Ref<'s>> {
        Ok(Some(ExternRef::V8(new_bytes(self.scope, value)?)))
    }
    fn new_strings(&mut self, value: Vec<String>) -> ImportResult<Ref<'s>> {
        let array = v8::Array::new(self.scope, 0);
        for (index, value) in value.iter().enumerate() {
            let value = self.scope.string(value);
            array
                .set_index(self.scope, index as u32, value.into())
                .ok_or(V8)?;
        }
        Ok(Some(ExternRef::V8(array.into())))
    }
    fn array_len(&mut self, value: &Ref<'s>) -> ImportResult<i32> {
        let array = local(self.scope, value).to_object(self.scope).ok_or(V8)?;
        let key = self.scope.string("length");
        Ok(array
            .get(self.scope, key.into())
            .ok_or(V8)?
            .int32_value(self.scope)
            .ok_or(V8)?)
    }
    fn array_get(&mut self, value: &Ref<'s>, index: i32) -> ImportResult<Ref<'s>> {
        let array = local(self.scope, value).to_object(self.scope).ok_or(V8)?;
        let key = v8::Integer::new(self.scope, index);
        Ok(Some(ExternRef::V8(
            array.get(self.scope, key.into()).ok_or(V8)?,
        )))
    }
    fn is_string(&mut self, value: &Ref<'s>) -> ImportResult<i32> {
        Ok(i32::from(local(self.scope, value).is_string()))
    }
    fn string_builder(&mut self) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.string_builder);
        let buffer = v8::ArrayBuffer::new(self.scope, 0);
        Ok(Some(ExternRef::V8(
            Handle::new(self.scope, kind.into(), buffer.into())?
                .0
                .into(),
        )))
    }
    fn append_char(&mut self, value: &Ref<'s>, unit: i32) -> ImportResult<()> {
        let kind = Local::new(self.scope, &self.values.string_builder);
        let value = local(self.scope, value);
        let mut builder = Builder::from_value(self.scope, value, kind.into())?;
        if builder.used / 2 >= v8::String::MAX_LENGTH {
            return Err(Range("Invalid string length").into());
        }
        Ok(builder.append(self.scope, &(unit as u16).to_le_bytes())?)
    }
    fn finish_string(&mut self, value: &Ref<'s>) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.string_builder);
        let value = local(self.scope, value);
        let builder = Builder::from_value(self.scope, value, kind.into())?;
        let units: Vec<_> = builder.buffer.get_backing_store()[..builder.used]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0].get(), pair[1].get()]))
            .collect();
        let string = v8::String::new_from_two_byte(self.scope, &units, v8::NewStringType::Normal)
            .ok_or(Range("Invalid string length"))?;
        Ok(Some(ExternRef::V8(string.into())))
    }
    fn byte_builder(&mut self) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.byte_builder);
        let buffer = v8::ArrayBuffer::new(self.scope, 0);
        Ok(Some(ExternRef::V8(
            Handle::new(self.scope, kind.into(), buffer.into())?
                .0
                .into(),
        )))
    }
    fn append_byte(&mut self, value: &Ref<'s>, byte: i32) -> ImportResult<()> {
        let kind = Local::new(self.scope, &self.values.byte_builder);
        let value = local(self.scope, value);
        let mut builder = Builder::from_value(self.scope, value, kind.into())?;
        if builder.used >= u32::MAX as usize {
            return Err(Range("Invalid array length").into());
        }
        Ok(builder.append(self.scope, &[byte as u8])?)
    }
    fn finish_bytes(&mut self, value: &Ref<'s>) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.byte_builder);
        let value = local(self.scope, value);
        let builder = Builder::from_value(self.scope, value, kind.into())?;
        self.new_bytes(builder.snapshot())
    }
    fn string_reader(&mut self, value: &Ref<'s>) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.string_reader);
        let value = local(self.scope, value);
        Ok(Some(ExternRef::V8(
            Handle::new(self.scope, kind.into(), value)?.0.into(),
        )))
    }
    fn read_char(&mut self, value: &Ref<'s>) -> ImportResult<i32> {
        let kind = Local::new(self.scope, &self.values.string_reader);
        let value = local(self.scope, value);
        Ok(Reader::from_value(self.scope, value, kind.into())?.read_code_unit(self.scope)?)
    }
    fn byte_reader(&mut self, value: &Ref<'s>) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.byte_reader);
        let value = local(self.scope, value);
        Ok(Some(ExternRef::V8(
            Handle::new(self.scope, kind.into(), value)?.0.into(),
        )))
    }
    fn read_byte(&mut self, value: &Ref<'s>) -> ImportResult<i32> {
        let kind = Local::new(self.scope, &self.values.byte_reader);
        let value = local(self.scope, value);
        let mut reader = Reader::from_value(self.scope, value, kind.into())?;
        let end = v8::Integer::new(self.scope, -1);
        Ok(reader
            .read_element(self.scope, end.into())?
            .int32_value(self.scope)
            .ok_or(V8)?)
    }
    fn array_reader(&mut self, value: &Ref<'s>) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.array_reader);
        let value = local(self.scope, value);
        Ok(Some(ExternRef::V8(
            Handle::new(self.scope, kind.into(), value)?.0.into(),
        )))
    }
    fn read_string(&mut self, value: &Ref<'s>) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.array_reader);
        let value = local(self.scope, value);
        let mut reader = Reader::from_value(self.scope, value, kind.into())?;
        let end = self.scope.string("ffi_end_of_/string_array");
        Ok(Some(ExternRef::V8(
            reader.read_element(self.scope, end.into())?,
        )))
    }
    fn new_rng(&mut self, seed: i32) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.rng);
        self.values.opaque(
            self.scope,
            kind.into(),
            RefCell::new(StdRng::seed_from_u64(seed as u64)),
        )
    }
    fn random(&mut self, value: &Ref<'s>, upper: i32) -> ImportResult<i32> {
        if upper <= 0 {
            return Err(Range("random range upper bound must be positive").into());
        }
        let kind = Local::new(self.scope, &self.values.rng);
        let value = local(self.scope, value);
        let handle = Handle::from_value(self.scope, value, kind.into())?;
        let payload =
            Local::<v8::External>::try_from(handle.payload(self.scope)).expect("RNG payload");
        // Only new_rng creates this private brand. The rooted object keeps the
        // finalizer-owned allocation alive; no engine calls occur during access.
        let rng = unsafe { &*payload.value().cast::<RefCell<StdRng>>() };
        Ok(rng.borrow_mut().gen_range(0..upper))
    }
    fn new_instant(&mut self) -> ImportResult<Ref<'s>> {
        let kind = Local::new(self.scope, &self.values.instant);
        self.values
            .opaque(self.scope, kind.into(), std::time::Instant::now())
    }
    fn elapsed(&mut self, value: &Ref<'s>) -> ImportResult<f64> {
        let kind = Local::new(self.scope, &self.values.instant);
        let value = local(self.scope, value);
        let handle = Handle::from_value(self.scope, value, kind.into())?;
        let payload =
            Local::<v8::External>::try_from(handle.payload(self.scope)).expect("clock payload");
        // The private clock brand and the rooted object protect this allocation.
        let instant = unsafe { &*payload.value().cast::<std::time::Instant>() };
        Ok(instant.elapsed().as_secs_f64())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::v8::context::wasm_constructor;

    #[test]
    fn opaque_payloads_are_released_by_gc_and_run_teardown() {
        struct DropCount(std::rc::Rc<Cell<usize>>);
        impl Drop for DropCount {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        crate::v8::initialize(&crate::EngineConfig::default()).unwrap();
        let isolate = &mut v8::Isolate::new(Default::default());
        let scope = &mut HandleScope::new(isolate);
        let context = v8::Context::new(scope, Default::default());
        let scope = &mut v8::ContextScope::new(scope, context);
        let state = State::new(scope).unwrap();
        let drops = std::rc::Rc::new(Cell::new(0));
        {
            let scope = &mut HandleScope::new(scope);
            let kind = Local::new(scope, &state.rng);
            state
                .opaque(scope, kind.into(), DropCount(std::rc::Rc::clone(&drops)))
                .unwrap();
        }
        scope.low_memory_notification();
        assert_eq!(
            drops.get(),
            1,
            "weak finalizers must not keep abandoned payloads alive"
        );
        let kind = Local::new(scope, &state.instant);
        let live = state
            .opaque(scope, kind.into(), DropCount(std::rc::Rc::clone(&drops)))
            .unwrap();
        assert!(live.is_some());
        assert_eq!(drops.get(), 1);
        // Run teardown removes the callbacks along with this state; no guest
        // operation can subsequently dereference a surviving native object.
        drop(state);
        assert_eq!(drops.get(), 2);
    }

    #[test]
    fn builders_and_readers_survive_gc_when_only_wasm_retains_them() {
        crate::v8::initialize(&crate::EngineConfig::default()).unwrap();
        let isolate = &mut v8::Isolate::new(Default::default());
        let scope = &mut HandleScope::new(isolate);
        let context = v8::Context::new(scope, Default::default());
        let scope = &mut v8::ContextScope::new(scope, context);
        let imports = v8::Object::new(scope);
        let values = std::rc::Rc::new(State::new(scope).unwrap());
        let mut retained = Vec::new();
        crate::host_imports::register_values(&mut super::super::wasm_imports::Registry {
            object: imports,
            scope,
            data: std::rc::Rc::new(()),
            memory: std::rc::Rc::new(crate::v8::context::V8MemoryBinding::new()),
            termination: Default::default(),
            values,
            retained: &mut retained,
        })
        .unwrap();
        let test = imports.child(scope, "test");
        test.set_func(
            scope,
            "collect",
            |scope: &mut HandleScope, _: FunctionCallbackArguments, _: v8::ReturnValue| {
                scope.low_memory_notification();
            },
        );
        // No Rust Local/Global handle roots these builders or readers. Their
        // only owners are Wasm locals/globals, across repeated GC and growth.
        let wasm = wat::parse_str(r#"(module
            (import "test" "collect" (func $gc))
            (import "__moonbit_fs_unstable" "begin_create_string" (func $sb (result externref)))
            (import "__moonbit_fs_unstable" "string_append_char" (func $sa (param externref i32)))
            (import "__moonbit_fs_unstable" "finish_create_string" (func $sf (param externref) (result externref)))
            (import "__moonbit_fs_unstable" "begin_read_string" (func $sr (param externref) (result externref)))
            (import "__moonbit_fs_unstable" "string_read_char" (func $sc (param externref) (result i32)))
            (import "__moonbit_fs_unstable" "begin_create_byte_array" (func $bb (result externref)))
            (import "__moonbit_fs_unstable" "byte_array_append_byte" (func $append_byte (param externref i32)))
            (import "__moonbit_fs_unstable" "finish_create_byte_array" (func $bf (param externref) (result externref)))
            (import "__moonbit_fs_unstable" "begin_read_byte_array" (func $br (param externref) (result externref)))
            (import "__moonbit_fs_unstable" "byte_array_read_byte" (func $bc (param externref) (result i32)))
            (global $string_builder (mut externref) (ref.null extern))
            (func $eq (param i32 i32) local.get 0 local.get 1 i32.ne if unreachable end)
            (func (export "run") (local $bytes externref) (local $string_reader externref) (local $byte_reader externref) (local $i i32)
                call $sb global.set $string_builder
                call $bb local.set $bytes
                loop $append
                    global.get $string_builder local.get $i i32.const 0xd800 i32.add call $sa
                    local.get $bytes local.get $i call $append_byte
                    local.get $i i32.const 127 i32.and i32.eqz if call $gc end
                    local.get $i i32.const 1 i32.add local.tee $i i32.const 2049 i32.lt_u br_if $append
                end
                global.get $string_builder call $sf call $sr local.set $string_reader
                local.get $bytes call $bf call $br local.set $byte_reader
                ref.null extern global.set $string_builder
                ref.null extern local.set $bytes
                call $gc
                i32.const 0 local.set $i
                loop $read
                    local.get $string_reader call $sc local.get $i i32.const 0xd800 i32.add call $eq
                    local.get $byte_reader call $bc local.get $i i32.const 255 i32.and call $eq
                    local.get $i i32.const 127 i32.and i32.eqz if call $gc end
                    local.get $i i32.const 1 i32.add local.tee $i i32.const 2049 i32.lt_u br_if $read
                end
                local.get $string_reader call $sc i32.const -1 call $eq
                local.get $byte_reader call $bc i32.const -1 call $eq))"#).unwrap();
        let module = v8::WasmModuleObject::compile(scope, &wasm).unwrap();
        let constructor = wasm_constructor(scope, "Instance").unwrap();
        let instance = constructor
            .new_instance(scope, &[module.into(), imports.into()])
            .unwrap();
        let key = scope.string("exports");
        let exports =
            Local::<v8::Object>::try_from(instance.get(scope, key.into()).unwrap()).unwrap();
        let key = scope.string("run");
        let run = Local::<v8::Function>::try_from(exports.get(scope, key.into()).unwrap()).unwrap();
        let undefined = v8::undefined(scope);
        run.call(scope, undefined.into(), &[]).unwrap();
    }
}
