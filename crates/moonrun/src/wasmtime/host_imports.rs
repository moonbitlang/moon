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

//! Wasmtime-only imported memory and memory-sanitizer support.
use super::{JsString, StoreData};
use crate::memory_sanitizer::{SanitizerStack, SanitizerStackFrame};
use crate::wasm_imports::payload::JsBytes;
use std::sync::Mutex;
use wasmtime::{AsContext, Caller, ExternRef, Linker, Memory, MemoryType, Rooted, Store};
pub(super) struct State {
    ffi_bytes_memory: Option<Memory>,
}
impl State {
    pub(super) fn new() -> Self {
        Self {
            ffi_bytes_memory: None,
        }
    }
    fn ffi_bytes_memory(&self) -> wasmtime::Result<Memory> {
        self.ffi_bytes_memory
            .ok_or_else(|| wasmtime::format_err!("ffi-bytes.memory was not imported"))
    }
}
pub(super) fn define_import(
    linker: &mut Linker<StoreData>,
    namespace: &str,
    name: &str,
) -> wasmtime::Result<bool> {
    match namespace {
        "ffi-bytes" => define_ffi_bytes_import(linker, name),
        crate::memory_sanitizer::MEMORY_SANITIZER_MODULE => {
            define_memory_sanitizer_import(linker, name)
        }
        _ => Ok(false),
    }
}
fn define_memory_sanitizer_import(
    linker: &mut Linker<StoreData>,
    name: &str,
) -> wasmtime::Result<bool> {
    match name {
        "register-object-alloc" => {
            linker.func_wrap(
                crate::memory_sanitizer::MEMORY_SANITIZER_MODULE,
                name,
                |caller: Caller<'_, StoreData>, address: i32, size: i32| {
                    caller
                        .data()
                        .memory_sanitizer
                        .register_object_alloc(address as u32, size as u32, || {
                            capture_memory_sanitizer_stack(&caller)
                        })
                        .map_err(|error| {
                            wasmtime::format_err!(
                                "{}.{} failed: {error}",
                                crate::memory_sanitizer::MEMORY_SANITIZER_MODULE,
                                "register-object-alloc",
                            )
                        })
                },
            )?;
        }
        "register-object-free" => {
            linker.func_wrap(
                crate::memory_sanitizer::MEMORY_SANITIZER_MODULE,
                name,
                |caller: Caller<'_, StoreData>, address: i32| {
                    caller
                        .data()
                        .memory_sanitizer
                        .register_object_free(address as u32)
                        .map_err(|error| {
                            wasmtime::format_err!(
                                "{}.{} failed: {error}",
                                crate::memory_sanitizer::MEMORY_SANITIZER_MODULE,
                                "register-object-free",
                            )
                        })
                },
            )?;
        }
        "object-is-valid" => {
            linker.func_wrap(
                crate::memory_sanitizer::MEMORY_SANITIZER_MODULE,
                name,
                |caller: Caller<'_, StoreData>, address: i32| {
                    i32::from(
                        caller
                            .data()
                            .memory_sanitizer
                            .object_is_valid(address as u32),
                    )
                },
            )?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

pub(super) fn define_ffi_bytes_memory(
    linker: &mut Linker<StoreData>,
    store: &mut Store<StoreData>,
) -> wasmtime::Result<()> {
    let memory = Memory::new(&mut *store, MemoryType::new(1, None))?;
    store.data_mut().host_imports.ffi_bytes_memory = Some(memory);
    linker.define(&mut *store, "ffi-bytes", "memory", memory)?;
    Ok(())
}

fn define_ffi_bytes_import(linker: &mut Linker<StoreData>, name: &str) -> wasmtime::Result<bool> {
    match name {
        "from_memory" => {
            linker.func_wrap(
                "ffi-bytes",
                name,
                |mut caller: Caller<'_, StoreData>, offset: i32, length: i32| {
                    let offset = usize::try_from(offset)?;
                    let length = usize::try_from(length)?;
                    let memory = caller.data().host_imports.ffi_bytes_memory()?;
                    let end = offset
                        .checked_add(length)
                        .ok_or_else(|| wasmtime::format_err!("ffi-bytes range overflow"))?;
                    let bytes = memory
                        .data(&caller)
                        .get(offset..end)
                        .ok_or_else(|| wasmtime::format_err!("ffi-bytes range out of bounds"))?
                        .to_vec();
                    ExternRef::new(&mut caller, JsBytes(Mutex::new(bytes)))
                },
            )?;
        }
        "new" => {
            linker.func_wrap(
                "ffi-bytes",
                name,
                |mut caller: Caller<'_, StoreData>, length: i32| {
                    let length = usize::try_from(length)?;
                    ExternRef::new(&mut caller, JsBytes(Mutex::new(vec![0; length])))
                },
            )?;
        }
        "get" => {
            linker.func_wrap(
                "ffi-bytes",
                name,
                |caller: Caller<'_, StoreData>, bytes: Option<Rooted<ExternRef>>, index: i32| {
                    let index = usize::try_from(index)?;
                    let value = with_extern_data::<JsBytes, _>(&caller, bytes.as_ref(), |bytes| {
                        bytes
                            .0
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .get(index)
                            .copied()
                    })?
                    .ok_or_else(|| wasmtime::format_err!("ffi-bytes index out of bounds"))?;
                    Ok(i32::from(value))
                },
            )?;
        }
        "set" => {
            linker.func_wrap(
                "ffi-bytes",
                name,
                |caller: Caller<'_, StoreData>,
                 bytes: Option<Rooted<ExternRef>>,
                 index: i32,
                 value: i32| {
                    let index = usize::try_from(index)?;
                    with_extern_data::<JsBytes, _>(&caller, bytes.as_ref(), |bytes| {
                        let mut bytes = bytes
                            .0
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        let output = bytes.get_mut(index).ok_or_else(|| {
                            wasmtime::format_err!("ffi-bytes index out of bounds")
                        })?;
                        *output = value as u8;
                        Ok::<(), wasmtime::Error>(())
                    })??;
                    Ok(())
                },
            )?;
        }
        "copy" => {
            linker.func_wrap(
                "ffi-bytes",
                name,
                |caller: Caller<'_, StoreData>,
                 destination: Option<Rooted<ExternRef>>,
                 destination_offset: i32,
                 source: Option<Rooted<ExternRef>>,
                 source_offset: i32,
                 length: i32| {
                    let destination_offset = usize::try_from(destination_offset)?;
                    let source = extern_bytes(&caller, source.as_ref())?;
                    let source_offset = usize::try_from(source_offset)?;
                    let length = usize::try_from(length)?;
                    let source_end = source_offset
                        .checked_add(length)
                        .ok_or_else(|| wasmtime::format_err!("ffi-bytes source range overflow"))?;
                    let source = source.get(source_offset..source_end).ok_or_else(|| {
                        wasmtime::format_err!("ffi-bytes source range out of bounds")
                    })?;
                    with_extern_data::<JsBytes, _>(
                        &caller,
                        destination.as_ref(),
                        |destination| {
                            let mut destination = destination
                                .0
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let destination_end =
                                destination_offset.checked_add(length).ok_or_else(|| {
                                    wasmtime::format_err!("ffi-bytes destination range overflow")
                                })?;
                            let output = destination
                                .get_mut(destination_offset..destination_end)
                                .ok_or_else(|| {
                                wasmtime::format_err!("ffi-bytes destination range out of bounds")
                            })?;
                            output.copy_from_slice(source);
                            Ok::<(), wasmtime::Error>(())
                        },
                    )??;
                    Ok(())
                },
            )?;
        }
        "fill" => {
            linker.func_wrap(
                "ffi-bytes",
                name,
                |caller: Caller<'_, StoreData>,
                 bytes: Option<Rooted<ExternRef>>,
                 start: i32,
                 value: i32,
                 length: i32| {
                    let start = usize::try_from(start)?;
                    let length = usize::try_from(length)?;
                    let end = start
                        .checked_add(length)
                        .ok_or_else(|| wasmtime::format_err!("ffi-bytes range overflow"))?;
                    with_extern_data::<JsBytes, _>(&caller, bytes.as_ref(), |bytes| {
                        bytes
                            .0
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .get_mut(start..end)
                            .ok_or_else(|| wasmtime::format_err!("ffi-bytes range out of bounds"))?
                            .fill(value as u8);
                        Ok::<(), wasmtime::Error>(())
                    })??;
                    Ok(())
                },
            )?;
        }
        "length" => {
            linker.func_wrap(
                "ffi-bytes",
                name,
                |caller: Caller<'_, StoreData>,
                 bytes: Option<Rooted<ExternRef>>|
                 -> wasmtime::Result<i32> {
                    Ok(with_extern_data::<JsBytes, _>(
                        &caller,
                        bytes.as_ref(),
                        |bytes| {
                            i32::try_from(
                                bytes
                                    .0
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                                    .len(),
                            )
                        },
                    )??)
                },
            )?;
        }
        "equals" => {
            linker.func_wrap(
                "ffi-bytes",
                name,
                |caller: Caller<'_, StoreData>,
                 left: Option<Rooted<ExternRef>>,
                 right: Option<Rooted<ExternRef>>| {
                    Ok(i32::from(
                        extern_bytes(&caller, left.as_ref())?
                            == extern_bytes(&caller, right.as_ref())?,
                    ))
                },
            )?;
        }
        "asString" => {
            linker.func_wrap(
                "ffi-bytes",
                name,
                |mut caller: Caller<'_, StoreData>,
                 bytes: Option<Rooted<ExternRef>>,
                 start: i32,
                 length: i32| {
                    let bytes = extern_bytes(&caller, bytes.as_ref())?;
                    let start = usize::try_from(start)?;
                    let length = usize::try_from(length)?;
                    let end = start
                        .checked_add(length)
                        .ok_or_else(|| wasmtime::format_err!("ffi-bytes range overflow"))?;
                    let bytes = bytes
                        .get(start..end)
                        .ok_or_else(|| wasmtime::format_err!("ffi-bytes range out of bounds"))?;
                    let units = bytes
                        .chunks_exact(2)
                        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                        .collect::<Vec<_>>();
                    ExternRef::new(&mut caller, JsString(units.into()))
                },
            )?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn capture_memory_sanitizer_stack(caller: &Caller<'_, StoreData>) -> SanitizerStack {
    let frames = wasmtime::WasmBacktrace::capture(caller)
        .frames()
        .iter()
        .map(|frame| {
            SanitizerStackFrame::new(
                frame
                    .func_name()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("wasm-function[{}]", frame.func_index())),
                true,
            )
        })
        .collect();
    SanitizerStack::new(frames)
}

fn with_extern_data<T: 'static, R>(
    caller: &Caller<'_, StoreData>,
    reference: Option<&Rooted<ExternRef>>,
    f: impl FnOnce(&T) -> R,
) -> wasmtime::Result<R> {
    let reference = reference.ok_or_else(|| wasmtime::format_err!("host value received null"))?;
    let data = reference
        .data(caller.as_context())?
        .ok_or_else(|| wasmtime::format_err!("externref has no host data"))?;
    let data = data
        .downcast_ref::<Box<dyn std::any::Any + Send + Sync>>()
        .map_or(data, |data| data.as_ref());
    let value = data
        .downcast_ref::<T>()
        .ok_or_else(|| wasmtime::format_err!("unexpected externref host value"))?;
    Ok(f(value))
}

fn extern_bytes(
    caller: &Caller<'_, StoreData>,
    value: Option<&Rooted<ExternRef>>,
) -> wasmtime::Result<Vec<u8>> {
    with_extern_data::<JsBytes, _>(caller, value, |bytes| {
        bytes
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    })
}
