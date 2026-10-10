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

//! The `ffi-bytes` ABI. Byte values and the imported memory belong to V8;
//! Rust borrows checked backing-store ranges only for one synchronous operation.

use super::builder::ObjectExt;
use super::context::{ValueImportError, value_import, wasm_constructor};
use ValueImportError::{Range, Type, V8};
use anyhow::Context;
use std::cell::Cell;
use v8::{FunctionCallbackArguments, HandleScope, Local, Value};

pub(crate) fn new_bytes<'s>(
    scope: &mut HandleScope<'s>,
    bytes: Vec<u8>,
) -> Result<Local<'s, Value>, ValueImportError> {
    let length = bytes.len();
    if length > v8::Uint8Array::MAX_LENGTH {
        return Err(Range("Invalid typed array length"));
    }
    let store = v8::ArrayBuffer::new_backing_store_from_bytes(bytes).make_shared();
    let buffer = v8::ArrayBuffer::with_backing_store(scope, &store);
    v8::Uint8Array::new(scope, buffer, 0, length)
        .map(Into::into)
        .ok_or(V8)
}

fn uint8_array<'s>(value: Local<'s, Value>) -> Result<Local<'s, v8::Uint8Array>, ValueImportError> {
    Local::<v8::Uint8Array>::try_from(value).map_err(|_| Type("Expected a Uint8Array"))
}

/// A checked view, used only after argument conversion has finished. No V8
/// calls may occur while borrowing its cells: those calls could re-enter guest
/// code and detach or resize a buffer. Shared buffers cannot be read as Cells.
struct ByteView {
    store: v8::SharedRef<v8::BackingStore>,
    range: std::ops::Range<usize>,
}

impl ByteView {
    fn new(
        scope: &mut HandleScope,
        array: Local<v8::Uint8Array>,
    ) -> Result<Self, ValueImportError> {
        let buffer = array.buffer(scope).ok_or(V8)?;
        if buffer.was_detached() {
            return Err(Type("Cannot operate on a detached ArrayBuffer"));
        }
        let store = buffer.get_backing_store();
        if store.is_shared() {
            return Err(Type("Expected an unshared byte array"));
        }
        let start = array.byte_offset();
        let end = start
            .checked_add(array.byte_length())
            .ok_or(Range("Byte array is outside its backing store"))?;
        if end > store.byte_length() {
            return Err(Range("Byte array is outside its backing store"));
        }
        Ok(Self {
            store,
            range: start..end,
        })
    }

    fn cells(&self) -> &[Cell<u8>] {
        &self.store[self.range.clone()]
    }
}

// ArrayBuffer.slice and TypedArray.subarray/fill use relative, clamped indices.
// Perform the addition before clamping, without wrapping Wasm i32 operands.
fn relative_index(index: f64, length: usize) -> usize {
    let index = index.trunc();
    if index.is_nan() {
        0
    } else if index < 0.0 {
        (length as f64 + index).clamp(0.0, length as f64) as usize
    } else {
        index.min(length as f64) as usize
    }
}

fn bytes_from_memory<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let start = args.get(0).number_value(scope).ok_or(V8)?;
        let length = args.get(1).number_value(scope).ok_or(V8)?;
        let memory = Local::<v8::WasmMemoryObject>::try_from(args.data())
            .expect("from_memory retains its imported Wasm memory");
        // memory.grow replaces the ArrayBuffer; never retain yesterday's buffer.
        let store = memory.buffer().get_backing_store();
        let end = relative_index(start + length, store.byte_length());
        let start = relative_index(start, store.byte_length()).min(end);
        let bytes = store[start..end].iter().map(Cell::get).collect();
        new_bytes(scope, bytes)
    });
}

fn bytes_new<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let length = args.get(0).number_value(scope).ok_or(V8)?;
        let length = if length.is_nan() { 0.0 } else { length.trunc() };
        if length < 0.0 || length > v8::Uint8Array::MAX_LENGTH as f64 {
            return Err(Range("Invalid typed array length"));
        }
        let buffer = v8::ArrayBuffer::new(scope, length as usize);
        v8::Uint8Array::new(scope, buffer, 0, length as usize)
            .map(Into::into)
            .ok_or(V8)
    });
}

fn bytes_get<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let array = uint8_array(args.get(0))?;
        array.get(scope, args.get(1)).ok_or(V8)
    });
}

fn bytes_set<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let array = uint8_array(args.get(0))?;
        array.set(scope, args.get(1), args.get(2)).ok_or(V8)?;
        // Assignment returns the original value, before Uint8 conversion.
        Ok(args.get(2))
    });
}

fn bytes_copy<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let destination = uint8_array(args.get(0))?;
        let source = uint8_array(args.get(2))?;
        let source_start = args.get(3).number_value(scope).ok_or(V8)?;
        let length = args.get(4).number_value(scope).ok_or(V8)?;
        let destination_start = args.get(1).number_value(scope).ok_or(V8)?;
        let destination_start = if destination_start.is_nan() {
            0.0
        } else {
            destination_start.trunc()
        };
        let source = ByteView::new(scope, source)?;
        let destination = ByteView::new(scope, destination)?;
        let end = relative_index(source_start + length, source.range.len());
        let start = relative_index(source_start, source.range.len()).min(end);
        let length = end - start;
        if destination_start < 0.0
            || destination_start > destination.range.len().saturating_sub(length) as f64
            || length > destination.range.len()
        {
            return Err(Range("Source is too large for the destination byte array"));
        }
        let destination_start = destination_start as usize;
        let src = &source.cells()[start..end];
        let dst = &destination.cells()[destination_start..destination_start + length];
        // SAFETY: both checked ranges belong to live, unshared backing stores.
        // Cell<u8> has u8's layout and permits mutation through shared references.
        // No V8 calls can detach/resize the buffers while these cells are borrowed.
        // copy (not copy_nonoverlapping) preserves TypedArray.set's aliasing rules.
        unsafe { std::ptr::copy(src.as_ptr(), dst.as_ptr().cast_mut(), length) };
        Ok(v8::undefined(scope).into())
    });
}

fn bytes_fill<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let array = uint8_array(args.get(0))?;
        let start = args.get(1).number_value(scope).ok_or(V8)?;
        let byte = args.get(2).uint32_value(scope).ok_or(V8)? as u8;
        let length = args.get(3).number_value(scope).ok_or(V8)?;
        let view = ByteView::new(scope, array)?;
        let end = relative_index(start + length, view.range.len());
        let start = relative_index(start, view.range.len()).min(end);
        for cell in &view.cells()[start..end] {
            cell.set(byte);
        }
        Ok(array.into())
    });
}

fn bytes_length<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let array = uint8_array(args.get(0))?;
        Ok(v8::Number::new(scope, array.length() as f64).into())
    });
}

fn bytes_equals<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let left = uint8_array(args.get(0))?;
        let right = uint8_array(args.get(1))?;
        if left.length() != right.length() {
            return Ok(v8::Integer::new(scope, 0).into());
        }
        let left = ByteView::new(scope, left)?;
        let right = ByteView::new(scope, right)?;
        let equal = left
            .cells()
            .iter()
            .zip(right.cells())
            .all(|(a, b)| a.get() == b.get());
        Ok(v8::Integer::new(scope, i32::from(equal)).into())
    });
}

fn bytes_as_string<'s>(
    scope: &mut HandleScope<'s>,
    args: FunctionCallbackArguments<'s>,
    ret: v8::ReturnValue,
) {
    value_import(scope, ret, |scope| {
        let array = uint8_array(args.get(0))?;
        let start = args.get(1).number_value(scope).ok_or(V8)?;
        let length = args.get(2).number_value(scope).ok_or(V8)?;
        let view = ByteView::new(scope, array)?;
        let end = relative_index(start + length, view.range.len());
        let start = relative_index(start, view.range.len()).min(end);
        if (end - start) / 2 > v8::String::MAX_LENGTH {
            return Err(Range("Invalid string length"));
        }
        let units = view.cells()[start..end]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0].get(), pair[1].get()]))
            .collect::<Vec<_>>();
        v8::String::new_from_two_byte(scope, &units, v8::NewStringType::Normal)
            .map(Into::into)
            .ok_or(Range("Invalid string length"))
    });
}

pub(super) fn install<'s>(
    scope: &mut HandleScope<'s>,
    bytes: Local<'s, v8::Object>,
) -> anyhow::Result<()> {
    let memory_constructor = wasm_constructor(scope, "Memory")?;
    let memory_type = v8::Object::new(scope);
    let initial = v8::Integer::new(scope, 1);
    memory_type.set_value(scope, "initial", initial.into());
    let memory = memory_constructor
        .new_instance(scope, &[memory_type.into()])
        .and_then(|value| Local::<v8::WasmMemoryObject>::try_from(value).ok())
        .context("failed to create Moonrun's ffi-bytes memory")?;
    let from_memory = v8::Function::builder(bytes_from_memory)
        .data(memory.into())
        .build(scope)
        .context("failed to create Moonrun's byte memory callback")?;
    bytes.set_value(scope, "memory", memory.into());
    bytes.set_value(scope, "from_memory", from_memory.into());
    bytes.set_func(scope, "new", bytes_new);
    bytes.set_func(scope, "get", bytes_get);
    bytes.set_func(scope, "set", bytes_set);
    bytes.set_func(scope, "copy", bytes_copy);
    bytes.set_func(scope, "fill", bytes_fill);
    bytes.set_func(scope, "length", bytes_length);
    bytes.set_func(scope, "equals", bytes_equals);
    bytes.set_func(scope, "asString", bytes_as_string);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_copy_respects_view_offsets_and_empty_buffers() {
        crate::v8::initialize(&crate::EngineConfig::default()).unwrap();
        let isolate = &mut v8::Isolate::new(Default::default());
        let scope = &mut HandleScope::new(isolate);
        let context = v8::Context::new(scope, Default::default());
        let scope = &mut v8::ContextScope::new(scope, context);
        let copy = v8::Function::new(scope, bytes_copy).unwrap();
        let receiver = v8::undefined(scope).into();
        let zero = v8::Integer::new(scope, 0).into();

        let store = v8::ArrayBuffer::new_backing_store_from_bytes(vec![0, 1, 2, 3, 4, 5, 6, 7])
            .make_shared();
        let buffer = v8::ArrayBuffer::with_backing_store(scope, &store);
        let source = v8::Uint8Array::new(scope, buffer, 1, 4).unwrap();
        let destination = v8::Uint8Array::new(scope, buffer, 2, 4).unwrap();
        let length = v8::Integer::new(scope, 4).into();
        copy.call(
            scope,
            receiver,
            &[destination.into(), zero, source.into(), zero, length],
        )
        .unwrap();
        assert_eq!(
            store.iter().map(Cell::get).collect::<Vec<_>>(),
            [0, 1, 1, 2, 3, 4, 6, 7]
        );

        let buffer = v8::ArrayBuffer::new(scope, 0);
        let empty = v8::Uint8Array::new(scope, buffer, 0, 0).unwrap();
        copy.call(
            scope,
            receiver,
            &[empty.into(), zero, empty.into(), zero, zero],
        )
        .unwrap();
    }
}
