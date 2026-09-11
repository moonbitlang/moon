// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Wasmtime lowering for Moonrun's synchronous, JS-shaped Wasm imports.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use wasmtime::{AsContext, Caller, ExternRef, Linker, Memory, MemoryType, Rooted, Store};

use super::{JsString, StoreData};
use crate::filesystem::FsOperationResults;
use crate::memory_sanitizer::{SanitizerStack, SanitizerStackFrame};

const FS_MODULE: &str = "__moonbit_fs_unstable";
const IO_MODULE: &str = "__moonbit_io_unstable";
const RAND_MODULE: &str = "__moonbit_rand_unstable";
const TIME_MODULE: &str = "__moonbit_time_unstable";

pub(super) struct State {
    arguments: Vec<String>,
    filesystem_results: FsOperationResults,
    ffi_bytes_memory: Option<Memory>,
}

impl State {
    pub(super) fn new(module_name: &str, arguments: &[String]) -> Self {
        Self {
            arguments: std::iter::once(module_name.to_owned())
                .chain(arguments.iter().cloned())
                .collect(),
            filesystem_results: FsOperationResults::default(),
            ffi_bytes_memory: None,
        }
    }

    fn ffi_bytes_memory(&self) -> wasmtime::Result<Memory> {
        self.ffi_bytes_memory
            .ok_or_else(|| wasmtime::format_err!("ffi-bytes.memory was not imported"))
    }
}

#[derive(Debug)]
struct JsBytes(Mutex<Vec<u8>>);

#[derive(Clone, Debug)]
struct JsStringArray(Vec<Vec<u16>>);

#[derive(Debug, Default)]
struct JsStringBuilder(Mutex<Vec<u16>>);

#[derive(Debug)]
struct JsStringReader {
    units: Arc<[u16]>,
    index: AtomicUsize,
}

#[derive(Debug, Default)]
struct JsByteBuilder(Mutex<Vec<u8>>);

#[derive(Debug)]
struct JsByteReader {
    bytes: Vec<u8>,
    index: AtomicUsize,
}

#[derive(Debug)]
struct JsStringArrayReader {
    strings: Vec<Vec<u16>>,
    index: AtomicUsize,
}

#[derive(Debug)]
struct JsInstant(Instant);

#[derive(Debug)]
struct JsRng(Mutex<StdRng>);

pub(super) fn define_import(
    linker: &mut Linker<StoreData>,
    namespace: &str,
    name: &str,
) -> wasmtime::Result<bool> {
    match namespace {
        FS_MODULE => define_filesystem_import(linker, name),
        IO_MODULE => define_io_import(linker, namespace, name),
        "spectest" if name == "read_char" => define_io_import(linker, namespace, name),
        RAND_MODULE => define_random_import(linker, name),
        TIME_MODULE => define_time_import(linker, name),
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

fn define_filesystem_import(linker: &mut Linker<StoreData>, name: &str) -> wasmtime::Result<bool> {
    match name {
        "begin_create_string" => {
            linker.func_wrap(FS_MODULE, name, |mut caller: Caller<'_, StoreData>| {
                Ok(Some(ExternRef::new(
                    &mut caller,
                    JsStringBuilder::default(),
                )?))
            })?;
        }
        "string_append_char" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |caller: Caller<'_, StoreData>, builder: Option<Rooted<ExternRef>>, value: i32| {
                    with_extern_data::<JsStringBuilder, _>(&caller, builder.as_ref(), |builder| {
                        builder
                            .0
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .push(value as u16);
                    })?;
                    Ok(())
                },
            )?;
        }
        "finish_create_string" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, builder: Option<Rooted<ExternRef>>| {
                    let units = with_extern_data::<JsStringBuilder, _>(
                        &caller,
                        builder.as_ref(),
                        |builder| {
                            builder
                                .0
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .clone()
                        },
                    )?;
                    Ok(Some(ExternRef::new(&mut caller, JsString(units.into()))?))
                },
            )?;
        }
        "begin_read_string" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, string: Option<Rooted<ExternRef>>| {
                    let units =
                        with_extern_data::<JsString, _>(&caller, string.as_ref(), |string| {
                            Arc::clone(&string.0)
                        })?;
                    Ok(Some(ExternRef::new(
                        &mut caller,
                        JsStringReader {
                            units,
                            index: AtomicUsize::new(0),
                        },
                    )?))
                },
            )?;
        }
        "string_read_char" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |caller: Caller<'_, StoreData>, reader: Option<Rooted<ExternRef>>| {
                    with_extern_data::<JsStringReader, _>(&caller, reader.as_ref(), |reader| {
                        next(&reader.units, &reader.index).map_or(-1, i32::from)
                    })
                },
            )?;
        }
        "finish_read_string" | "finish_read_byte_array" | "finish_read_string_array" => {
            linker.func_wrap(FS_MODULE, name, |_value: Option<Rooted<ExternRef>>| {})?;
        }
        "begin_create_byte_array" => {
            linker.func_wrap(FS_MODULE, name, |mut caller: Caller<'_, StoreData>| {
                Ok(Some(ExternRef::new(&mut caller, JsByteBuilder::default())?))
            })?;
        }
        "byte_array_append_byte" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |caller: Caller<'_, StoreData>, builder: Option<Rooted<ExternRef>>, value: i32| {
                    with_extern_data::<JsByteBuilder, _>(&caller, builder.as_ref(), |builder| {
                        builder
                            .0
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .push(value as u8);
                    })?;
                    Ok(())
                },
            )?;
        }
        "finish_create_byte_array" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, builder: Option<Rooted<ExternRef>>| {
                    let bytes = with_extern_data::<JsByteBuilder, _>(
                        &caller,
                        builder.as_ref(),
                        |builder| {
                            builder
                                .0
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .clone()
                        },
                    )?;
                    Ok(Some(ExternRef::new(
                        &mut caller,
                        JsBytes(Mutex::new(bytes)),
                    )?))
                },
            )?;
        }
        "begin_read_byte_array" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, bytes: Option<Rooted<ExternRef>>| {
                    let bytes = extern_bytes(&caller, bytes.as_ref())?;
                    Ok(Some(ExternRef::new(
                        &mut caller,
                        JsByteReader {
                            bytes,
                            index: AtomicUsize::new(0),
                        },
                    )?))
                },
            )?;
        }
        "byte_array_read_byte" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |caller: Caller<'_, StoreData>, reader: Option<Rooted<ExternRef>>| {
                    with_extern_data::<JsByteReader, _>(&caller, reader.as_ref(), |reader| {
                        next(&reader.bytes, &reader.index).map_or(-1, i32::from)
                    })
                },
            )?;
        }
        "begin_read_string_array" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, strings: Option<Rooted<ExternRef>>| {
                    let strings =
                        with_extern_data::<JsStringArray, _>(&caller, strings.as_ref(), |array| {
                            array.0.clone()
                        })?;
                    Ok(Some(ExternRef::new(
                        &mut caller,
                        JsStringArrayReader {
                            strings,
                            index: AtomicUsize::new(0),
                        },
                    )?))
                },
            )?;
        }
        "string_array_read_string" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, reader: Option<Rooted<ExternRef>>| {
                    let units = with_extern_data::<JsStringArrayReader, _>(
                        &caller,
                        reader.as_ref(),
                        |reader| {
                            let index = reader.index.load(Ordering::Relaxed);
                            let value = reader.strings.get(index).cloned();
                            if value.is_some() {
                                reader.index.store(index + 1, Ordering::Relaxed);
                            }
                            value
                        },
                    )?
                    .unwrap_or_else(|| "ffi_end_of_/string_array".encode_utf16().collect());
                    Ok(Some(ExternRef::new(&mut caller, JsString(units.into()))?))
                },
            )?;
        }
        "array_len" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |caller: Caller<'_, StoreData>, array: Option<Rooted<ExternRef>>| {
                    Ok(with_extern_data::<JsStringArray, _>(
                        &caller,
                        array.as_ref(),
                        |array| i32::try_from(array.0.len()),
                    )??)
                },
            )?;
        }
        "array_get" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |mut caller: Caller<'_, StoreData>,
                 array: Option<Rooted<ExternRef>>,
                 index: i32| {
                    let index = usize::try_from(index)?;
                    let units =
                        with_extern_data::<JsStringArray, _>(&caller, array.as_ref(), |array| {
                            array.0.get(index).cloned()
                        })?
                        .ok_or_else(|| wasmtime::format_err!("array index out of bounds"))?;
                    Ok(Some(ExternRef::new(&mut caller, JsString(units.into()))?))
                },
            )?;
        }
        "jsvalue_is_string" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |caller: Caller<'_, StoreData>, value: Option<Rooted<ExternRef>>| {
                    Ok(i32::from(is_extern_data::<JsString>(
                        &caller,
                        value.as_ref(),
                    )?))
                },
            )?;
        }
        "args_get" => {
            linker.func_wrap(FS_MODULE, name, |mut caller: Caller<'_, StoreData>| {
                let values = caller
                    .data()
                    .host_imports
                    .arguments
                    .iter()
                    .map(|value| value.encode_utf16().collect())
                    .collect();
                Ok(Some(ExternRef::new(&mut caller, JsStringArray(values))?))
            })?;
        }
        "env_get_var" | "get_env_var" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, key: Option<Rooted<ExternRef>>| {
                    let key = extern_string(&caller, key.as_ref())?;
                    let value = caller
                        .data()
                        .runtime()
                        .environment()
                        .get(key.as_ref())
                        .and_then(|value| value.into_string().ok())
                        .unwrap_or_default();
                    Ok(Some(new_string_value(&mut caller, &value)?))
                },
            )?;
        }
        "get_env_var_exists" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |caller: Caller<'_, StoreData>, key: Option<Rooted<ExternRef>>| {
                    let key = extern_string(&caller, key.as_ref())?;
                    Ok(i32::from(
                        caller
                            .data()
                            .runtime()
                            .environment()
                            .get(key.as_ref())
                            .is_some(),
                    ))
                },
            )?;
        }
        "set_env_var" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |caller: Caller<'_, StoreData>,
                 key: Option<Rooted<ExternRef>>,
                 value: Option<Rooted<ExternRef>>| {
                    let key = extern_string(&caller, key.as_ref())?;
                    let value = extern_string(&caller, value.as_ref())?;
                    let _ = caller
                        .data()
                        .runtime()
                        .environment()
                        .set(key.into(), value.into());
                    Ok(())
                },
            )?;
        }
        "unset_env_var" => {
            linker.func_wrap(
                FS_MODULE,
                name,
                |caller: Caller<'_, StoreData>, key: Option<Rooted<ExternRef>>| {
                    let key = extern_string(&caller, key.as_ref())?;
                    let _ = caller.data().runtime().environment().unset(key.as_ref());
                    Ok(())
                },
            )?;
        }
        "get_env_vars" => {
            linker.func_wrap(FS_MODULE, name, |mut caller: Caller<'_, StoreData>| {
                let values = caller
                    .data()
                    .runtime()
                    .environment()
                    .entries()
                    .into_iter()
                    .filter_map(|(name, value)| {
                        Some((name.into_string().ok()?, value.into_string().ok()?))
                    })
                    .flat_map(|(name, value)| [name, value])
                    .map(|value| value.encode_utf16().collect())
                    .collect();
                Ok(Some(ExternRef::new(&mut caller, JsStringArray(values))?))
            })?;
        }
        "read_file_to_string" => define_read_file_to_string(linker, name)?,
        "write_string_to_file" => define_write_string_to_file(linker, name)?,
        "write_bytes_to_file" => define_write_bytes_to_file(linker, name, false)?,
        "create_dir" | "remove_file" | "remove_dir" => define_legacy_path_operation(linker, name)?,
        "read_dir" => define_read_dir(linker, name)?,
        "is_file" | "is_dir" | "path_exists" => define_path_predicate(linker, name)?,
        "current_dir" => {
            linker.func_wrap(FS_MODULE, name, |mut caller: Caller<'_, StoreData>| {
                let directory = caller.data().runtime().filesystem().current_dir();
                Ok(Some(new_string_value(&mut caller, &directory)?))
            })?;
        }
        "read_file_to_bytes_new"
        | "create_dir_new"
        | "read_dir_new"
        | "is_file_new"
        | "is_dir_new"
        | "remove_file_new"
        | "remove_dir_new" => define_status_path_operation(linker, name)?,
        "write_bytes_to_file_new" => define_write_bytes_to_file(linker, name, true)?,
        "get_file_content" => {
            linker.func_wrap(FS_MODULE, name, |mut caller: Caller<'_, StoreData>| {
                let contents = caller
                    .data()
                    .host_imports
                    .filesystem_results
                    .file_content()
                    .to_vec();
                Ok(Some(ExternRef::new(
                    &mut caller,
                    JsBytes(Mutex::new(contents)),
                )?))
            })?;
        }
        "get_dir_files" => {
            linker.func_wrap(FS_MODULE, name, |mut caller: Caller<'_, StoreData>| {
                let files = caller
                    .data()
                    .host_imports
                    .filesystem_results
                    .dir_files()
                    .iter()
                    .map(|value| value.encode_utf16().collect())
                    .collect();
                Ok(Some(ExternRef::new(&mut caller, JsStringArray(files))?))
            })?;
        }
        "get_error_message" => {
            linker.func_wrap(FS_MODULE, name, |mut caller: Caller<'_, StoreData>| {
                let message = caller
                    .data()
                    .host_imports
                    .filesystem_results
                    .error_message()
                    .to_owned();
                Ok(Some(new_string_value(&mut caller, &message)?))
            })?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn define_io_import(
    linker: &mut Linker<StoreData>,
    namespace: &str,
    name: &str,
) -> wasmtime::Result<bool> {
    match name {
        "read_bytes_from_stdin" => {
            linker.func_wrap(namespace, name, |mut caller: Caller<'_, StoreData>| {
                let mut bytes = Vec::new();
                caller
                    .data()
                    .runtime()
                    .stdio()
                    .with_stdin(|stdin| stdin.read_to_end(&mut bytes))?;
                Ok(Some(ExternRef::new(
                    &mut caller,
                    JsBytes(Mutex::new(bytes)),
                )?))
            })?;
        }
        "read_char" => {
            linker.func_wrap(namespace, name, |caller: Caller<'_, StoreData>| {
                caller
                    .data()
                    .runtime()
                    .stdio()
                    .read_utf8_char()
                    .ok()
                    .flatten()
                    .map_or(-1, |value| value as i32)
            })?;
        }
        "write_char" => {
            linker.func_wrap(
                namespace,
                name,
                |caller: Caller<'_, StoreData>, fd: i32, value: i32| {
                    let value = char::from_u32(value as u32)
                        .ok_or_else(|| wasmtime::format_err!("invalid character"))?;
                    let stdio = caller.data().runtime().stdio();
                    match fd {
                        1 => stdio.with_stdout(|stdout| write!(stdout, "{value}"))?,
                        2 => stdio.with_stderr(|stderr| write!(stderr, "{value}"))?,
                        _ => {}
                    }
                    Ok(())
                },
            )?;
        }
        "flush" => {
            linker.func_wrap(namespace, name, |caller: Caller<'_, StoreData>, fd: i32| {
                let stdio = caller.data().runtime().stdio();
                match fd {
                    1 => stdio.with_stdout(|stdout| stdout.flush())?,
                    2 => stdio.with_stderr(|stderr| stderr.flush())?,
                    _ => {}
                }
                Ok(())
            })?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn define_random_import(linker: &mut Linker<StoreData>, name: &str) -> wasmtime::Result<bool> {
    match name {
        "stdrng_seed_from_u64" => {
            linker.func_wrap(
                RAND_MODULE,
                name,
                |mut caller: Caller<'_, StoreData>, seed: i32| {
                    let rng = StdRng::seed_from_u64(seed as u64);
                    Ok(Some(ExternRef::new(&mut caller, JsRng(Mutex::new(rng)))?))
                },
            )?;
        }
        "stdrng_gen_range" => {
            linker.func_wrap(
                RAND_MODULE,
                name,
                |caller: Caller<'_, StoreData>, rng: Option<Rooted<ExternRef>>, upper: i32| {
                    if upper <= 0 {
                        wasmtime::bail!("random range upper bound must be positive");
                    }
                    with_extern_data::<JsRng, _>(&caller, rng.as_ref(), |rng| {
                        rng.0
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .gen_range(0..upper)
                    })
                },
            )?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn define_time_import(linker: &mut Linker<StoreData>, name: &str) -> wasmtime::Result<bool> {
    match name {
        "instant_now" => {
            linker.func_wrap(TIME_MODULE, name, |mut caller: Caller<'_, StoreData>| {
                Ok(Some(ExternRef::new(
                    &mut caller,
                    JsInstant(Instant::now()),
                )?))
            })?;
        }
        "instant_elapsed_as_secs_f64" => {
            linker.func_wrap(
                TIME_MODULE,
                name,
                |caller: Caller<'_, StoreData>, instant: Option<Rooted<ExternRef>>| {
                    with_extern_data::<JsInstant, _>(&caller, instant.as_ref(), |instant| {
                        instant.0.elapsed().as_secs_f64()
                    })
                },
            )?;
        }
        "now" => {
            linker.func_wrap(TIME_MODULE, name, || {
                let millis = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| wasmtime::format_err!("system time is before the Unix epoch"))?
                    .as_millis();
                Ok(u64::try_from(millis)? as i64)
            })?;
        }
        _ => return Ok(false),
    }
    Ok(true)
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

fn define_read_file_to_string(linker: &mut Linker<StoreData>, name: &str) -> wasmtime::Result<()> {
    linker.func_wrap(
        FS_MODULE,
        name,
        |mut caller: Caller<'_, StoreData>, path: Option<Rooted<ExternRef>>| {
            let path = extern_string(&caller, path.as_ref())?;
            let contents = caller
                .data()
                .runtime()
                .filesystem()
                .read_file_to_string(&path)
                .map_err(|error| wasmtime::format_err!(error.to_string()))?;
            Ok(Some(new_string_value(&mut caller, &contents)?))
        },
    )?;
    Ok(())
}

fn define_write_string_to_file(linker: &mut Linker<StoreData>, name: &str) -> wasmtime::Result<()> {
    linker.func_wrap(
        FS_MODULE,
        name,
        |caller: Caller<'_, StoreData>,
         path: Option<Rooted<ExternRef>>,
         contents: Option<Rooted<ExternRef>>| {
            let path = extern_string(&caller, path.as_ref())?;
            let contents = extern_string(&caller, contents.as_ref())?;
            caller
                .data()
                .runtime()
                .filesystem()
                .write_string_to_file(&path, &contents)
                .map_err(|error| wasmtime::format_err!(error.to_string()))
        },
    )?;
    Ok(())
}

fn define_write_bytes_to_file(
    linker: &mut Linker<StoreData>,
    name: &str,
    status_result: bool,
) -> wasmtime::Result<()> {
    if status_result {
        linker.func_wrap(
            FS_MODULE,
            name,
            |mut caller: Caller<'_, StoreData>,
             path: Option<Rooted<ExternRef>>,
             contents: Option<Rooted<ExternRef>>|
             -> wasmtime::Result<i32> {
                let path = extern_string(&caller, path.as_ref())?;
                let filesystem = caller.data().runtime().filesystem().clone();
                let mut operation_results =
                    std::mem::take(&mut caller.data_mut().host_imports.filesystem_results);
                let status =
                    filesystem.write_bytes_to_file_new(&mut operation_results, &path, || {
                        extern_bytes(&caller, contents.as_ref()).map_err(|error| error.to_string())
                    });
                caller.data_mut().host_imports.filesystem_results = operation_results;
                Ok(status)
            },
        )?;
    } else {
        linker.func_wrap(
            FS_MODULE,
            name,
            |caller: Caller<'_, StoreData>,
             path: Option<Rooted<ExternRef>>,
             contents: Option<Rooted<ExternRef>>| {
                let path = extern_string(&caller, path.as_ref())?;
                let filesystem = caller.data().runtime().filesystem().clone();
                filesystem.write_bytes_to_file(&path, || extern_bytes(&caller, contents.as_ref()))
            },
        )?;
    }
    Ok(())
}

fn define_legacy_path_operation(
    linker: &mut Linker<StoreData>,
    name: &str,
) -> wasmtime::Result<()> {
    let operation = name.to_owned();
    linker.func_wrap(
        FS_MODULE,
        name,
        move |caller: Caller<'_, StoreData>, path: Option<Rooted<ExternRef>>| {
            let path = extern_string(&caller, path.as_ref())?;
            let filesystem = caller.data().runtime().filesystem();
            let result = match operation.as_str() {
                "create_dir" => filesystem.create_dir(&path),
                "remove_file" => filesystem.remove_file(&path),
                "remove_dir" => filesystem.remove_dir(&path),
                _ => unreachable!(),
            };
            result.map_err(|error| wasmtime::format_err!(error.to_string()))
        },
    )?;
    Ok(())
}

fn define_read_dir(linker: &mut Linker<StoreData>, name: &str) -> wasmtime::Result<()> {
    linker.func_wrap(
        FS_MODULE,
        name,
        |mut caller: Caller<'_, StoreData>, path: Option<Rooted<ExternRef>>| {
            let path = extern_string(&caller, path.as_ref())?;
            let entries = caller
                .data()
                .runtime()
                .filesystem()
                .read_dir(&path)
                .map_err(|error| wasmtime::format_err!(error.to_string()))?
                .into_iter()
                .map(|value| value.encode_utf16().collect())
                .collect();
            Ok(Some(ExternRef::new(&mut caller, JsStringArray(entries))?))
        },
    )?;
    Ok(())
}

fn define_path_predicate(linker: &mut Linker<StoreData>, name: &str) -> wasmtime::Result<()> {
    let operation = name.to_owned();
    linker.func_wrap(
        FS_MODULE,
        name,
        move |caller: Caller<'_, StoreData>, path: Option<Rooted<ExternRef>>| {
            let path = extern_string(&caller, path.as_ref())?;
            let filesystem = caller.data().runtime().filesystem();
            let value = match operation.as_str() {
                "is_file" => filesystem.is_file(&path),
                "is_dir" => filesystem.is_dir(&path),
                "path_exists" => filesystem.path_exists(&path),
                _ => unreachable!(),
            };
            Ok(i32::from(value))
        },
    )?;
    Ok(())
}

fn define_status_path_operation(
    linker: &mut Linker<StoreData>,
    name: &str,
) -> wasmtime::Result<()> {
    let operation = name.to_owned();
    linker.func_wrap(
        FS_MODULE,
        name,
        move |mut caller: Caller<'_, StoreData>, path: Option<Rooted<ExternRef>>| {
            let path = extern_string(&caller, path.as_ref())?;
            let filesystem = caller.data().runtime().filesystem().clone();
            let state = &mut caller.data_mut().host_imports.filesystem_results;
            let status = match operation.as_str() {
                "read_file_to_bytes_new" => filesystem.read_file_to_bytes_new(state, &path),
                "create_dir_new" => filesystem.create_dir_new(state, &path),
                "read_dir_new" => filesystem.read_dir_new(state, &path),
                "is_file_new" => filesystem.is_file_new(state, &path),
                "is_dir_new" => filesystem.is_dir_new(state, &path),
                "remove_file_new" => filesystem.remove_file_new(state, &path),
                "remove_dir_new" => filesystem.remove_dir_new(state, &path),
                _ => unreachable!(),
            };
            Ok(status)
        },
    )?;
    Ok(())
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
    let value = data
        .downcast_ref::<T>()
        .ok_or_else(|| wasmtime::format_err!("unexpected externref host value"))?;
    Ok(f(value))
}

fn is_extern_data<T: 'static>(
    caller: &Caller<'_, StoreData>,
    reference: Option<&Rooted<ExternRef>>,
) -> wasmtime::Result<bool> {
    let Some(reference) = reference else {
        return Ok(false);
    };
    Ok(reference
        .data(caller.as_context())?
        .is_some_and(|value| value.is::<T>()))
}

fn extern_string(
    caller: &Caller<'_, StoreData>,
    value: Option<&Rooted<ExternRef>>,
) -> wasmtime::Result<String> {
    Ok(String::from_utf16_lossy(&with_extern_data::<JsString, _>(
        caller,
        value,
        |string| Arc::clone(&string.0),
    )?))
}

fn new_string_value(
    caller: &mut Caller<'_, StoreData>,
    value: &str,
) -> wasmtime::Result<Rooted<ExternRef>> {
    ExternRef::new(
        caller,
        JsString(value.encode_utf16().collect::<Vec<_>>().into()),
    )
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

fn next<T: Copy>(values: &[T], index: &AtomicUsize) -> Option<T> {
    let current = index.load(Ordering::Relaxed);
    let value = values.get(current).copied();
    if value.is_some() {
        index.store(current + 1, Ordering::Relaxed);
    }
    value
}
