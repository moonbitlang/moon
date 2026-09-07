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

mod wasi;

use std::collections::HashSet;
use std::fmt;
use std::sync::{Arc, Mutex};

use ::wasmtime as wt;
use ::wasmtime::{
    AsContext, AsContextMut, Collector, ExnRef, ExnRefPre, ExnType, ExternRef, ExternType,
    FuncType, Global, HeapType, Linker, Mutability, RefType, Store, Strategy, Tag, Val, ValType,
};
use anyhow::Context;

use crate::engine::{
    BackendCallOutcome, BackendRunOutcome, EngineConfig, RunOptions, complete_run_call,
    run_test_driver,
};
use crate::run_termination::{RunTermination, TerminationRequest};
use crate::runtime::Runtime;
use crate::source_map::SourceMap;
use crate::wasm_diagnostic::{self, DiagnosticLine};

/// Immutable UTF-16 storage shared by JS-string values and host readers.
#[derive(Clone, Debug, PartialEq, Eq)]
struct JsString(Arc<[u16]>);

/// An independent UTF-16 cursor. Wasmtime requires externref host data to be
/// thread-safe even though one Moonrun Store executes synchronously.
#[derive(Debug)]
struct JsStringReader {
    units: Arc<[u16]>,
    index: Mutex<usize>,
}

#[derive(Default)]
struct PrintState {
    dangling_high_half: Option<u32>,
}

pub(crate) struct StoreData {
    runtime: Runtime,
    termination_request: TerminationRequest,
    print: PrintState,
    exception_tag: Option<Tag>,
    wasi: crate::wasi::WasiContext,
}

impl StoreData {
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
        let mut store = Store::new(
            engine,
            StoreData {
                runtime,
                termination_request: termination_request.clone(),
                print: PrintState::default(),
                exception_tag: None,
                wasi,
            },
        );
        let linker = linker_for_module(engine, &mut store, &module.0)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
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
                } else {
                    return Err(anyhow::Error::from(error)
                        .context(format!("failed to instantiate `{module_name}`")));
                };
                return complete_run_call(outcome, &stdio);
            }
        };

        if let Some(test_args) = test_args {
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
        }
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
    let mut defined = HashSet::new();

    for import in module.imports() {
        let key = (import.module().to_owned(), import.name().to_owned());
        if !defined.insert(key) {
            continue;
        }
        match (import.module(), import.name(), import.ty()) {
            (wasi::WASI_SNAPSHOT_PREVIEW1_MODULE, _, ExternType::Func(_)) => {}
            ("_", literal, ExternType::Global(ty)) => {
                if ty.mutability() != Mutability::Const {
                    wt::bail!("imported string constant `{literal}` is mutable");
                }
                validate_types(
                    "imported string constant",
                    [ty.content().clone()],
                    [non_null_externref()],
                )?;
                let value =
                    ExternRef::new(&mut *store, JsString(literal.encode_utf16().collect()))?;
                let global = Global::new(&mut *store, ty, Val::ExternRef(Some(value)))?;
                linker.define(&mut *store, "_", literal, global)?;
            }
            ("wasm:js-string", name, ExternType::Func(ty)) => {
                register_js_string_builtin(&mut linker, name, ty)?;
            }
            ("__moonbit_fs_unstable", "begin_read_string", ExternType::Func(ty)) => {
                validate_func(&ty, [ValType::EXTERNREF], [ValType::EXTERNREF])?;
                linker.func_new(
                    "__moonbit_fs_unstable",
                    "begin_read_string",
                    ty,
                    |mut caller, params, results| {
                        let units = Arc::clone(&require_string(caller.as_context(), &params[0])?.0);
                        let reader = ExternRef::new(
                            &mut caller,
                            JsStringReader {
                                units,
                                index: Mutex::new(0),
                            },
                        )?;
                        results[0] = Val::ExternRef(Some(reader));
                        Ok(())
                    },
                )?;
            }
            ("__moonbit_fs_unstable", "string_read_char", ExternType::Func(ty)) => {
                validate_func(&ty, [ValType::EXTERNREF], [ValType::I32])?;
                linker.func_new(
                    "__moonbit_fs_unstable",
                    "string_read_char",
                    ty,
                    |caller, params, results| {
                        let reference = params[0].unwrap_externref().ok_or_else(|| {
                            wt::format_err!("string reader operation received null")
                        })?;
                        let data = reference
                            .data(caller.as_context())?
                            .ok_or_else(|| wt::format_err!("externref has no host reader data"))?;
                        let reader = data
                            .downcast_ref::<JsStringReader>()
                            .ok_or_else(|| wt::format_err!("externref is not a string reader"))?;
                        let mut index = reader
                            .index
                            .lock()
                            .map_err(|_| wt::format_err!("string reader lock is poisoned"))?;
                        results[0] = Val::I32(match reader.units.get(*index) {
                            Some(unit) => {
                                *index += 1;
                                i32::from(*unit)
                            }
                            None => -1,
                        });
                        Ok(())
                    },
                )?;
            }
            ("__moonbit_fs_unstable", "finish_read_string", ExternType::Func(ty)) => {
                validate_func(&ty, [ValType::EXTERNREF], [])?;
                linker.func_new(
                    "__moonbit_fs_unstable",
                    "finish_read_string",
                    ty,
                    |_caller, _params, _results| Ok(()),
                )?;
            }
            ("console", "log", ExternType::Func(ty)) => {
                validate_func(&ty, [non_null_externref()], [])?;
                linker.func_new("console", "log", ty, |caller, params, _results| {
                    let units = require_string(caller.as_context(), &params[0])?;
                    let value = String::from_utf16_lossy(&units.0);
                    caller
                        .data()
                        .runtime
                        .stdio()
                        .with_stdout(|stdout| writeln!(stdout, "{value}"))?;
                    Ok(())
                })?;
            }
            ("spectest", "print_char", ExternType::Func(ty)) => {
                validate_func(&ty, [ValType::I32], [])?;
                linker.func_new(
                    "spectest",
                    "print_char",
                    ty,
                    |mut caller, params, _results| {
                        print_char(caller.data_mut(), params[0].unwrap_i32() as u32)?;
                        Ok(())
                    },
                )?;
            }
            ("exception", "tag", ExternType::Tag(ty)) => {
                validate_func(ty.ty(), [], [])?;
                let tag = Tag::new(&mut *store, &ty)?;
                if store.data_mut().exception_tag.replace(tag).is_some() {
                    wt::bail!("duplicate `exception.tag` import");
                }
                linker.define(&mut *store, "exception", "tag", tag)?;
            }
            ("exception", "throw", ExternType::Func(ty)) => {
                validate_func(&ty, [], [])?;
                linker.func_new("exception", "throw", ty, |mut caller, _params, _results| {
                    let tag = caller
                        .data()
                        .exception_tag
                        .ok_or_else(|| wt::format_err!("missing `exception.tag` import"))?;
                    let exception_type = ExnType::from_tag_type(&tag.ty(caller.as_context()))?;
                    let allocator = ExnRefPre::new(&mut caller, exception_type);
                    let exception = ExnRef::new(&mut caller, &allocator, &tag, &[])?;
                    caller.as_context_mut().throw(exception)
                })?;
            }
            ("__moonbit_sys_unstable", "is_windows", ExternType::Func(ty)) => {
                validate_func(&ty, [], [ValType::I32])?;
                linker.func_new(
                    "__moonbit_sys_unstable",
                    "is_windows",
                    ty,
                    |_caller, _params, results| {
                        results[0] = Val::I32(i32::from(cfg!(windows)));
                        Ok(())
                    },
                )?;
            }
            ("__moonbit_sys_unstable", "exit", ExternType::Func(ty)) => {
                validate_func(&ty, [ValType::I32], [])?;
                linker.func_new(
                    "__moonbit_sys_unstable",
                    "exit",
                    ty,
                    |caller, params, _results| {
                        caller
                            .data()
                            .termination_request
                            .request(RunTermination::Exit(params[0].unwrap_i32()));
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
            validate_func(&ty, [ValType::EXTERNREF], [non_null_externref()])?;
            linker.func_new("wasm:js-string", name, ty, |caller, params, results| {
                require_builtin_string(caller.as_context(), &params[0])?;
                results[0] = params[0];
                Ok(())
            })?;
        }
        "test" => {
            validate_func(&ty, [ValType::EXTERNREF], [ValType::I32])?;
            linker.func_new("wasm:js-string", name, ty, |caller, params, results| {
                results[0] = Val::I32(i32::from(matches!(
                    js_string_value(caller.as_context(), &params[0])?,
                    JsStringValue::String(_)
                )));
                Ok(())
            })?;
        }
        "fromCharCode" => {
            validate_func(&ty, [ValType::I32], [non_null_externref()])?;
            linker.func_new("wasm:js-string", name, ty, |mut caller, params, results| {
                let value = ExternRef::new(
                    &mut caller,
                    JsString(vec![params[0].unwrap_i32() as u16].into()),
                )?;
                results[0] = Val::ExternRef(Some(value));
                Ok(())
            })?;
        }
        "fromCodePoint" => {
            validate_func(&ty, [ValType::I32], [non_null_externref()])?;
            linker.func_new("wasm:js-string", name, ty, |mut caller, params, results| {
                let code_point = params[0].unwrap_i32() as u32;
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
                let value = ExternRef::new(&mut caller, JsString(units.into()))?;
                results[0] = Val::ExternRef(Some(value));
                Ok(())
            })?;
        }
        "charCodeAt" => {
            validate_func(&ty, [ValType::EXTERNREF, ValType::I32], [ValType::I32])?;
            linker.func_new("wasm:js-string", name, ty, |caller, params, results| {
                let units = require_builtin_string(caller.as_context(), &params[0])?;
                let index = params[1].unwrap_i32() as u32;
                let Some(code_unit) = units.0.get(index as usize) else {
                    return js_string_trap();
                };
                results[0] = Val::I32(i32::from(*code_unit));
                Ok(())
            })?;
        }
        "codePointAt" => {
            validate_func(&ty, [ValType::EXTERNREF, ValType::I32], [ValType::I32])?;
            linker.func_new("wasm:js-string", name, ty, |caller, params, results| {
                let units = require_builtin_string(caller.as_context(), &params[0])?;
                let index = params[1].unwrap_i32() as u32 as usize;
                let Some(&first) = units.0.get(index) else {
                    return js_string_trap();
                };
                let code_point = match (first, units.0.get(index + 1).copied()) {
                    (0xd800..=0xdbff, Some(second @ 0xdc00..=0xdfff)) => {
                        0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00)
                    }
                    _ => u32::from(first),
                };
                results[0] = Val::I32(code_point as i32);
                Ok(())
            })?;
        }
        "length" => {
            validate_func(&ty, [ValType::EXTERNREF], [ValType::I32])?;
            linker.func_new("wasm:js-string", name, ty, |caller, params, results| {
                let units = require_builtin_string(caller.as_context(), &params[0])?;
                results[0] = Val::I32(i32::try_from(units.0.len())?);
                Ok(())
            })?;
        }
        "concat" => {
            validate_func(
                &ty,
                [ValType::EXTERNREF, ValType::EXTERNREF],
                [non_null_externref()],
            )?;
            linker.func_new("wasm:js-string", name, ty, |mut caller, params, results| {
                let left = require_builtin_string(caller.as_context(), &params[0])?;
                let right = require_builtin_string(caller.as_context(), &params[1])?;
                let mut units = Vec::with_capacity(left.0.len() + right.0.len());
                units.extend_from_slice(&left.0);
                units.extend_from_slice(&right.0);
                let value = ExternRef::new(&mut caller, JsString(units.into()))?;
                results[0] = Val::ExternRef(Some(value));
                Ok(())
            })?;
        }
        "substring" => {
            validate_func(
                &ty,
                [ValType::EXTERNREF, ValType::I32, ValType::I32],
                [non_null_externref()],
            )?;
            linker.func_new("wasm:js-string", name, ty, |mut caller, params, results| {
                let units = require_builtin_string(caller.as_context(), &params[0])?;
                let start = params[1].unwrap_i32() as u32 as usize;
                let end = params[2].unwrap_i32() as u32 as usize;
                let substring = if start > end || start > units.0.len() {
                    Vec::new()
                } else {
                    units.0[start..end.min(units.0.len())].to_vec()
                };
                let value = ExternRef::new(&mut caller, JsString(substring.into()))?;
                results[0] = Val::ExternRef(Some(value));
                Ok(())
            })?;
        }
        "equals" => {
            validate_func(
                &ty,
                [ValType::EXTERNREF, ValType::EXTERNREF],
                [ValType::I32],
            )?;
            linker.func_new("wasm:js-string", name, ty, |caller, params, results| {
                let left = optional_builtin_string(caller.as_context(), &params[0])?;
                let right = optional_builtin_string(caller.as_context(), &params[1])?;
                results[0] = Val::I32(i32::from(left == right));
                Ok(())
            })?;
        }
        "compare" => {
            validate_func(
                &ty,
                [ValType::EXTERNREF, ValType::EXTERNREF],
                [ValType::I32],
            )?;
            linker.func_new("wasm:js-string", name, ty, |caller, params, results| {
                let left = require_builtin_string(caller.as_context(), &params[0])?;
                let right = require_builtin_string(caller.as_context(), &params[1])?;
                results[0] = Val::I32(match left.0.cmp(&right.0) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                });
                Ok(())
            })?;
        }
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
                let units = Arc::clone(&require_builtin_string(caller.as_context(), &params[0])?.0);
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
        Err(error) => Err(error.into()),
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
    wasm_diagnostic::render(lines, source_map, no_stack_trace)
}

fn print_char(data: &mut StoreData, value: u32) -> wt::Result<()> {
    let stdio = Arc::clone(data.runtime.stdio());
    if (0xd800..=0xdbff).contains(&value) {
        if data
            .print
            .dangling_high_half
            .replace(value - 0xd800)
            .is_some()
        {
            stdio.with_stdout(|stdout| write!(stdout, "\u{fffd}"))?;
        }
        return Ok(());
    }

    let value = if (0xdc00..=0xdfff).contains(&value) {
        data.print
            .dangling_high_half
            .take()
            .map_or(0xfffd, |high| 0x10000 + (high << 10) + (value - 0xdc00))
    } else {
        value
    };
    let value = char::from_u32(value).ok_or_else(|| wt::format_err!("invalid character"))?;
    stdio.with_stdout(|stdout| write!(stdout, "{value}"))?;
    Ok(())
}

pub(super) fn validate_func<const P: usize, const R: usize>(
    ty: &FuncType,
    params: [ValType; P],
    results: [ValType; R],
) -> wt::Result<()> {
    validate_types("function parameters", ty.params(), params)?;
    validate_types("function results", ty.results(), results)
}

pub(super) fn validate_types(
    label: &str,
    actual: impl IntoIterator<Item = ValType>,
    expected: impl IntoIterator<Item = ValType>,
) -> wt::Result<()> {
    let actual = actual.into_iter().collect::<Vec<_>>();
    let expected = expected.into_iter().collect::<Vec<_>>();
    if actual.len() != expected.len()
        || !actual
            .iter()
            .zip(&expected)
            .all(|(actual, expected)| ValType::eq(actual, expected))
    {
        wt::bail!("invalid Wasmtime import {label}: expected {expected:?}, found {actual:?}");
    }
    Ok(())
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
    value: &Val,
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
    value: &Val,
) -> wt::Result<JsStringValue<'a>> {
    let Some(reference) = value.unwrap_externref() else {
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
    value: &Val,
) -> wt::Result<&'a JsString> {
    match js_string_value(store, value)? {
        JsStringValue::String(units) => Ok(units),
        JsStringValue::Null | JsStringValue::Other => js_string_trap(),
    }
}

fn optional_builtin_string<'a, T: 'static>(
    store: wt::StoreContext<'a, T>,
    value: &Val,
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
                "imported string constant `value` is mutable",
            ),
            (
                r#"(module
                    (import "wasm:js-string" "length"
                        (func (param i32) (result i32))))"#,
                "invalid Wasmtime import function parameters",
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
