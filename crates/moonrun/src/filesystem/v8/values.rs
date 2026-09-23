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

//! Value helpers exposed by `__moonbit_fs_unstable`.
//!
//! V8 owns all guest-visible state. Builders contain a private ArrayBuffer and
//! used-byte count; readers retain their live source and cursor. Rust only owns
//! temporary copies. Internal fields are traced GC edges, never Rust pointers.

use anyhow::Context;
use std::cell::Cell;
use v8::{FunctionCallbackArguments, HandleScope, Local, Value};

use crate::v8::builder::{ObjectExt, ScopeExt};
use crate::v8::context::{ValueImportError, value_import};
use crate::v8::ffi_bytes::new_bytes;
use ValueImportError::{Range, Type, V8};

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

    fn register(
        &self,
        scope: &mut HandleScope<'s>,
        imports: Local<'s, v8::Object>,
        name: &str,
        callback: impl v8::MapFnTo<v8::FunctionCallback>,
    ) -> anyhow::Result<()> {
        let function = v8::Function::builder(callback)
            .data(self.0.into())
            .build(scope)
            .with_context(|| format!("failed to create Moonrun's {name} import"))?;
        imports.set_value(scope, name, function.into());
        Ok(())
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

fn begin_builder<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let buffer = v8::ArrayBuffer::new(scope, 0);
        Ok(Handle::new(scope, args.data(), buffer.into())?.0.into())
    });
}

fn string_append_char<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let unit = args.get(1).uint32_value(scope).ok_or(V8)? as u16;
        let mut builder = Builder::from_value(scope, args.get(0), args.data())?;
        if builder.used / 2 >= v8::String::MAX_LENGTH {
            return Err(Range("Invalid string length"));
        }
        builder.append(scope, &unit.to_le_bytes())?;
        Ok(v8::undefined(scope).into())
    });
}

fn finish_create_string<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let builder = Builder::from_value(scope, args.get(0), args.data())?;
        let units: Vec<_> = builder.buffer.get_backing_store()[..builder.used]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0].get(), pair[1].get()]))
            .collect();
        v8::String::new_from_two_byte(scope, &units, v8::NewStringType::Normal)
            .map(Into::into)
            .ok_or(Range("Invalid string length"))
    });
}

fn byte_array_append_byte<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let byte = args.get(1).uint32_value(scope).ok_or(V8)? as u8;
        let mut builder = Builder::from_value(scope, args.get(0), args.data())?;
        if builder.used >= u32::MAX as usize {
            return Err(Range("Invalid array length"));
        }
        builder.append(scope, &[byte])?;
        Ok(v8::undefined(scope).into())
    });
}

fn finish_create_byte_array<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let builder = Builder::from_value(scope, args.get(0), args.data())?;
        new_bytes(scope, builder.snapshot())
    });
}

fn begin_reader<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        Ok(Handle::new(scope, args.data(), args.get(0))?.0.into())
    });
}

fn string_read_char<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let mut reader = Reader::from_value(scope, args.get(0), args.data())?;
        let unit = reader.read_code_unit(scope)?;
        Ok(v8::Integer::new(scope, unit).into())
    });
}

fn byte_array_read_byte<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let mut reader = Reader::from_value(scope, args.get(0), args.data())?;
        let end = v8::Integer::new(scope, -1);
        reader.read_element(scope, end.into())
    });
}

fn string_array_read_string<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let mut reader = Reader::from_value(scope, args.get(0), args.data())?;
        let end = scope.string("ffi_end_of_/string_array");
        reader.read_element(scope, end.into())
    });
}

fn finish_read(
    _scope: &mut HandleScope,
    _args: FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    // The ABI's finish operations do not invalidate a reader.
    ret.set_undefined();
}

fn array_len<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let array = args.get(0).to_object(scope).ok_or(V8)?;
        let key = scope.string("length");
        array.get(scope, key.into()).ok_or(V8)
    });
}

fn array_get<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let array = args.get(0).to_object(scope).ok_or(V8)?;
        array.get(scope, args.get(1)).ok_or(V8)
    });
}

fn jsvalue_is_string(
    _scope: &mut HandleScope,
    args: FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    ret.set_bool(args.get(0).is_string());
}

pub(super) fn register<'s>(
    imports: Local<'s, v8::Object>,
    scope: &mut HandleScope<'s>,
) -> anyhow::Result<()> {
    let string_builder = HandleType::new(scope)?;
    string_builder.register(scope, imports, "begin_create_string", begin_builder)?;
    string_builder.register(scope, imports, "string_append_char", string_append_char)?;
    string_builder.register(scope, imports, "finish_create_string", finish_create_string)?;
    let byte_builder = HandleType::new(scope)?;
    byte_builder.register(scope, imports, "begin_create_byte_array", begin_builder)?;
    byte_builder.register(
        scope,
        imports,
        "byte_array_append_byte",
        byte_array_append_byte,
    )?;
    byte_builder.register(
        scope,
        imports,
        "finish_create_byte_array",
        finish_create_byte_array,
    )?;
    let string_reader = HandleType::new(scope)?;
    string_reader.register(scope, imports, "begin_read_string", begin_reader)?;
    string_reader.register(scope, imports, "string_read_char", string_read_char)?;
    let byte_reader = HandleType::new(scope)?;
    byte_reader.register(scope, imports, "begin_read_byte_array", begin_reader)?;
    byte_reader.register(scope, imports, "byte_array_read_byte", byte_array_read_byte)?;
    let string_array_reader = HandleType::new(scope)?;
    string_array_reader.register(scope, imports, "begin_read_string_array", begin_reader)?;
    string_array_reader.register(
        scope,
        imports,
        "string_array_read_string",
        string_array_read_string,
    )?;
    imports.set_func(scope, "finish_read_string", finish_read);
    imports.set_func(scope, "finish_read_byte_array", finish_read);
    imports.set_func(scope, "finish_read_string_array", finish_read);
    imports.set_func(scope, "array_len", array_len);
    imports.set_func(scope, "array_get", array_get);
    imports.set_func(scope, "jsvalue_is_string", jsvalue_is_string);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v8::context::wasm_constructor;

    #[test]
    fn builders_and_readers_survive_gc_when_only_wasm_retains_them() {
        crate::v8::initialize(&crate::EngineConfig::default()).unwrap();
        let isolate = &mut v8::Isolate::new(Default::default());
        let scope = &mut HandleScope::new(isolate);
        let context = v8::Context::new(scope, Default::default());
        let scope = &mut v8::ContextScope::new(scope, context);
        let imports = v8::Object::new(scope);
        let values = imports.child(scope, "__moonbit_fs_unstable");
        register(values, scope).unwrap();
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
            (import "__moonbit_fs_unstable" "byte_array_append_byte" (func $ba (param externref i32)))
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
                    local.get $bytes local.get $i call $ba
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
