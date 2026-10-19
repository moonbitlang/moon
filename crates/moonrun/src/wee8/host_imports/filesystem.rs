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

//! Convert legacy filesystem values and delegate policy, I/O, and status results
//! to the same Host Filesystem used by V8 and Wasmtime.

use super::{
    Bytes, Kind, StoreData, StringValue, Strings, Value, api, bind, host_error, owned, reference,
    string,
};

pub(super) fn define<'s>(
    store: &'s api::Store<StoreData>,
    import: &api::Import,
) -> api::Result<Option<api::HostFunc<'s, StoreData>>> {
    let (params, results): (&[Kind], &[Kind]) = match import.name.as_str() {
        "read_file_to_string" | "read_dir" => (&[Kind::ExternRef], &[Kind::ExternRef]),
        "current_dir" | "get_file_content" | "get_dir_files" | "get_error_message" => {
            (&[], &[Kind::ExternRef])
        }
        "write_string_to_file" | "write_bytes_to_file" => {
            (&[Kind::ExternRef, Kind::ExternRef], &[])
        }
        "create_dir" | "remove_file" | "remove_dir" => (&[Kind::ExternRef], &[]),
        "write_bytes_to_file_new" => (&[Kind::ExternRef, Kind::ExternRef], &[Kind::I32]),
        "is_file"
        | "is_dir"
        | "path_exists"
        | "read_file_to_bytes_new"
        | "create_dir_new"
        | "read_dir_new"
        | "is_file_new"
        | "is_dir_new"
        | "remove_file_new"
        | "remove_dir_new" => (&[Kind::ExternRef], &[Kind::I32]),
        _ => return Ok(None),
    };
    let name = import.name.clone();
    bind(store, import, params, results, move |caller, args| {
        caller.with_data(|state| {
            let filesystem = state.runtime.filesystem();
            let result = match name.as_str() {
                "read_file_to_string" => {
                    let contents = filesystem
                        .read_file_to_string(&string(&args[0])?)
                        .map_err(host_error)?;
                    owned(&caller, StringValue(contents.encode_utf16().collect()))?
                }
                "write_string_to_file" => {
                    filesystem
                        .write_string_to_file(&string(&args[0])?, &string(&args[1])?)
                        .map_err(host_error)?;
                    return Ok(Vec::new());
                }
                "write_bytes_to_file" => {
                    // Keep byte conversion lazy: authorization must precede a
                    // bad guest payload, matching the other adapters.
                    filesystem
                        .write_bytes_to_file::<anyhow::Error>(&string(&args[0])?, || {
                            Ok(reference::<Bytes>(&args[1])?.0.clone())
                        })
                        .map_err(host_error)?;
                    return Ok(Vec::new());
                }
                "create_dir" | "remove_file" | "remove_dir" => {
                    let path = string(&args[0])?;
                    match name.as_str() {
                        "create_dir" => filesystem.create_dir(&path),
                        "remove_file" => filesystem.remove_file(&path),
                        "remove_dir" => filesystem.remove_dir(&path),
                        _ => unreachable!("checked import"),
                    }
                    .map_err(host_error)?;
                    return Ok(Vec::new());
                }
                "read_dir" => {
                    let entries = filesystem
                        .read_dir(&string(&args[0])?)
                        .map_err(host_error)?;
                    owned(
                        &caller,
                        Strings(
                            entries
                                .into_iter()
                                .map(|entry| entry.encode_utf16().collect())
                                .collect(),
                        ),
                    )?
                }
                "is_file" | "is_dir" | "path_exists" => {
                    let path = string(&args[0])?;
                    Value::I32(i32::from(match name.as_str() {
                        "is_file" => filesystem.is_file(&path),
                        "is_dir" => filesystem.is_dir(&path),
                        "path_exists" => filesystem.path_exists(&path),
                        _ => unreachable!("checked import"),
                    }))
                }
                "current_dir" => owned(
                    &caller,
                    StringValue(filesystem.current_dir().encode_utf16().collect()),
                )?,
                "get_file_content" => {
                    let contents = state
                        .host_imports
                        .filesystem_results
                        .borrow()
                        .file_content()
                        .to_vec();
                    owned(&caller, Bytes(contents))?
                }
                "get_dir_files" => {
                    let entries = state
                        .host_imports
                        .filesystem_results
                        .borrow()
                        .dir_files()
                        .iter()
                        .map(|entry| entry.encode_utf16().collect())
                        .collect();
                    owned(&caller, Strings(entries))?
                }
                "get_error_message" => {
                    let error = state
                        .host_imports
                        .filesystem_results
                        .borrow()
                        .error_message()
                        .encode_utf16()
                        .collect();
                    owned(&caller, StringValue(error))?
                }
                "write_bytes_to_file_new" => {
                    let path = string(&args[0])?;
                    let mut results = state.host_imports.filesystem_results.borrow_mut();
                    Value::I32(filesystem.write_bytes_to_file_new(&mut results, &path, || {
                        reference::<Bytes>(&args[1])
                            .map(|bytes| bytes.0.clone())
                            .map_err(|error| error.to_string())
                    }))
                }
                _ => {
                    let path = string(&args[0])?;
                    // This borrow covers only a synchronous Host Domain operation;
                    // it never crosses native allocation or guest re-entry.
                    let mut results = state.host_imports.filesystem_results.borrow_mut();
                    Value::I32(match name.as_str() {
                        "read_file_to_bytes_new" => {
                            filesystem.read_file_to_bytes_new(&mut results, &path)
                        }
                        "create_dir_new" => filesystem.create_dir_new(&mut results, &path),
                        "read_dir_new" => filesystem.read_dir_new(&mut results, &path),
                        "is_file_new" => filesystem.is_file_new(&mut results, &path),
                        "is_dir_new" => filesystem.is_dir_new(&mut results, &path),
                        "remove_file_new" => filesystem.remove_file_new(&mut results, &path),
                        "remove_dir_new" => filesystem.remove_dir_new(&mut results, &path),
                        _ => unreachable!("checked import"),
                    })
                }
            };
            Ok(vec![result])
        })
    })
}
