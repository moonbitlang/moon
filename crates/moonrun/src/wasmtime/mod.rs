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

//! Wasmtime Engine Backend for Moonrun's Wasm execution surface.

mod host_imports;
mod wasi;

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use ::wasmtime as wt;
use ::wasmtime::{
    AsContext, AsContextMut, Caller, Collector, ExnRef, ExnRefPre, ExnType, ExternRef, ExternType,
    FuncType, Global, GlobalType, HeapType, Linker, Mutability, RefType, Rooted, Store, Strategy,
    Tag, TagType, Val, ValType,
};
use anyhow::Context;

use crate::engine::{
    BackendCallOutcome, BackendRunOutcome, EngineConfig, RunOptions, complete_run_call,
    run_test_driver,
};
use crate::run_termination::{RunTermination, TerminationRequest};
use crate::runtime::{Runtime, Utf16Writer};
use crate::source_map::SourceMap;
use crate::wasm_diagnostic::{self, DiagnosticLine};

/// Immutable UTF-16 storage shared by JS-string values and host readers.
#[derive(Clone, Debug, PartialEq, Eq)]
struct JsString(Arc<[u16]>);

pub(crate) struct StoreData {
    runtime: Runtime,
    termination_request: TerminationRequest,
    print: Utf16Writer,
    exception_tag: Option<Tag>,
    wasi: crate::wasi::WasiContext,
    host_imports: host_imports::State,
    memory_sanitizer: crate::memory_sanitizer::MemorySanitizer,
}

impl StoreData {
    pub(crate) fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub(crate) fn termination_request(&self) -> &TerminationRequest {
        &self.termination_request
    }

    pub(crate) fn wasi(&self) -> &crate::wasi::WasiContext {
        &self.wasi
    }
}

#[derive(Clone)]
pub(crate) struct Engine {
    inner: Result<::wasmtime::Engine, Arc<str>>,
}

impl Engine {
    pub(crate) fn new(config: EngineConfig) -> Self {
        let mut wasmtime_config = ::wasmtime::Config::new();
        wasmtime_config
            .strategy(Strategy::Cranelift)
            .gc_support(true)
            .collector(Collector::Copying)
            .wasm_reference_types(true)
            .wasm_function_references(true)
            .wasm_gc(true)
            .wasm_exceptions(true);
        let stack_size = config
            .stack_size
            .map(|size| {
                size.checked_mul(1024)
                    .ok_or_else(|| Arc::<str>::from("Wasmtime stack size overflows bytes"))
            })
            .transpose();
        if let Ok(Some(stack_size)) = &stack_size {
            wasmtime_config.max_wasm_stack(*stack_size);
        }
        Self {
            inner: stack_size.and_then(|_| {
                ::wasmtime::Engine::new(&wasmtime_config)
                    .map_err(|error| Arc::<str>::from(error.to_string()))
            }),
        }
    }

    fn inner(&self) -> anyhow::Result<&::wasmtime::Engine> {
        self.inner
            .as_ref()
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    pub(crate) fn compile(&self, bytes: &[u8]) -> anyhow::Result<CompiledModule> {
        ::wasmtime::Module::new(self.inner()?, bytes)
            .map(CompiledModule)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    pub(crate) fn run(
        &self,
        module_name: &str,
        module: &CompiledModule,
        source_map: Option<&SourceMap>,
        options: RunOptions,
        runtime: Runtime,
    ) -> anyhow::Result<BackendRunOutcome> {
        let test_args = options.parsed_test_args()?;
        let engine = self.inner()?;
        let termination_request = TerminationRequest::default();
        let wasi = crate::wasi::WasiContext::new(
            module_name,
            &options.args,
            &runtime,
            termination_request.clone(),
        );
        let stdio = Arc::clone(runtime.stdio());
        let memory_sanitizer = crate::memory_sanitizer::MemorySanitizer::default();
        let mut store = Store::new(
            engine,
            StoreData {
                runtime,
                termination_request: termination_request.clone(),
                print: Utf16Writer::default(),
                exception_tag: None,
                wasi,
                host_imports: host_imports::State::new(module_name, &options.args),
                memory_sanitizer: memory_sanitizer.clone(),
            },
        );
        let linker = linker_for_module(engine, &mut store, &module.0)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
            .with_context(|| format!("failed to instantiate `{module_name}`"))?;
        let instance = match linker.instantiate(&mut store, &module.0) {
            Ok(instance) => instance,
            Err(error) => {
                let outcome = if let Some(termination) = termination_request.take() {
                    BackendCallOutcome::Terminated(termination)
                } else if is_guest_failure(&error) {
                    BackendCallOutcome::GuestFailure(format_wasm_error(
                        &error,
                        source_map,
                        options.no_stack_trace,
                    ))
                } else if error.downcast_ref::<wt::WasmBacktrace>().is_some() {
                    return Err(anyhow::anyhow!(format_host_error(
                        &error,
                        source_map,
                        options.no_stack_trace,
                    )))
                    .with_context(|| format!("failed to instantiate `{module_name}`"));
                } else {
                    return Err(anyhow::Error::from(error)
                        .context(format!("failed to instantiate `{module_name}`")));
                };
                return complete_run_call(outcome, &stdio);
            }
        };

        let result = if let Some(test_args) = test_args {
            let execute = instance
                .get_func(&mut store, "moonbit_test_driver_internal_execute")
                .context("test module does not export `moonbit_test_driver_internal_execute`")?;
            let finish = instance
                .get_func(&mut store, "moonbit_test_driver_finish")
                .context("test module does not export `moonbit_test_driver_finish`")?;
            run_test_driver(
                &mut store,
                test_args,
                &stdio,
                |store, file, index| {
                    let file =
                        ExternRef::new(&mut *store, JsString(file.encode_utf16().collect()))?;
                    call_export(
                        store,
                        &execute,
                        &[Val::ExternRef(Some(file)), Val::I32(index as i32)],
                        source_map,
                        options.no_stack_trace,
                    )
                    .context("failed to execute a MoonBit test")
                },
                |store| {
                    call_export(store, &finish, &[], source_map, options.no_stack_trace)
                        .context("failed to finish the MoonBit test driver")
                },
            )
        } else if let Some(start) = instance.get_func(&mut store, "_start") {
            let outcome = call_export(&mut store, &start, &[], source_map, options.no_stack_trace)?;
            complete_run_call(outcome, &stdio)
        } else {
            Ok(BackendRunOutcome::Completed)
        };
        drop(store);

        let outcome = result?;
        if matches!(outcome, BackendRunOutcome::Completed) {
            memory_sanitizer.check_for_leaks()?;
        }
        Ok(outcome)
    }
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WasmtimeEngine")
            .field("initialized", &self.inner.is_ok())
            .finish()
    }
}

pub(crate) struct CompiledModule(::wasmtime::Module);

fn linker_for_module(
    engine: &::wasmtime::Engine,
    store: &mut Store<StoreData>,
    module: &::wasmtime::Module,
) -> wt::Result<Linker<StoreData>> {
    let mut linker = Linker::new(engine);
    wasi::register_imports(&mut linker)?;
    crate::async_api::register_wasmtime_imports(&mut linker)?;
    crate::sqlite::wasm::register_wasmtime_imports(&mut linker)?;
    let mut defined = HashSet::new();

    for import in module.imports() {
        let key = (import.module().to_owned(), import.name().to_owned());
        if !defined.insert(key) {
            continue;
        }
        match (import.module(), import.name(), import.ty()) {
            (wasi::WASI_SNAPSHOT_PREVIEW1_MODULE, _, ExternType::Func(_)) => {}
            (crate::async_api::MOONBIT_ASYNC_MODULE, _, ExternType::Func(_)) => {}
            (crate::sqlite::wasm::MOONBIT_SQLITE_MODULE, _, ExternType::Func(_)) => {}
            ("ffi-bytes", "memory", ExternType::Memory(_)) => {
                host_imports::define_ffi_bytes_memory(&mut linker, store)?;
            }
            (namespace, name, ExternType::Func(_))
                if host_imports::define_import(&mut linker, namespace, name)? => {}
            ("_", literal, ExternType::Global(_)) => {
                let value =
                    ExternRef::new(&mut *store, JsString(literal.encode_utf16().collect()))?;
                let ty = GlobalType::new(non_null_externref(), Mutability::Const);
                let global = Global::new(&mut *store, ty, Val::ExternRef(Some(value)))?;
                linker.define(&mut *store, "_", literal, global)?;
            }
            ("wasm:js-string", name, ExternType::Func(ty)) => {
                register_js_string_builtin(&mut linker, name, ty)?;
            }
            ("console", "log", ExternType::Func(_)) => {
                linker.func_wrap(
                    "console",
                    "log",
                    |caller: Caller<'_, StoreData>, value: Rooted<ExternRef>| {
                        let units = require_string(caller.as_context(), Some(&value))?;
                        let value = String::from_utf16_lossy(&units.0);
                        caller
                            .data()
                            .runtime
                            .stdio()
                            .with_stdout(|stdout| writeln!(stdout, "{value}"))?;
                        Ok(())
                    },
                )?;
            }
            ("spectest", "print_char", ExternType::Func(_)) => {
                linker.func_wrap(
                    "spectest",
                    "print_char",
                    |caller: Caller<'_, StoreData>, value: i32| {
                        caller
                            .data()
                            .print
                            .write_stdout(caller.data().runtime.stdio(), value as u32)?;
                        Ok(())
                    },
                )?;
            }
            ("exception", "tag", ExternType::Tag(_)) => {
                let ty = TagType::new(FuncType::new(engine, [], []));
                let tag = Tag::new(&mut *store, &ty)?;
                if store.data_mut().exception_tag.replace(tag).is_some() {
                    wt::bail!("duplicate `exception.tag` import");
                }
                linker.define(&mut *store, "exception", "tag", tag)?;
            }
            ("exception", "throw", ExternType::Func(_)) => {
                linker.func_wrap("exception", "throw", |mut caller: Caller<'_, StoreData>| {
                    let tag = caller
                        .data()
                        .exception_tag
                        .ok_or_else(|| wt::format_err!("missing `exception.tag` import"))?;
                    let exception_type = ExnType::from_tag_type(&tag.ty(caller.as_context()))?;
                    let allocator = ExnRefPre::new(&mut caller, exception_type);
                    let exception = ExnRef::new(&mut caller, &allocator, &tag, &[])?;
                    caller.as_context_mut().throw::<()>(exception)
                })?;
            }
            ("__moonbit_sys_unstable", "is_windows", ExternType::Func(_)) => {
                linker.func_wrap("__moonbit_sys_unstable", "is_windows", || {
                    i32::from(cfg!(windows))
                })?;
            }
            ("__moonbit_sys_unstable", "exit", ExternType::Func(_)) => {
                linker.func_wrap(
                    "__moonbit_sys_unstable",
                    "exit",
                    |caller: Caller<'_, StoreData>, code: i32| -> wt::Result<()> {
                        caller
                            .data()
                            .termination_request
                            .request(RunTermination::Exit(code));
                        wt::bail!("run termination requested")
                    },
                )?;
            }
            (namespace, name, ty) => {
                wt::bail!("unsupported Wasmtime import `{namespace}.{name}`: {ty:?}");
            }
        }
    }
    Ok(linker)
}

fn register_js_string_builtin(
    linker: &mut Linker<StoreData>,
    name: &str,
    ty: FuncType,
) -> wt::Result<()> {
    match name {
        "cast" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |caller: Caller<'_, StoreData>, value: Option<Rooted<ExternRef>>| {
                    require_builtin_string(caller.as_context(), value.as_ref())?;
                    value.ok_or_else(|| wt::Trap::UnreachableCodeReached.into())
                },
            )?;
        }
        "test" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |caller: Caller<'_, StoreData>, value: Option<Rooted<ExternRef>>| {
                    Ok(i32::from(matches!(
                        js_string_value(caller.as_context(), value.as_ref())?,
                        JsStringValue::String(_)
                    )))
                },
            )?;
        }
        "fromCharCode" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |mut caller: Caller<'_, StoreData>, value: i32| {
                    ExternRef::new(&mut caller, JsString(vec![value as u16].into()))
                },
            )?;
        }
        "fromCodePoint" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |mut caller: Caller<'_, StoreData>, code_point: i32| {
                    let code_point = code_point as u32;
                    let units = match code_point {
                        0..=0xffff => vec![code_point as u16],
                        0x10000..=0x10ffff => {
                            let value = code_point - 0x10000;
                            vec![
                                0xd800 | ((value >> 10) as u16),
                                0xdc00 | ((value & 0x3ff) as u16),
                            ]
                        }
                        _ => return js_string_trap(),
                    };
                    ExternRef::new(&mut caller, JsString(units.into()))
                },
            )?;
        }
        "charCodeAt" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |caller: Caller<'_, StoreData>, value: Option<Rooted<ExternRef>>, index: i32| {
                    let units = require_builtin_string(caller.as_context(), value.as_ref())?;
                    units
                        .0
                        .get(index as u32 as usize)
                        .copied()
                        .map(i32::from)
                        .ok_or_else(|| wt::Trap::UnreachableCodeReached.into())
                },
            )?;
        }
        "codePointAt" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |caller: Caller<'_, StoreData>, value: Option<Rooted<ExternRef>>, index: i32| {
                    let units = require_builtin_string(caller.as_context(), value.as_ref())?;
                    let index = index as u32 as usize;
                    let Some(&first) = units.0.get(index) else {
                        return js_string_trap();
                    };
                    Ok(match (first, units.0.get(index + 1).copied()) {
                        (0xd800..=0xdbff, Some(second @ 0xdc00..=0xdfff)) => {
                            (0x10000
                                + ((u32::from(first) - 0xd800) << 10)
                                + (u32::from(second) - 0xdc00)) as i32
                        }
                        _ => i32::from(first),
                    })
                },
            )?;
        }
        "length" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |caller: Caller<'_, StoreData>, value: Option<Rooted<ExternRef>>| {
                    let units = require_builtin_string(caller.as_context(), value.as_ref())?;
                    Ok(i32::try_from(units.0.len())?)
                },
            )?;
        }
        "concat" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |mut caller: Caller<'_, StoreData>,
                 left: Option<Rooted<ExternRef>>,
                 right: Option<Rooted<ExternRef>>| {
                    let left = require_builtin_string(caller.as_context(), left.as_ref())?;
                    let right = require_builtin_string(caller.as_context(), right.as_ref())?;
                    let mut units = Vec::with_capacity(left.0.len() + right.0.len());
                    units.extend_from_slice(&left.0);
                    units.extend_from_slice(&right.0);
                    ExternRef::new(&mut caller, JsString(units.into()))
                },
            )?;
        }
        "substring" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |mut caller: Caller<'_, StoreData>,
                 value: Option<Rooted<ExternRef>>,
                 start: i32,
                 end: i32| {
                    let units = require_builtin_string(caller.as_context(), value.as_ref())?;
                    let start = start as u32 as usize;
                    let end = end as u32 as usize;
                    let substring = if start > end || start > units.0.len() {
                        Vec::new()
                    } else {
                        units.0[start..end.min(units.0.len())].to_vec()
                    };
                    ExternRef::new(&mut caller, JsString(substring.into()))
                },
            )?;
        }
        "equals" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |caller: Caller<'_, StoreData>,
                 left: Option<Rooted<ExternRef>>,
                 right: Option<Rooted<ExternRef>>| {
                    let left = optional_builtin_string(caller.as_context(), left.as_ref())?;
                    let right = optional_builtin_string(caller.as_context(), right.as_ref())?;
                    Ok(i32::from(left == right))
                },
            )?;
        }
        "compare" => {
            linker.func_wrap(
                "wasm:js-string",
                name,
                |caller: Caller<'_, StoreData>,
                 left: Option<Rooted<ExternRef>>,
                 right: Option<Rooted<ExternRef>>| {
                    let left = require_builtin_string(caller.as_context(), left.as_ref())?;
                    let right = require_builtin_string(caller.as_context(), right.as_ref())?;
                    Ok(match left.0.cmp(&right.0) {
                        std::cmp::Ordering::Less => -1,
                        std::cmp::Ordering::Equal => 0,
                        std::cmp::Ordering::Greater => 1,
                    })
                },
            )?;
        }
        // These imports name a concrete array type declared by the guest module. `func_wrap`
        // can express only the abstract `(ref array)` type, which Wasmtime does not link as
        // the same function parameter type. Preserve the concrete type after checking its
        // mutable-i16 shape; all other callbacks above have statically declared host ABIs.
        "fromCharCodeArray" => {
            validate_from_char_code_array(&ty)?;
            linker.func_new("wasm:js-string", name, ty, |mut caller, params, results| {
                let Some(reference) = params[0].unwrap_anyref() else {
                    return js_string_trap();
                };
                let array = reference.unwrap_array(caller.as_context())?;
                let start = params[1].unwrap_i32() as u32;
                let end = params[2].unwrap_i32() as u32;
                let len = array.len(caller.as_context())?;
                if start > end || end > len {
                    return js_string_trap();
                }

                let mut units = Vec::with_capacity((end - start) as usize);
                for index in start..end {
                    units.push(array.get(&mut caller, index)?.unwrap_i32() as u16);
                }
                let value = ExternRef::new(&mut caller, JsString(units.into()))?;
                results[0] = Val::ExternRef(Some(value));
                Ok(())
            })?;
        }
        "intoCharCodeArray" => {
            validate_into_char_code_array(&ty)?;
            linker.func_new("wasm:js-string", name, ty, |mut caller, params, results| {
                let units = Arc::clone(
                    &require_builtin_string(caller.as_context(), params[0].unwrap_externref())?.0,
                );
                let Some(reference) = params[1].unwrap_anyref() else {
                    return js_string_trap();
                };
                let array = reference.unwrap_array(caller.as_context())?;
                let start = params[2].unwrap_i32() as u32;
                let len = array.len(caller.as_context())?;
                let unit_count = u32::try_from(units.len())?;
                if start.checked_add(unit_count).is_none_or(|end| end > len) {
                    return js_string_trap();
                }
                for (offset, unit) in units.iter().copied().enumerate() {
                    array.set(
                        &mut caller,
                        start + offset as u32,
                        Val::I32(i32::from(unit)),
                    )?;
                }
                results[0] = Val::I32(unit_count as i32);
                Ok(())
            })?;
        }
        _ => wt::bail!("unsupported Wasmtime import `wasm:js-string.{name}`: {ty:?}"),
    }
    Ok(())
}

fn is_guest_failure(error: &wt::Error) -> bool {
    error.root_cause().is::<wt::Trap>() || error.root_cause().is::<wt::ThrownException>()
}

fn call_export(
    store: &mut Store<StoreData>,
    function: &wt::Func,
    arguments: &[Val],
    source_map: Option<&SourceMap>,
    no_stack_trace: bool,
) -> anyhow::Result<BackendCallOutcome> {
    let result = function.call(&mut *store, arguments, &mut []);
    if let Some(termination) = store.data().termination_request.take() {
        return Ok(BackendCallOutcome::Terminated(termination));
    }
    match result {
        Ok(()) => Ok(BackendCallOutcome::Completed),
        Err(error) if is_guest_failure(&error) => Ok(BackendCallOutcome::GuestFailure(
            format_wasm_error(&error, source_map, no_stack_trace),
        )),
        Err(error) => Err(anyhow::anyhow!(format_host_error(
            &error,
            source_map,
            no_stack_trace,
        ))),
    }
}

fn format_wasm_error(
    error: &wasmtime::Error,
    source_map: Option<&SourceMap>,
    no_stack_trace: bool,
) -> String {
    let root = error.root_cause();
    let thrown_exception = root.is::<wasmtime::ThrownException>();
    let message = if matches!(
        root.downcast_ref::<wasmtime::Trap>(),
        Some(wasmtime::Trap::UnreachableCodeReached)
    ) {
        "RuntimeError: unreachable".to_owned()
    } else if thrown_exception {
        "Error".to_owned()
    } else {
        format!("Error: {root}")
    };
    let mut lines = vec![DiagnosticLine::Text(message)];
    append_wasm_backtrace(&mut lines, error);
    wasm_diagnostic::render(lines, source_map, no_stack_trace)
}

fn format_host_error(
    error: &wasmtime::Error,
    source_map: Option<&SourceMap>,
    no_stack_trace: bool,
) -> String {
    let mut lines = vec![DiagnosticLine::Text(error.root_cause().to_string())];
    append_wasm_backtrace(&mut lines, error);
    wasm_diagnostic::render(lines, source_map, no_stack_trace)
}

fn append_wasm_backtrace(lines: &mut Vec<DiagnosticLine>, error: &wasmtime::Error) {
    if let Some(backtrace) = error.downcast_ref::<wasmtime::WasmBacktrace>() {
        for frame in backtrace.frames() {
            let raw_name = frame
                .func_name()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("wasm-function[{}]", frame.func_index()));
            lines.push(DiagnosticLine::Frame {
                indentation: "    ".to_owned(),
                function: raw_name,
                module_offset: frame.module_offset(),
            });
        }
    }
}

fn validate_from_char_code_array(ty: &FuncType) -> wt::Result<()> {
    let params = ty.params().collect::<Vec<_>>();
    let results = ty.results().collect::<Vec<_>>();
    let valid_array = params.first().is_some_and(is_nullable_mutable_i16_array);
    let valid_scalars = params.len() == 3
        && ValType::eq(&params[1], &ValType::I32)
        && ValType::eq(&params[2], &ValType::I32);
    let valid_result = results.len() == 1 && ValType::eq(&results[0], &non_null_externref());
    if !valid_array || !valid_scalars || !valid_result {
        wt::bail!("invalid fromCharCodeArray signature: params {params:?}, results {results:?}");
    }
    Ok(())
}

fn validate_into_char_code_array(ty: &FuncType) -> wt::Result<()> {
    let params = ty.params().collect::<Vec<_>>();
    let results = ty.results().collect::<Vec<_>>();
    let valid_params = params.len() == 3
        && ValType::eq(&params[0], &ValType::EXTERNREF)
        && is_nullable_mutable_i16_array(&params[1])
        && ValType::eq(&params[2], &ValType::I32);
    let valid_result = results.len() == 1 && ValType::eq(&results[0], &ValType::I32);
    if !valid_params || !valid_result {
        wt::bail!("invalid intoCharCodeArray signature: params {params:?}, results {results:?}");
    }
    Ok(())
}

fn is_nullable_mutable_i16_array(ty: &ValType) -> bool {
    match ty {
        ValType::Ref(reference) if reference.is_nullable() => match reference.heap_type() {
            HeapType::ConcreteArray(array) => {
                array.mutability() == Mutability::Var && array.element_type().is_i16()
            }
            _ => false,
        },
        _ => false,
    }
}

fn non_null_externref() -> ValType {
    RefType::new(false, HeapType::Extern).into()
}

fn require_string<'a, T: 'static>(
    store: wt::StoreContext<'a, T>,
    value: Option<&Rooted<ExternRef>>,
) -> wt::Result<&'a JsString> {
    match js_string_value(store, value)? {
        JsStringValue::String(units) => Ok(units),
        JsStringValue::Null => wt::bail!("JS-string operation received null"),
        JsStringValue::Other => wt::bail!("externref is not a JS string"),
    }
}

enum JsStringValue<'a> {
    Null,
    String(&'a JsString),
    Other,
}

fn js_string_value<'a, T: 'static>(
    store: wt::StoreContext<'a, T>,
    value: Option<&Rooted<ExternRef>>,
) -> wt::Result<JsStringValue<'a>> {
    let Some(reference) = value else {
        return Ok(JsStringValue::Null);
    };
    let Some(data) = reference.data(store)? else {
        return Ok(JsStringValue::Other);
    };
    Ok(match data.downcast_ref::<JsString>() {
        Some(string) => JsStringValue::String(string),
        None => JsStringValue::Other,
    })
}

fn require_builtin_string<'a, T: 'static>(
    store: wt::StoreContext<'a, T>,
    value: Option<&Rooted<ExternRef>>,
) -> wt::Result<&'a JsString> {
    match js_string_value(store, value)? {
        JsStringValue::String(units) => Ok(units),
        JsStringValue::Null | JsStringValue::Other => js_string_trap(),
    }
}

fn optional_builtin_string<'a, T: 'static>(
    store: wt::StoreContext<'a, T>,
    value: Option<&Rooted<ExternRef>>,
) -> wt::Result<Option<&'a JsString>> {
    match js_string_value(store, value)? {
        JsStringValue::Null => Ok(None),
        JsStringValue::String(units) => Ok(Some(units)),
        JsStringValue::Other => js_string_trap(),
    }
}

fn js_string_trap<T>() -> wt::Result<T> {
    Err(wt::Trap::UnreachableCodeReached.into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_unsupported_string_constant_namespaces_and_shapes() {
        for (source, expected) in [
            (
                r#"(module (import "my_strings" "value" (global (ref extern))))"#,
                "unsupported Wasmtime import `my_strings.value`",
            ),
            (
                r#"(module (import "_" "value" (global (mut externref))))"#,
                "incompatible import type for `_::value`",
            ),
            (
                r#"(module
                    (import "wasm:js-string" "length"
                        (func (param i32) (result i32))))"#,
                "incompatible import type for `wasm:js-string::length`",
            ),
        ] {
            let engine = crate::Engine::default();
            let module = engine.compile("invalid-import.wasm", wat::parse_str(source).unwrap());
            let error = engine
                .run(&module.unwrap(), crate::RunOptions::default())
                .unwrap_err();
            assert!(
                format!("{error:#}").contains(expected),
                "expected {expected:?} in {error:#}"
            );
        }
    }
}
