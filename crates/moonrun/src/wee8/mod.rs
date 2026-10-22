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

//! Experimental Wasm backend using V8's C entry points. The public Moonrun
//! Engine/Module/Run interface is shared with the other backends. The upstream
//! Rust crate owns native lifetimes and the C ABI; this adapter wires Moonrun
//! host services. GC objects remain internal to guest execution.

use ::wee8 as api;

mod runtime_imports;
mod stack;
mod wasm_imports;

use crate::engine::{
    BackendCallOutcome, BackendRunOutcome, EngineConfig, TestArgs, complete_run_call,
    run_test_driver,
};
use crate::run_termination::TerminationRequest;
use crate::runtime::{Runtime, Utf16Writer};
use crate::source_map::SourceMap;
use crate::wasm_imports::{Caller, ImportRegistry};
use anyhow::Context;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct Engine {
    initialization: Result<(), String>,
    stack_size: Option<usize>,
}

#[derive(Debug)]
pub(crate) struct CompiledModule {
    code: api::Compiled,
    functions: Arc<std::collections::BTreeMap<u32, String>>,
}

struct StoreData {
    runtime: Runtime,
    print: Utf16Writer,
    termination: TerminationRequest,
    wasi: crate::wasi::WasiContext,
    functions: Arc<std::collections::BTreeMap<u32, String>>,
    memory_sanitizer: crate::memory_sanitizer::MemorySanitizer,
}

impl AsRef<StoreData> for StoreData {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl AsRef<Runtime> for StoreData {
    fn as_ref(&self) -> &Runtime {
        &self.runtime
    }
}
impl AsRef<crate::wasi::WasiContext> for StoreData {
    fn as_ref(&self) -> &crate::wasi::WasiContext {
        &self.wasi
    }
}

impl Engine {
    pub(crate) fn new(config: EngineConfig) -> Self {
        Self {
            initialization: api::initialize().map_err(|error| error.to_string()),
            stack_size: config.stack_size,
        }
    }

    pub(crate) fn compile(&self, bytes: &[u8]) -> anyhow::Result<CompiledModule> {
        self.initialization
            .as_ref()
            .map_err(|error| anyhow::anyhow!(error.clone()))?;
        let store = api::Store::new(())?;
        let module = store.compile_with_options(
            bytes,
            &api::CompileOptions {
                js_string_builtins: true,
                imported_string_constants: Some("_".into()),
            },
        )?;
        let mut functions = std::collections::BTreeMap::new();
        for payload in wasmparser::Parser::new(0).parse_all(bytes) {
            if let wasmparser::Payload::CustomSection(section) = payload?
                && let wasmparser::KnownCustom::Name(names) = section.as_known()
            {
                // Names are optional debug metadata. Ignore malformed entries
                // rather than rejecting code already validated by V8.
                for name in names.into_iter().flatten() {
                    if let wasmparser::Name::Function(names) = name {
                        for name in names.into_iter().flatten() {
                            functions.insert(name.index, name.name.to_owned());
                        }
                    }
                }
            }
        }
        Ok(CompiledModule {
            code: module.serialize(),
            functions: Arc::new(functions),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run(
        &self,
        module_name: &str,
        compiled: &CompiledModule,
        source_map: Option<&SourceMap>,
        args: &[String],
        test_args: Option<TestArgs>,
        no_stack_trace: bool,
        runtime: Runtime,
    ) -> anyhow::Result<BackendRunOutcome> {
        self.initialization
            .as_ref()
            .map_err(|error| anyhow::anyhow!(error.clone()))?;
        let termination = TerminationRequest::default();
        let wasi = crate::wasi::WasiContext::new(module_name, args, &runtime, termination.clone());
        let stdio = Arc::clone(runtime.stdio());
        let memory_sanitizer = crate::memory_sanitizer::MemorySanitizer::default();
        let store = api::Store::new(StoreData {
            runtime,
            print: Utf16Writer::default(),
            termination: termination.clone(),
            wasi,
            functions: Arc::clone(&compiled.functions),
            memory_sanitizer: memory_sanitizer.clone(),
        })?;
        let stack_limit = stack::Limit::new(&store, self.stack_size)?;
        let module = store.module(&compiled.code)?;
        let registry = (|| -> api::Result<_> {
            let mut registry = wasm_imports::Registry::new(&store, &module)?;
            registry.register_exceptions()?;
            runtime_imports::register(&mut registry)?;
            crate::wasi::imports::register(&mut registry)?;
            crate::async_api::register(&mut registry)?;
            crate::sqlite::wasm::register(&mut registry)?;
            crate::core_api::register(&mut registry)?;
            crate::host_imports::register(&mut registry, module_name, args)?;
            // The GC compiler uses the legacy character ABI. Reuse the same
            // per-run UTF-16 writer as V8 and Wasmtime, including surrogate pairs.
            registry.func_wrap(
                "spectest",
                "print_char",
                |caller: Caller<'_, '_, StoreData>, value: i32| {
                    caller
                        .with_data(|data| {
                            data.print.write_stdout(data.runtime.stdio(), value as u32)
                        })
                        .map_err(|error| crate::wasm_imports::ImportError::Host(error.into()))
                },
            )?;
            Ok(registry)
        })()
        .with_context(|| format!("failed to instantiate `{module_name}`"))?;
        let imports = registry
            .finish()
            .with_context(|| format!("failed to instantiate `{module_name}`"))?;
        let mut instance = match module.instantiate_with_imports(&imports) {
            Ok(instance) => instance,
            Err(error) => {
                return complete_run_call(
                    call_outcome(&store, &termination, source_map, no_stack_trace, Err(error))
                        .with_context(|| format!("failed to instantiate `{module_name}`"))?,
                    &stdio,
                );
            }
        };
        if let Some(termination) = termination.take() {
            return Ok(BackendRunOutcome::Terminated(termination));
        }

        let result = if let Some(test_args) = test_args {
            for export in [
                "moonbit_test_driver_internal_execute",
                "moonbit_test_driver_finish",
            ] {
                anyhow::ensure!(
                    instance.has_export(export),
                    "test module does not export `{export}`"
                );
            }
            run_test_driver(
                &mut instance,
                test_args,
                &stdio,
                |instance, file, index| {
                    let file =
                        api::StringRef::from_utf8(store.as_context(), file)?.into_extern_ref();
                    call_outcome(
                        &store,
                        &termination,
                        source_map,
                        no_stack_trace,
                        instance.call(
                            "moonbit_test_driver_internal_execute",
                            &[
                                api::Value::ExternRef(Some(file)),
                                api::Value::I32(index as i32),
                            ],
                        ),
                    )
                    .context("failed to execute a MoonBit test")
                },
                |instance| {
                    call_outcome(
                        &store,
                        &termination,
                        source_map,
                        no_stack_trace,
                        instance.call("moonbit_test_driver_finish", &[]),
                    )
                    .context("failed to finish the MoonBit test driver")
                },
            )
        } else if instance.has_export("_start") {
            // Instantiation already executes the Wasm start section. `_start`
            // is optional, as in the other backends.
            complete_run_call(
                call_outcome(
                    &store,
                    &termination,
                    source_map,
                    no_stack_trace,
                    instance.call("_start", &[]),
                )?,
                &stdio,
            )
        } else {
            Ok(BackendRunOutcome::Completed)
        };
        drop(instance);
        drop(imports);
        drop(registry);
        drop(module);
        drop(stack_limit);
        drop(store);
        let outcome = result?;
        if matches!(outcome, BackendRunOutcome::Completed) {
            memory_sanitizer.check_for_leaks()?;
        }
        Ok(outcome)
    }
}

fn call_outcome(
    store: &api::Store<StoreData>,
    termination: &TerminationRequest,
    source_map: Option<&SourceMap>,
    no_stack_trace: bool,
    result: api::Result<Vec<api::Value<'_>>>,
) -> anyhow::Result<BackendCallOutcome> {
    // Release an uncaught exception's native root before executing another test,
    // including when a host exit takes precedence over the guest exception.
    let exception = store.take_exception();
    if let Some(termination) = termination.take() {
        return Ok(BackendCallOutcome::Terminated(termination));
    }
    Ok(match result {
        Ok(values) if values.is_empty() => BackendCallOutcome::Completed,
        Ok(_) => anyhow::bail!("wee8 entry point must return no values"),
        Err(api::Error::Trap(trap)) => BackendCallOutcome::GuestFailure(render_failure(
            store,
            trap.message.trim_start_matches("Uncaught ").to_owned(),
            &trap.frames,
            source_map,
            no_stack_trace,
        )),
        Err(api::Error::ThrownException) => BackendCallOutcome::GuestFailure(render_failure(
            store,
            "Error".into(),
            &exception
                .map(|exception| exception.trace())
                .unwrap_or_default(),
            source_map,
            no_stack_trace,
        )),
        Err(api::Error::Host(error)) => {
            return Err(anyhow::anyhow!(render_failure(
                store,
                error.message,
                &error.frames,
                source_map,
                no_stack_trace,
            )));
        }
        Err(error) => return Err(error.into()),
    })
}

fn render_failure(
    store: &api::Store<StoreData>,
    message: String,
    frames: &[api::StackFrame],
    source_map: Option<&SourceMap>,
    no_stack_trace: bool,
) -> String {
    use crate::wasm_diagnostic::DiagnosticLine;
    let lines = std::iter::once(DiagnosticLine::Text(message)).chain(frames.iter().map(|frame| {
        DiagnosticLine::Frame {
            indentation: "    ".into(),
            function: store
                .as_context()
                .with_data(|data| data.function_name(frame.function_index)),
            module_offset: Some(frame.module_offset),
        }
    }));
    crate::wasm_diagnostic::render(lines, source_map, no_stack_trace)
}
impl StoreData {
    fn function_name(&self, index: u32) -> String {
        self.functions
            .get(&index)
            .cloned()
            .unwrap_or_else(|| format!("wasm-function[{index}]"))
    }
}
