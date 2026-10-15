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

//! Experimental numeric Wasm backend using V8's C entry points. The public Moonrun
//! Engine/Module/Run interface is shared with the other backends. The upstream
//! Rust crate owns native lifetimes and the C ABI; this adapter wires Moonrun
//! host services. Extending host references is a separate step.

use ::wee8 as api;
mod wasi;

use crate::engine::{
    BackendCallOutcome, BackendRunOutcome, EngineConfig, TestArgs, complete_run_call,
};
use crate::run_termination::TerminationRequest;
use crate::runtime::Runtime;
use crate::source_map::SourceMap;
use anyhow::Context;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct Engine {
    initialization: Result<(), String>,
}

pub(crate) type CompiledModule = api::Compiled;

struct StoreData {
    _runtime: Runtime,
    wasi: crate::wasi::WasiContext,
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
        let module = store.compile(bytes)?;
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
        anyhow::ensure!(
            test_args.is_none(),
            "wee8 does not support the MoonBit test driver yet"
        );
        let termination = TerminationRequest::default();
        let wasi = crate::wasi::WasiContext::new(module_name, args, &runtime, termination.clone());
        let stdio = Arc::clone(runtime.stdio());
        let store = api::Store::new(StoreData {
            _runtime: runtime,
            wasi,
        })?;
        let module = store.module(compiled)?;
        let imports = wasi::imports(&store, &module)
            .with_context(|| format!("failed to instantiate `{module_name}`"))?;
        let result = module
            .instantiate(&imports.iter().collect::<Vec<_>>())
            .and_then(|mut instance| {
                // Instantiation already executes any Wasm start section. The
                // conventional `_start` export is optional, as in other backends.
                if instance.has_export("_start") {
                    instance.call("_start", &[])
                } else {
                    Ok(Vec::new())
                }
            });
        let outcome = if let Some(termination) = termination.take() {
            BackendCallOutcome::Terminated(termination)
        } else {
            match result {
                Ok(values) if values.is_empty() => BackendCallOutcome::Completed,
                Ok(_) => anyhow::bail!("wee8 requires `_start` to return no values"),
                Err(api::Error::Trap(message)) => BackendCallOutcome::GuestFailure(message),
                Err(error) => return Err(error.into()),
            }
        };
        complete_run_call(outcome, &stdio)
    }
}
