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

//! Adapter for the legacy reference-valued Moonrun imports.
//!
//! Match Wasmtime's value semantics: builders snapshot on finish, readers own
//! their cursor, and strings preserve UTF-16 units until an OS key is needed.
//! Native externrefs root these typed payloads; no separate handle table exists.

mod filesystem;
mod services;

use std::cell::{Cell, RefCell};

use super::{StoreData, api};
use api::{Kind, Value};

struct StringBuilder(RefCell<Vec<u16>>);
struct StringValue(Vec<u16>);
struct ByteBuilder(RefCell<Vec<u8>>);
struct Bytes(Vec<u8>);
struct Strings(Vec<Vec<u16>>);
struct Reader<T> {
    values: Vec<T>,
    next: Cell<usize>,
}
impl<T: Clone> Reader<T> {
    fn new(values: Vec<T>) -> Self {
        Self {
            values,
            next: Cell::new(0),
        }
    }
    fn read(&self) -> Option<T> {
        let index = self.next.get();
        let value = self.values.get(index)?.clone();
        self.next.set(index + 1);
        Some(value)
    }
}

pub(super) struct State {
    arguments: Vec<Vec<u16>>,
    filesystem_results: RefCell<crate::filesystem::FsOperationResults>,
}
impl State {
    pub(super) fn new(module_name: &str, args: &[String]) -> Self {
        Self {
            filesystem_results: RefCell::default(),
            arguments: std::iter::once(module_name)
                .chain(args.iter().map(String::as_str))
                .map(|value| value.encode_utf16().collect())
                .collect(),
        }
    }
}

pub(super) fn define<'s>(
    store: &'s api::Store<StoreData>,
    import: &api::Import,
) -> api::Result<Option<api::HostFunc<'s, StoreData>>> {
    if import.module != "__moonbit_fs_unstable" {
        return services::define(store, import);
    }
    let (params, results): (&[Kind], &[Kind]) = match import.name.as_str() {
        "begin_create_string" | "begin_create_byte_array" | "args_get" | "get_env_vars" => {
            (&[], &[Kind::ExternRef])
        }
        "string_append_char" | "byte_array_append_byte" => (&[Kind::ExternRef, Kind::I32], &[]),
        "finish_create_string"
        | "begin_read_string"
        | "get_env_var"
        | "finish_create_byte_array"
        | "begin_read_byte_array"
        | "begin_read_string_array"
        | "string_array_read_string"
        | "env_get_var" => (&[Kind::ExternRef], &[Kind::ExternRef]),
        "string_read_char"
        | "get_env_var_exists"
        | "byte_array_read_byte"
        | "array_len"
        | "jsvalue_is_string" => (&[Kind::ExternRef], &[Kind::I32]),
        "finish_read_string"
        | "finish_read_byte_array"
        | "finish_read_string_array"
        | "unset_env_var" => (&[Kind::ExternRef], &[]),
        "set_env_var" => (&[Kind::ExternRef, Kind::ExternRef], &[]),
        "array_get" => (&[Kind::ExternRef, Kind::I32], &[Kind::ExternRef]),
        _ => return filesystem::define(store, import),
    };
    let name = import.name.clone();
    bind(store, import, params, results, move |caller, args| {
        let result = match name.as_str() {
            "begin_create_string" => owned(&caller, StringBuilder(RefCell::new(Vec::new())))?,
            "string_append_char" => {
                let Value::I32(unit) = args[1] else {
                    unreachable!("checked signature")
                };
                reference::<StringBuilder>(&args[0])?
                    .0
                    .borrow_mut()
                    .push(unit as u16);
                return Ok(Vec::new());
            }
            "finish_create_string" => {
                let units = reference::<StringBuilder>(&args[0])?.0.borrow().clone();
                owned(&caller, StringValue(units))?
            }
            "begin_read_string" => {
                let units = reference::<StringValue>(&args[0])?.0.clone();
                owned(&caller, Reader::new(units))?
            }
            "string_read_char" => Value::I32(
                reference::<Reader<u16>>(&args[0])?
                    .read()
                    .map_or(-1, i32::from),
            ),
            // The guest releases its reference; V8 owns reclamation, as in the
            // other adapters. This operation does not invalidate aliases.
            "finish_read_string" | "finish_read_byte_array" | "finish_read_string_array" => {
                return Ok(Vec::new());
            }
            "begin_create_byte_array" => owned(&caller, ByteBuilder(RefCell::new(Vec::new())))?,
            "byte_array_append_byte" => {
                let Value::I32(byte) = args[1] else {
                    unreachable!("checked signature")
                };
                reference::<ByteBuilder>(&args[0])?
                    .0
                    .borrow_mut()
                    .push(byte as u8);
                return Ok(Vec::new());
            }
            "finish_create_byte_array" => {
                let bytes = reference::<ByteBuilder>(&args[0])?.0.borrow().clone();
                owned(&caller, Bytes(bytes))?
            }
            "begin_read_byte_array" => {
                let bytes = reference::<Bytes>(&args[0])?.0.clone();
                owned(&caller, Reader::new(bytes))?
            }
            "byte_array_read_byte" => Value::I32(
                reference::<Reader<u8>>(&args[0])?
                    .read()
                    .map_or(-1, i32::from),
            ),
            "args_get" => {
                let args = caller.with_data(|state| state.host_imports.arguments.clone());
                owned(&caller, Strings(args))?
            }
            "array_len" => Value::I32(
                i32::try_from(reference::<Strings>(&args[0])?.0.len()).map_err(host_error)?,
            ),
            "array_get" => {
                let Value::I32(index) = args[1] else {
                    unreachable!("checked signature")
                };
                let value = reference::<Strings>(&args[0])?
                    .0
                    .get(index as u32 as usize)
                    .ok_or_else(|| host_error("array index out of bounds"))?
                    .clone();
                owned(&caller, StringValue(value))?
            }
            "jsvalue_is_string" => Value::I32(i32::from(matches!(&args[0],
                    Value::ExternRef(Some(value)) if value.data::<StringValue>().is_some()))),
            "begin_read_string_array" => owned(
                &caller,
                Reader::new(reference::<Strings>(&args[0])?.0.clone()),
            )?,
            "string_array_read_string" => {
                let value = reference::<Reader<Vec<u16>>>(&args[0])?
                    .read()
                    .unwrap_or_else(|| "ffi_end_of_/string_array".encode_utf16().collect());
                owned(&caller, StringValue(value))?
            }
            "set_env_var" => {
                let key = string(&args[0])?;
                let value = string(&args[1])?;
                caller.with_data(|state| {
                    let _ = state.runtime.environment().set(key.into(), value.into());
                });
                return Ok(Vec::new());
            }
            "unset_env_var" => {
                let key = string(&args[0])?;
                caller.with_data(|state| {
                    let _ = state.runtime.environment().unset(key.as_ref());
                });
                return Ok(Vec::new());
            }
            "get_env_vars" => {
                let values = caller
                    .with_data(|state| state.runtime.environment().entries())
                    .into_iter()
                    .filter_map(|(key, value)| {
                        Some((key.into_string().ok()?, value.into_string().ok()?))
                    })
                    .flat_map(|(key, value)| [key, value])
                    .map(|s| s.encode_utf16().collect())
                    .collect();
                owned(&caller, Strings(values))?
            }
            "get_env_var" | "env_get_var" => {
                let key = String::from_utf16_lossy(&reference::<StringValue>(&args[0])?.0);
                let value = caller.with_data(|state| {
                    state
                        .runtime
                        .environment()
                        .get(key.as_ref())
                        .and_then(|value| value.into_string().ok())
                        .unwrap_or_default()
                });
                owned(&caller, StringValue(value.encode_utf16().collect()))?
            }
            "get_env_var_exists" => {
                let key = String::from_utf16_lossy(&reference::<StringValue>(&args[0])?.0);
                Value::I32(caller.with_data(|state| {
                    i32::from(state.runtime.environment().get(key.as_ref()).is_some())
                }))
            }
            _ => unreachable!("recognized core import"),
        };
        Ok(vec![result])
    })
}

fn reference<'a, T: 'static>(value: &'a Value<'_>) -> api::Result<&'a T> {
    match value {
        Value::ExternRef(Some(reference)) => reference.data::<T>(),
        _ => None,
    }
    .ok_or_else(|| api::Error::Host("invalid host reference".into()))
}

fn owned<'a>(
    caller: &api::Caller<'a, StoreData>,
    value: impl std::any::Any,
) -> api::Result<Value<'a>> {
    Ok(Value::ExternRef(Some(api::ExternRef::new(
        caller.as_context(),
        value,
    )?)))
}

fn string(value: &Value<'_>) -> api::Result<String> {
    Ok(String::from_utf16_lossy(
        &reference::<StringValue>(value)?.0,
    ))
}

fn host_error(error: impl std::fmt::Display) -> api::Error {
    api::Error::Host(error.to_string())
}

// Validate the guest declaration once before any callback decodes its values.
// Reference payloads still have to be checked at each call: the guest can pass
// null or any other externref even when its Wasm signature is correct.
fn bind<'s>(
    store: &'s api::Store<StoreData>,
    import: &api::Import,
    params: &[Kind],
    results: &[Kind],
    callback: impl for<'a> Fn(api::Caller<'a, StoreData>, &[Value<'a>]) -> api::Result<Vec<Value<'a>>>
    + 'static,
) -> api::Result<Option<api::HostFunc<'s, StoreData>>> {
    if import.params != params || import.results != results {
        return Err(api::Error::Interface(format!(
            "incorrect signature for {}::{}",
            import.module, import.name
        )));
    }
    store.host_func(params, results, callback).map(Some)
}
