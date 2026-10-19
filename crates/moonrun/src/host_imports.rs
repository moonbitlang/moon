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

//! Shared definitions for MoonBit's legacy host ABI. Adapters retain only
//! value representation, rooting, memory access and error/termination delivery.
use crate::{
    filesystem::{FsOperationResults, HostFs},
    runtime::Runtime,
    wasm_imports::{Caller, ExternRef, ImportError, ImportRegistry},
};
use std::sync::{Arc, Mutex};
type Ref<'a> = Option<ExternRef<'a>>;
type Result<T> = std::result::Result<T, ImportError>;
const FS: &str = "__moonbit_fs_unstable";
// Rust needs the higher-ranked return lifetime at the closure's definition site.
// Named reference functions already express this relationship in their signature.
fn reference_result<F>(function: F) -> F
where
    F: for<'context, 'call> Fn(Caller<'context, 'call, Runtime>) -> Result<Ref<'call>>,
{
    function
}

fn reference_param<F, R>(function: F) -> F
where
    F: for<'context, 'call> Fn(Caller<'context, 'call, Runtime>, Ref<'call>) -> Result<R>,
{
    function
}
fn reference_pair<F, R>(function: F) -> F
where
    F: for<'context, 'call> Fn(
        Caller<'context, 'call, Runtime>,
        Ref<'call>,
        Ref<'call>,
    ) -> Result<R>,
{
    function
}

pub(crate) fn register<R: ImportRegistry<Runtime>>(
    registry: &mut R,
    module_name: &str,
    args: &[String],
) -> std::result::Result<(), R::Error> {
    register_values(registry)?;
    let arguments: Vec<String> = std::iter::once(module_name.to_owned())
        .chain(args.iter().cloned())
        .collect();
    registry.func_wrap(
        FS,
        "args_get",
        reference_result(move |mut caller: Caller<'_, '_, Runtime>| {
            caller.values().new_strings(arguments.clone())
        }),
    )?;
    registry.func_wrap(FS, "get_env_vars", get_env_vars)?;
    registry.func_wrap(FS, "get_env_var", get_env_var)?;
    registry.func_wrap(FS, "env_get_var", get_env_var)?;
    registry.func_wrap(FS, "get_env_var_exists", get_env_var_exists)?;
    registry.func_wrap(FS, "set_env_var", set_env_var)?;
    registry.func_wrap(FS, "unset_env_var", unset_env_var)?;
    registry.func_wrap(FS, "read_file_to_string", read_file_to_string)?;
    registry.func_wrap(FS, "write_string_to_file", write_string_to_file)?;
    registry.func_wrap(FS, "write_bytes_to_file", write_bytes_to_file)?;
    registry.func_wrap(FS, "read_dir", read_dir)?;
    registry.func_wrap(FS, "current_dir", current_dir)?;
    for (name, operation) in [
        ("create_dir", HostFs::create_dir as fn(&HostFs, &str) -> _),
        ("remove_file", HostFs::remove_file),
        ("remove_dir", HostFs::remove_dir),
    ] {
        registry.func_wrap(
            FS,
            name,
            reference_param(move |mut caller: Caller<'_, '_, Runtime>, path: Ref<'_>| {
                let path = caller.values().string(&path)?;
                caller
                    .with_data(|runtime| operation(runtime.filesystem(), &path))
                    .map_err(|e| ImportError::Host(e.into()))
            }),
        )?;
    }
    for (name, operation) in [
        ("is_file", HostFs::is_file as fn(&HostFs, &str) -> bool),
        ("is_dir", HostFs::is_dir),
        ("path_exists", HostFs::path_exists),
    ] {
        registry.func_wrap(
            FS,
            name,
            reference_param(move |mut caller: Caller<'_, '_, Runtime>, path: Ref<'_>| {
                let path = caller.values().string(&path)?;
                Ok(i32::from(caller.with_data(|runtime| {
                    operation(runtime.filesystem(), &path)
                })))
            }),
        )?;
    }
    // One result channel per run. The capture contains no Runtime or engine
    // reference, so it cannot form a store/callback ownership cycle.
    let results = Arc::new(Mutex::new(FsOperationResults::default()));
    for (name, operation) in [
        (
            "read_file_to_bytes_new",
            HostFs::read_file_to_bytes_new as fn(&HostFs, &mut FsOperationResults, &str) -> i32,
        ),
        ("create_dir_new", HostFs::create_dir_new),
        ("read_dir_new", HostFs::read_dir_new),
        ("is_file_new", HostFs::is_file_new),
        ("is_dir_new", HostFs::is_dir_new),
        ("remove_file_new", HostFs::remove_file_new),
        ("remove_dir_new", HostFs::remove_dir_new),
    ] {
        let results = Arc::clone(&results);
        registry.func_wrap(
            FS,
            name,
            reference_param(move |mut caller: Caller<'_, '_, Runtime>, path: Ref<'_>| {
                let path = caller.values().string(&path)?;
                let mut results = results.lock().unwrap_or_else(|e| e.into_inner());
                Ok(
                    caller
                        .with_data(|runtime| operation(runtime.filesystem(), &mut results, &path)),
                )
            }),
        )?;
    }
    let state = Arc::clone(&results);
    registry.func_wrap(
        FS,
        "write_bytes_to_file_new",
        reference_pair(
            move |mut caller: Caller<'_, '_, Runtime>, path: Ref<'_>, contents: Ref<'_>| {
                let path = caller.values().string(&path)?;
                let filesystem = caller.with_data(|runtime| Arc::clone(runtime.filesystem()));
                let mut results = state.lock().unwrap_or_else(|e| e.into_inner());
                // Preserve authorization before converting an untrusted byte payload.
                Ok(filesystem.write_bytes_to_file_new(&mut results, &path, || {
                    caller.values().bytes(&contents).map_err(|e| e.to_string())
                }))
            },
        ),
    )?;
    let state = Arc::clone(&results);
    registry.func_wrap(
        FS,
        "get_file_content",
        reference_result(move |mut caller: Caller<'_, '_, Runtime>| {
            let bytes = state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .file_content()
                .to_vec();
            caller.values().new_bytes(bytes)
        }),
    )?;
    let state = Arc::clone(&results);
    registry.func_wrap(
        FS,
        "get_dir_files",
        reference_result(move |mut caller: Caller<'_, '_, Runtime>| {
            let files = state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .dir_files()
                .to_vec();
            caller.values().new_strings(files)
        }),
    )?;
    registry.func_wrap(
        FS,
        "get_error_message",
        reference_result(move |mut caller: Caller<'_, '_, Runtime>| {
            let message = results
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .error_message()
                .to_owned();
            caller.values().new_string(&message)
        }),
    )?;
    registry.func_wrap(
        "__moonbit_io_unstable",
        "read_bytes_from_stdin",
        read_bytes_from_stdin,
    )?;
    for namespace in ["__moonbit_io_unstable", "spectest"] {
        registry.func_wrap(namespace, "read_char", |caller: Caller<'_, '_, Runtime>| {
            Ok(caller
                .with_data(|runtime| runtime.stdio().read_utf8_char())
                .ok()
                .flatten()
                .map_or(-1, |c| c as i32))
        })?;
    }
    registry.func_wrap(
        "__moonbit_io_unstable",
        "write_char",
        |caller: Caller<'_, '_, Runtime>, fd: i32, value: i32| {
            let value = char::from_u32(value as u32)
                .ok_or_else(|| ImportError::Operation("invalid character".into()))?;
            caller
                .with_data(|runtime| match fd {
                    1 => runtime.stdio().with_stdout(|s| write!(s, "{value}")),
                    2 => runtime.stdio().with_stderr(|s| write!(s, "{value}")),
                    _ => Ok(()),
                })
                .map_err(|e| ImportError::Host(e.into()))
        },
    )?;
    registry.func_wrap(
        "__moonbit_io_unstable",
        "flush",
        |caller: Caller<'_, '_, Runtime>, fd: i32| {
            caller
                .with_data(|runtime| match fd {
                    1 => runtime.stdio().with_stdout(|s| s.flush()),
                    2 => runtime.stdio().with_stderr(|s| s.flush()),
                    _ => Ok(()),
                })
                .map_err(|e| ImportError::Host(e.into()))
        },
    )?;
    registry.func_wrap("__moonbit_rand_unstable", "stdrng_seed_from_u64", new_rng)?;
    registry.func_wrap("__moonbit_rand_unstable", "stdrng_gen_range", random)?;
    registry.func_wrap("__moonbit_time_unstable", "instant_now", new_instant)?;
    registry.func_wrap(
        "__moonbit_time_unstable",
        "instant_elapsed_as_secs_f64",
        elapsed,
    )?;
    registry.func_wrap(
        "__moonbit_time_unstable",
        "now",
        |_: Caller<'_, '_, Runtime>| {
            let milliseconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| ImportError::Host(e.into()))?
                .as_millis();
            Ok(u64::try_from(milliseconds).map_err(|e| ImportError::Host(e.into()))? as i64)
        },
    )?;
    registry.func_wrap(
        "__moonbit_sys_unstable",
        "is_windows",
        |_: Caller<'_, '_, Runtime>| Ok(i32::from(cfg!(windows))),
    )?;
    registry.func_wrap(
        "__moonbit_sys_unstable",
        "exit",
        |caller: Caller<'_, '_, Runtime>, code: i32| -> Result<()> {
            Err(caller.terminate(crate::run_termination::RunTermination::Exit(code)))
        },
    )?;
    Ok(())
}

fn begin_create_string<'call, T>(mut caller: Caller<'_, 'call, T>) -> Result<Ref<'call>> {
    caller.values().string_builder()
}
fn string_append_char<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    builder: Ref<'call>,
    unit: i32,
) -> Result<()> {
    caller.values().append_char(&builder, unit)
}
fn finish_create_string<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    builder: Ref<'call>,
) -> Result<Ref<'call>> {
    caller.values().finish_string(&builder)
}
fn begin_create_byte_array<'call, T>(mut caller: Caller<'_, 'call, T>) -> Result<Ref<'call>> {
    caller.values().byte_builder()
}
fn byte_array_append_byte<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    builder: Ref<'call>,
    byte: i32,
) -> Result<()> {
    caller.values().append_byte(&builder, byte)
}
fn finish_create_byte_array<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    builder: Ref<'call>,
) -> Result<Ref<'call>> {
    caller.values().finish_bytes(&builder)
}
fn begin_read_string<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    string: Ref<'call>,
) -> Result<Ref<'call>> {
    caller.values().string_reader(&string)
}
fn string_read_char<'call, T>(mut caller: Caller<'_, 'call, T>, reader: Ref<'call>) -> Result<i32> {
    caller.values().read_char(&reader)
}
fn begin_read_byte_array<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    bytes: Ref<'call>,
) -> Result<Ref<'call>> {
    caller.values().byte_reader(&bytes)
}
fn byte_array_read_byte<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    reader: Ref<'call>,
) -> Result<i32> {
    caller.values().read_byte(&reader)
}
fn begin_read_string_array<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    strings: Ref<'call>,
) -> Result<Ref<'call>> {
    caller.values().array_reader(&strings)
}
fn string_array_read_string<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    reader: Ref<'call>,
) -> Result<Ref<'call>> {
    caller.values().read_string(&reader)
}
fn array_len<'call, T>(mut caller: Caller<'_, 'call, T>, array: Ref<'call>) -> Result<i32> {
    caller.values().array_len(&array)
}
fn array_get<'call, T>(
    mut caller: Caller<'_, 'call, T>,
    array: Ref<'call>,
    index: i32,
) -> Result<Ref<'call>> {
    caller.values().array_get(&array, index)
}
fn jsvalue_is_string<'call, T>(mut caller: Caller<'_, 'call, T>, value: Ref<'call>) -> Result<i32> {
    caller.values().is_string(&value)
}
fn new_rng<'call>(mut caller: Caller<'_, 'call, Runtime>, seed: i32) -> Result<Ref<'call>> {
    caller.values().new_rng(seed)
}
fn random<'call>(
    mut caller: Caller<'_, 'call, Runtime>,
    rng: Ref<'call>,
    upper: i32,
) -> Result<i32> {
    caller.values().random(&rng, upper)
}
fn new_instant<'call>(mut caller: Caller<'_, 'call, Runtime>) -> Result<Ref<'call>> {
    caller.values().new_instant()
}
fn elapsed<'call>(mut caller: Caller<'_, 'call, Runtime>, instant: Ref<'call>) -> Result<f64> {
    caller.values().elapsed(&instant)
}

fn finish_read<T>(_: Caller<'_, '_, T>, _: Ref<'_>) -> Result<()> {
    Ok(())
}
fn get_env_vars<'call>(mut caller: Caller<'_, 'call, Runtime>) -> Result<Ref<'call>> {
    let entries = caller
        .with_data(|runtime| runtime.environment().entries())
        .into_iter()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .flat_map(|(key, value)| [key, value])
        .collect();
    caller.values().new_strings(entries)
}
fn get_env_var<'call>(
    mut caller: Caller<'_, 'call, Runtime>,
    key: Ref<'call>,
) -> Result<Ref<'call>> {
    let key = caller.values().string(&key)?;
    let value = caller.with_data(|runtime| {
        runtime
            .environment()
            .get(key.as_ref())
            .and_then(|v| v.into_string().ok())
            .unwrap_or_default()
    });
    caller.values().new_string(&value)
}
fn get_env_var_exists<'call>(
    mut caller: Caller<'_, 'call, Runtime>,
    key: Ref<'call>,
) -> Result<i32> {
    let key = caller.values().string(&key)?;
    Ok(i32::from(caller.with_data(|runtime| {
        runtime.environment().get(key.as_ref()).is_some()
    })))
}
fn set_env_var<'call>(
    mut caller: Caller<'_, 'call, Runtime>,
    key: Ref<'call>,
    value: Ref<'call>,
) -> Result<()> {
    let key = caller.values().string(&key)?;
    let value = caller.values().string(&value)?;
    // The legacy ABI has no error channel for environment mutation.
    caller.with_data(|runtime| {
        let _ = runtime.environment().set(key.into(), value.into());
    });
    Ok(())
}
fn unset_env_var<'call>(mut caller: Caller<'_, 'call, Runtime>, key: Ref<'call>) -> Result<()> {
    let key = caller.values().string(&key)?;
    caller.with_data(|runtime| {
        let _ = runtime.environment().unset(key.as_ref());
    });
    Ok(())
}
fn read_file_to_string<'call>(
    mut caller: Caller<'_, 'call, Runtime>,
    path: Ref<'call>,
) -> Result<Ref<'call>> {
    let path = caller.values().string(&path)?;
    let contents = caller
        .with_data(|runtime| runtime.filesystem().read_file_to_string(&path))
        .map_err(|e| ImportError::Host(e.into()))?;
    caller.values().new_string(&contents)
}
fn write_string_to_file<'call>(
    mut caller: Caller<'_, 'call, Runtime>,
    path: Ref<'call>,
    contents: Ref<'call>,
) -> Result<()> {
    let path = caller.values().string(&path)?;
    let contents = caller.values().string(&contents)?;
    caller
        .with_data(|runtime| runtime.filesystem().write_string_to_file(&path, &contents))
        .map_err(|e| ImportError::Host(e.into()))
}
fn write_bytes_to_file<'call>(
    mut caller: Caller<'_, 'call, Runtime>,
    path: Ref<'call>,
    contents: Ref<'call>,
) -> Result<()> {
    let path = caller.values().string(&path)?;
    let filesystem = caller.with_data(|runtime| Arc::clone(runtime.filesystem()));
    filesystem
        .write_bytes_to_file::<anyhow::Error>(&path, || {
            caller.values().bytes(&contents).map_err(Into::into)
        })
        .map_err(ImportError::Host)
}
fn read_dir<'call>(mut caller: Caller<'_, 'call, Runtime>, path: Ref<'call>) -> Result<Ref<'call>> {
    let path = caller.values().string(&path)?;
    let entries = caller
        .with_data(|runtime| runtime.filesystem().read_dir(&path))
        .map_err(|e| ImportError::Host(e.into()))?;
    caller.values().new_strings(entries)
}
fn current_dir<'call>(mut caller: Caller<'_, 'call, Runtime>) -> Result<Ref<'call>> {
    let path = caller.with_data(|runtime| runtime.filesystem().current_dir());
    caller.values().new_string(&path)
}
fn read_bytes_from_stdin<'call>(mut caller: Caller<'_, 'call, Runtime>) -> Result<Ref<'call>> {
    let mut bytes = Vec::new();
    caller
        .with_data(|runtime| runtime.stdio().with_stdin(|s| s.read_to_end(&mut bytes)))
        .map_err(|e| ImportError::Host(e.into()))?;
    caller.values().new_bytes(bytes)
}

pub(crate) fn register_values<T: 'static, R: ImportRegistry<T>>(
    registry: &mut R,
) -> std::result::Result<(), R::Error> {
    registry.func_wrap(FS, "begin_create_string", begin_create_string)?;
    registry.func_wrap(FS, "string_append_char", string_append_char)?;
    registry.func_wrap(FS, "finish_create_string", finish_create_string)?;
    registry.func_wrap(FS, "begin_create_byte_array", begin_create_byte_array)?;
    registry.func_wrap(FS, "byte_array_append_byte", byte_array_append_byte)?;
    registry.func_wrap(FS, "finish_create_byte_array", finish_create_byte_array)?;
    registry.func_wrap(FS, "begin_read_string", begin_read_string)?;
    registry.func_wrap(FS, "string_read_char", string_read_char)?;
    registry.func_wrap(FS, "begin_read_byte_array", begin_read_byte_array)?;
    registry.func_wrap(FS, "byte_array_read_byte", byte_array_read_byte)?;
    registry.func_wrap(FS, "begin_read_string_array", begin_read_string_array)?;
    registry.func_wrap(FS, "string_array_read_string", string_array_read_string)?;
    registry.func_wrap(FS, "array_len", array_len)?;
    registry.func_wrap(FS, "array_get", array_get)?;
    registry.func_wrap(FS, "jsvalue_is_string", jsvalue_is_string)?;
    for name in [
        "finish_read_string",
        "finish_read_byte_array",
        "finish_read_string_array",
    ] {
        // Finish releases no engine root and does not invalidate guest aliases.
        registry.func_wrap(FS, name, finish_read)?;
    }
    Ok(())
}
