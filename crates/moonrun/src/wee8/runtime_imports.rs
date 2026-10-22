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

//! Runtime compatibility imports: native memory/strings and the legacy byte
//! representation. Host services remain in the shared host registrations.
use super::{StoreData, api, wasm_imports::Registry};
use crate::memory_sanitizer::{MEMORY_SANITIZER_MODULE, SanitizerStack, SanitizerStackFrame};
use crate::wasm_imports::payload::JsBytes;
use std::{any::Any, sync::Mutex};

pub(super) fn register(registry: &mut Registry<'_>) -> api::Result<()> {
    use api::Kind::{ExternRef, I32};
    use api::Value::{ExternRef as Ref, I32 as Int};
    let memory = if registry.needs_import("ffi-bytes", "memory") {
        let memory = api::Memory::new(
            registry.store.as_context(),
            api::MemoryType {
                minimum: 1,
                maximum: None,
            },
        )?;
        registry.define_memory("ffi-bytes", "memory", memory)?;
        Some(memory)
    } else {
        None
    };
    registry.define_func(
        "ffi-bytes",
        "from_memory",
        &[I32, I32],
        &[ExternRef],
        move |caller, args| {
            let [Int(offset), Int(length)] = args else {
                unreachable!()
            };
            let memory = memory.ok_or("ffi-bytes.memory was not imported")?;
            let bytes = memory.with_data(caller.as_context(), |bytes| {
                Ok::<_, api::Error>(
                    bytes
                        .get(range(*offset, *length)?)
                        .ok_or("ffi-bytes range out of bounds")?
                        .to_vec(),
                )
            })??;
            Ok(vec![new_bytes(&caller, bytes)?])
        },
    )?;
    registry.define_func("ffi-bytes", "new", &[I32], &[ExternRef], |caller, args| {
        let [Int(length)] = args else { unreachable!() };
        let length = usize::try_from(*length).map_err(|_| "negative byte length")?;
        Ok(vec![new_bytes(&caller, vec![0; length])?])
    })?;
    registry.define_func("ffi-bytes", "get", &[ExternRef, I32], &[I32], |_, args| {
        let [Ref(value), Int(index)] = args else {
            unreachable!()
        };
        let bytes = bytes(value)?.0.lock().unwrap_or_else(|e| e.into_inner());
        let byte = bytes
            .get(*index as u32 as usize)
            .ok_or("ffi-bytes index out of bounds")?;
        Ok(vec![Int(i32::from(*byte))])
    })?;
    registry.define_func(
        "ffi-bytes",
        "set",
        &[ExternRef, I32, I32],
        &[],
        |_, args| {
            let [Ref(value), Int(index), Int(byte)] = args else {
                unreachable!()
            };
            let mut bytes = bytes(value)?.0.lock().unwrap_or_else(|e| e.into_inner());
            *bytes
                .get_mut(*index as u32 as usize)
                .ok_or("ffi-bytes index out of bounds")? = *byte as u8;
            Ok(vec![])
        },
    )?;
    registry.define_func(
        "ffi-bytes",
        "copy",
        &[ExternRef, I32, ExternRef, I32, I32],
        &[],
        |_, args| {
            let [
                Ref(destination),
                Int(destination_offset),
                Ref(source),
                Int(source_offset),
                Int(length),
            ] = args
            else {
                unreachable!()
            };
            // Copy the source before taking the destination lock; the buffers may
            // alias and overlapping copies have memmove semantics.
            let source = bytes(source)?
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(range(*source_offset, *length)?)
                .ok_or("ffi-bytes source range out of bounds")?
                .to_vec();
            bytes(destination)?
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_mut(range(*destination_offset, *length)?)
                .ok_or("ffi-bytes destination range out of bounds")?
                .copy_from_slice(&source);
            Ok(vec![])
        },
    )?;
    registry.define_func(
        "ffi-bytes",
        "fill",
        &[ExternRef, I32, I32, I32],
        &[],
        |_, args| {
            let [Ref(value), Int(start), Int(byte), Int(length)] = args else {
                unreachable!()
            };
            bytes(value)?
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_mut(range(*start, *length)?)
                .ok_or("ffi-bytes range out of bounds")?
                .fill(*byte as u8);
            Ok(vec![])
        },
    )?;
    registry.define_func("ffi-bytes", "length", &[ExternRef], &[I32], |_, args| {
        let [Ref(value)] = args else { unreachable!() };
        let length = bytes(value)?
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len();
        Ok(vec![Int(
            i32::try_from(length).map_err(|_| "byte length exceeds i32")?
        )])
    })?;
    registry.define_func(
        "ffi-bytes",
        "equals",
        &[ExternRef, ExternRef],
        &[I32],
        |_, args| {
            let [Ref(left), Ref(right)] = args else {
                unreachable!()
            };
            let left = bytes(left)?
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            let equal = left == *bytes(right)?.0.lock().unwrap_or_else(|e| e.into_inner());
            Ok(vec![Int(i32::from(equal))])
        },
    )?;
    registry.define_func(
        "ffi-bytes",
        "asString",
        &[ExternRef, I32, I32],
        &[ExternRef],
        |caller, args| {
            let [Ref(value), Int(start), Int(length)] = args else {
                unreachable!()
            };
            let units = bytes(value)?
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(range(*start, *length)?)
                .ok_or("ffi-bytes range out of bounds")?
                .chunks_exact(2)
                .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                .collect::<Vec<_>>();
            let string = api::StringRef::from_utf16(caller.as_context(), &units)?;
            Ok(vec![Ref(Some(string.into_extern_ref()))])
        },
    )?;
    registry.define_func("console", "log", &[ExternRef], &[], |caller, args| {
        let [Ref(value)] = args else { unreachable!() };
        let string = api::StringRef::try_from(value.clone().ok_or("console.log received null")?)?;
        caller
            .with_data(|data| {
                data.runtime
                    .stdio()
                    .with_stdout(|out| writeln!(out, "{}", string.to_string_lossy()))
            })
            .map_err(|error| api::Error::Host(error.to_string().into()))?;
        Ok(vec![])
    })?;
    registry.define_func(
        MEMORY_SANITIZER_MODULE,
        "register-object-alloc",
        &[I32, I32],
        &[],
        |caller, args| {
            let [Int(size), Int(address)] = args else {
                unreachable!()
            };
            let trace = caller.capture_stack()?;
            caller
                .with_data(|data| {
                    data.memory_sanitizer.register_object_alloc(
                        *size as u32,
                        *address as u32,
                        || {
                            SanitizerStack::new(
                                trace
                                    .iter()
                                    .map(|frame| {
                                        SanitizerStackFrame::new(
                                            data.function_name(frame.function_index),
                                            true,
                                        )
                                    })
                                    .collect(),
                            )
                        },
                    )
                })
                .map_err(|error| {
                    api::Error::Host(
                        format!("{MEMORY_SANITIZER_MODULE}.register-object-alloc failed: {error}")
                            .into(),
                    )
                })?;
            Ok(vec![])
        },
    )?;
    registry.define_func(
        MEMORY_SANITIZER_MODULE,
        "register-object-free",
        &[I32],
        &[],
        |caller, args| {
            let [Int(address)] = args else { unreachable!() };
            caller
                .with_data(|data| data.memory_sanitizer.register_object_free(*address as u32))
                .map_err(|error| {
                    api::Error::Host(
                        format!("{MEMORY_SANITIZER_MODULE}.register-object-free failed: {error}")
                            .into(),
                    )
                })?;
            Ok(vec![])
        },
    )?;
    registry.define_func(
        MEMORY_SANITIZER_MODULE,
        "object-is-valid",
        &[I32],
        &[I32],
        |caller, args| {
            let [Int(address)] = args else { unreachable!() };
            Ok(vec![Int(i32::from(caller.with_data(|data| {
                data.memory_sanitizer.object_is_valid(*address as u32)
            })))])
        },
    )?;
    Ok(())
}

fn range(start: i32, length: i32) -> api::Result<std::ops::Range<usize>> {
    let start = usize::try_from(start).map_err(|_| "negative byte offset")?;
    let length = usize::try_from(length).map_err(|_| "negative byte length")?;
    let end = start
        .checked_add(length)
        .ok_or("ffi-bytes range overflow")?;
    Ok(start..end)
}
fn bytes<'a>(reference: &'a Option<api::ExternRef<'_>>) -> api::Result<&'a JsBytes> {
    reference
        .as_ref()
        .and_then(|value| value.data::<Box<dyn Any + Send + Sync>>())
        .and_then(|data| data.downcast_ref::<JsBytes>())
        .ok_or_else(|| "unexpected byte reference".into())
}
fn new_bytes<'a>(
    caller: &api::Caller<'a, StoreData>,
    value: Vec<u8>,
) -> api::Result<api::Value<'a>> {
    let data: Box<dyn Any + Send + Sync> = Box::new(JsBytes(Mutex::new(value)));
    Ok(api::Value::ExternRef(Some(caller.extern_ref(data)?)))
}
