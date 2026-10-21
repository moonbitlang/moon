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
}

pub(crate) type CompiledModule = api::Compiled;

struct StoreData {
    runtime: Runtime,
    print: Utf16Writer,
    termination: TerminationRequest,
    wasi: crate::wasi::WasiContext,
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
        let initialization = if config.stack_size.is_some() {
            Err("wee8 does not support a custom stack size yet".into())
        } else {
            api::initialize().map_err(|error| error.to_string())
        };
        Self { initialization }
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
        Ok(module.serialize())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run(
        &self,
        module_name: &str,
        compiled: &CompiledModule,
        _source_map: Option<&SourceMap>,
        args: &[String],
        test_args: Option<TestArgs>,
        _no_stack_trace: bool,
        runtime: Runtime,
    ) -> anyhow::Result<BackendRunOutcome> {
        self.initialization
            .as_ref()
            .map_err(|error| anyhow::anyhow!(error.clone()))?;
        let termination = TerminationRequest::default();
        let wasi = crate::wasi::WasiContext::new(module_name, args, &runtime, termination.clone());
        let stdio = Arc::clone(runtime.stdio());
        let store = api::Store::new(StoreData {
            runtime,
            print: Utf16Writer::default(),
            termination: termination.clone(),
            wasi,
        })?;
        let module = store.module(compiled)?;
        let imports = (|| -> api::Result<_> {
            let mut registry = wasm_imports::Registry::new(&store, &module)?;
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
            registry.finish()
        })()
        .with_context(|| format!("failed to instantiate `{module_name}`"))?;
        let mut instance = match module.instantiate(&imports.iter().collect::<Vec<_>>()) {
            Ok(instance) => instance,
            Err(error) => {
                return complete_run_call(call_outcome(Err(error), &termination)?, &stdio);
            }
        };
        if let Some(test_args) = test_args {
            for name in [
                "moonbit_test_driver_internal_execute",
                "moonbit_test_driver_finish",
            ] {
                anyhow::ensure!(
                    instance.has_export(name),
                    "test module does not export `{name}`"
                );
            }
            run_test_driver(
                &mut instance,
                test_args,
                &stdio,
                |instance, file, index| {
                    let file = api::StringRef::from_utf8(store.as_context(), file)?;
                    call_outcome(
                        instance.call(
                            "moonbit_test_driver_internal_execute",
                            &[
                                api::Value::ExternRef(Some(file.into_extern_ref())),
                                api::Value::I32(index as i32),
                            ],
                        ),
                        &termination,
                    )
                    .context("failed to execute a MoonBit test")
                },
                |instance| {
                    call_outcome(
                        instance.call("moonbit_test_driver_finish", &[]),
                        &termination,
                    )
                    .context("failed to finish the MoonBit test driver")
                },
            )
        } else {
            // Instantiation already executes any Wasm start section. The
            // conventional `_start` export is optional, as in other backends.
            let result = if instance.has_export("_start") {
                instance.call("_start", &[])
            } else {
                Ok(Vec::new())
            };
            complete_run_call(call_outcome(result, &termination)?, &stdio)
        }
    }
}

// All entry paths preserve explicit termination and distinguish guest failures
// from host service errors, including the two test-driver exports.
fn call_outcome(
    result: api::Result<Vec<api::Value<'_>>>,
    termination: &TerminationRequest,
) -> anyhow::Result<BackendCallOutcome> {
    if let Some(termination) = termination.take() {
        return Ok(BackendCallOutcome::Terminated(termination));
    }
    match result {
        Ok(values) if values.is_empty() => Ok(BackendCallOutcome::Completed),
        Ok(_) => anyhow::bail!("wee8 entry points must return no values"),
        Err(api::Error::Trap(message)) => Ok(BackendCallOutcome::GuestFailure(message)),
        Err(api::Error::ThrownException) => Ok(BackendCallOutcome::GuestFailure(
            "uncaught Wasm exception".into(),
        )),
        Err(error) => Err(error.into()),
    }
}
