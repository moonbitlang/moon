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

//! The legacy core string/environment ABI used by error formatting.
//!
//! Match Wasmtime's value semantics: builders snapshot on finish, readers own
//! their cursor, and strings preserve UTF-16 units until an OS key is needed.
//! Native externrefs root these typed payloads; no separate handle table exists.

use std::cell::{Cell, RefCell};

use super::{StoreData, api};
use api::{Kind, Value};

struct StringBuilder(RefCell<Vec<u16>>);
struct StringValue(Vec<u16>);
struct StringReader {
    units: Vec<u16>,
    next: Cell<usize>,
}

pub(super) fn define<'s>(
    store: &'s api::Store<StoreData>,
    import: &api::Import,
) -> api::Result<Option<api::HostFunc<'s, StoreData>>> {
    if import.module != "__moonbit_fs_unstable" {
        return Ok(None);
    }
    let (params, results): (&[Kind], &[Kind]) = match import.name.as_str() {
        "begin_create_string" => (&[], &[Kind::ExternRef]),
        "string_append_char" => (&[Kind::ExternRef, Kind::I32], &[]),
        "finish_create_string" | "begin_read_string" | "get_env_var" => {
            (&[Kind::ExternRef], &[Kind::ExternRef])
        }
        "string_read_char" | "get_env_var_exists" => (&[Kind::ExternRef], &[Kind::I32]),
        "finish_read_string" => (&[Kind::ExternRef], &[]),
        _ => return Ok(None),
    };
    if import.params != params || import.results != results {
        return Err(api::Error::Interface(format!(
            "incorrect signature for {}::{}",
            import.module, import.name
        )));
    }
    let name = import.name.clone();
    store
        .host_func(params, results, move |caller, args| {
            let result = match name.as_str() {
                "begin_create_string" => Value::ExternRef(Some(api::ExternRef::new(
                    caller.as_context(),
                    StringBuilder(RefCell::new(Vec::new())),
                )?)),
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
                    Value::ExternRef(Some(api::ExternRef::new(
                        caller.as_context(),
                        StringValue(units),
                    )?))
                }
                "begin_read_string" => {
                    let units = reference::<StringValue>(&args[0])?.0.clone();
                    Value::ExternRef(Some(api::ExternRef::new(
                        caller.as_context(),
                        StringReader {
                            units,
                            next: Cell::new(0),
                        },
                    )?))
                }
                "string_read_char" => {
                    let reader = reference::<StringReader>(&args[0])?;
                    let index = reader.next.get();
                    Value::I32(match reader.units.get(index) {
                        Some(unit) => {
                            reader.next.set(index + 1);
                            i32::from(*unit)
                        }
                        None => -1,
                    })
                }
                // The guest releases its reference; V8 owns reclamation, as in the
                // other adapters. This operation does not invalidate aliases.
                "finish_read_string" => return Ok(Vec::new()),
                "get_env_var" => {
                    let key = String::from_utf16_lossy(&reference::<StringValue>(&args[0])?.0);
                    let value = caller.with_data(|state| {
                        state
                            .runtime
                            .environment()
                            .get(key.as_ref())
                            .and_then(|value| value.into_string().ok())
                            .unwrap_or_default()
                    });
                    Value::ExternRef(Some(api::ExternRef::new(
                        caller.as_context(),
                        StringValue(value.encode_utf16().collect()),
                    )?))
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
        .map(Some)
}

fn reference<'a, T: 'static>(value: &'a Value<'_>) -> api::Result<&'a T> {
    match value {
        Value::ExternRef(Some(reference)) => reference.data::<T>(),
        _ => None,
    }
    .ok_or_else(|| api::Error::Host("invalid core string reference".into()))
}
