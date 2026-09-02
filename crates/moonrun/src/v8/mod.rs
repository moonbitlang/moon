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

pub(crate) mod builder;
pub(crate) mod context;
mod host_imports;
mod memory_sanitizer;
mod wasi;

use crate::engine::{
    BackendCallOutcome, BackendRunOutcome, EngineConfig, RunOptions, complete_run_call,
    run_test_driver,
};
use crate::run_termination::TerminationRequest;
use crate::runtime::Runtime;
use crate::source_map::SourceMap;
use crate::wasm_diagnostic::{self, DiagnosticLine};
use anyhow::Context;
use builder::ScopeExt;
use std::sync::{Arc, OnceLock};

const BUILTIN_SCRIPT_ORIGIN_PREFIX: &str = "__$moonrun_v8_builtin_script$__";

pub(crate) struct CompiledModule(v8::CompiledWasmModule);

#[derive(Clone, Copy)]
struct ExceptionClassifier<'s, 'm> {
    runtime_error: v8::Local<'s, v8::Object>,
    wasm_exception: v8::Local<'s, v8::Object>,
    source_map: Option<&'m SourceMap>,
    no_stack_trace: bool,
}

enum CallOutcome<'s> {
    Returned(v8::Local<'s, v8::Value>),
    Stopped(BackendCallOutcome),
}

fn classify_exception<'s>(
    scope: &mut v8::HandleScope<'s>,
    exception: v8::Local<'s, v8::Value>,
    classifier: ExceptionClassifier<'s, '_>,
) -> anyhow::Result<BackendCallOutcome> {
    let guest_failure = exception
        .instance_of(scope, classifier.runtime_error)
        .unwrap()
        || exception
            .instance_of(scope, classifier.wasm_exception)
            .unwrap();
    let formatted = format_exception(scope, exception, classifier)?;
    if guest_failure {
        Ok(BackendCallOutcome::GuestFailure(formatted))
    } else {
        // The outer anyhow reporter adds its own `Error:` label. V8 includes
        // that label in a generic JavaScript Error stack, so avoid doubling it.
        let formatted = formatted.strip_prefix("Error: ").unwrap_or(&formatted);
        Err(anyhow::anyhow!("{formatted}"))
    }
}

fn format_exception(
    scope: &mut v8::HandleScope,
    exception: v8::Local<v8::Value>,
    classifier: ExceptionClassifier<'_, '_>,
) -> anyhow::Result<String> {
    let stack_key = scope.string("stack");
    let stack = exception
        .to_object(scope)
        .and_then(|object| object.get(scope, stack_key.into()))
        .filter(|value| !value.is_null_or_undefined())
        .and_then(|value| value.to_string(scope))
        .or_else(|| exception.to_string(scope))
        .context("Moonrun could not read the V8 exception stack")?
        .to_rust_string_lossy(scope);
    let lines = stack
        .split('\n')
        .filter(|line| !line.contains(BUILTIN_SCRIPT_ORIGIN_PREFIX))
        .map(parse_stack_line);
    Ok(wasm_diagnostic::render(
        lines,
        classifier.source_map,
        classifier.no_stack_trace,
    ))
}

fn parse_stack_line(line: &str) -> DiagnosticLine {
    let body = line.trim_start();
    let indentation = &line[..line.len() - body.len()];
    let Some(frame) = body.strip_prefix("at ") else {
        return DiagnosticLine::Text(line.to_owned());
    };

    if let Some(frame) = frame.strip_suffix(')')
        && let Some((function, location)) = frame.rsplit_once(" (")
    {
        let module_offset = location
            .trim_end()
            .rsplit_once(":0x")
            .map(|(_, offset)| offset)
            .filter(|offset| {
                !offset.is_empty() && offset.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .and_then(|offset| usize::from_str_radix(offset, 16).ok());
        return DiagnosticLine::Frame {
            indentation: indentation.to_owned(),
            function: function.to_owned(),
            module_offset,
        };
    }

    let function = frame.trim_end();
    if !function.is_empty() && !function.chars().any(char::is_whitespace) {
        return DiagnosticLine::Frame {
            indentation: indentation.to_owned(),
            function: function.to_owned(),
            module_offset: None,
        };
    }
    DiagnosticLine::Text(line.to_owned())
}

fn invoke<'s>(
    scope: &mut v8::HandleScope<'s>,
    termination_request: &TerminationRequest,
    classifier: ExceptionClassifier<'s, '_>,
    call: impl FnOnce(&mut v8::HandleScope<'s>) -> Option<v8::Local<'s, v8::Value>>,
) -> anyhow::Result<CallOutcome<'s>> {
    let scope = &mut v8::TryCatch::new(scope);
    let result = call(scope);
    if let Some(termination) = termination_request.take() {
        return Ok(CallOutcome::Stopped(BackendCallOutcome::Terminated(
            termination,
        )));
    }
    if let Some(result) = result {
        return Ok(CallOutcome::Returned(result));
    }
    if scope.has_terminated() {
        anyhow::bail!("V8 execution terminated without a Moonrun termination request");
    }

    let exception = scope
        .exception()
        .context("V8 execution failed without an exception")?;
    Ok(CallOutcome::Stopped(classify_exception(
        scope, exception, classifier,
    )?))
}

fn call_export<'s>(
    scope: &mut v8::HandleScope<'s>,
    function: v8::Local<'s, v8::Function>,
    arguments: &[v8::Local<'s, v8::Value>],
    termination_request: &TerminationRequest,
    classifier: ExceptionClassifier<'s, '_>,
) -> anyhow::Result<BackendCallOutcome> {
    let outcome = invoke(scope, termination_request, classifier, |scope| {
        let receiver = v8::undefined(scope).into();
        function.call(scope, receiver, arguments)
    })?;
    Ok(match outcome {
        CallOutcome::Returned(_) => BackendCallOutcome::Completed,
        CallOutcome::Stopped(outcome) => outcome,
    })
}

fn exported_function<'s>(
    scope: &mut v8::HandleScope<'s>,
    exports: v8::Local<'s, v8::Object>,
    name: &str,
) -> anyhow::Result<Option<v8::Local<'s, v8::Function>>> {
    let key = scope.string(name);
    let value = exports.get(scope, key.into()).unwrap();
    if value.is_undefined() {
        return Ok(None);
    }
    Ok(Some(v8::Local::<v8::Function>::try_from(value).map_err(
        |_| anyhow::anyhow!("export `{name}` is not a function"),
    )?))
}

#[derive(Clone, Debug)]
pub(crate) struct Engine {
    config: EngineConfig,
}

impl Engine {
    pub(crate) fn new(config: EngineConfig) -> Self {
        Self { config }
    }

    pub(crate) fn compile(&self, bytes: &[u8]) -> anyhow::Result<CompiledModule> {
        compile(&self.config, bytes)
    }

    pub(crate) fn run(
        &self,
        module_name: &str,
        module: &CompiledModule,
        source_map: Option<&SourceMap>,
        options: RunOptions,
        runtime: Runtime,
    ) -> anyhow::Result<BackendRunOutcome> {
        run(
            &self.config,
            module_name,
            module,
            source_map,
            options,
            runtime,
        )
    }
}

pub(crate) fn initialize(config: &EngineConfig) -> anyhow::Result<()> {
    static ACTIVE_CONFIG: OnceLock<EngineConfig> = OnceLock::new();

    let active = ACTIVE_CONFIG.get_or_init(|| {
        v8::V8::set_flags_from_string("--experimental-wasm-exnref");
        v8::V8::set_flags_from_string("--experimental-wasm-imported-strings");
        if let Some(stack_size) = config.stack_size {
            v8::V8::set_flags_from_string(&format!("--stack-size={stack_size}"));
        }
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
        *config
    });

    if active != config {
        anyhow::bail!(
            "Moonrun's V8 engine is already initialized with a different process-wide configuration"
        );
    }
    Ok(())
}

pub(crate) fn compile(config: &EngineConfig, bytes: &[u8]) -> anyhow::Result<CompiledModule> {
    initialize(config)?;

    let isolate = &mut v8::Isolate::new(Default::default());
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);
    let scope = &mut v8::TryCatch::new(scope);

    // rusty_v8 0.106 does not expose V8's compile-time-import options through
    // WasmModuleObject::compile. Invoke the JavaScript constructor through the
    // V8 interface so loading preserves moonrun's historical string behavior,
    // then retain only V8's shareable compiled module.
    let backing_store = v8::ArrayBuffer::new_backing_store_from_bytes(bytes.to_vec()).make_shared();
    let array_buffer = v8::ArrayBuffer::with_backing_store(scope, &backing_store);
    let bytes = v8::Uint8Array::new(scope, array_buffer, 0, bytes.len())
        .context("failed to create the WebAssembly compilation buffer")?;

    let global_proxy = scope.get_current_context().global(scope);
    let webassembly_key = scope.string("WebAssembly");
    let webassembly = global_proxy
        .get(scope, webassembly_key.into())
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
        .context("V8 did not provide WebAssembly")?;
    let module_key = scope.string("Module");
    let module_constructor = webassembly
        .get(scope, module_key.into())
        .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok())
        .context("V8 did not provide the WebAssembly.Module constructor")?;

    let builtins = v8::Array::new(scope, 1);
    let js_string = scope.string("js-string");
    builtins
        .set_index(scope, 0, js_string.into())
        .context("failed to configure WebAssembly string builtins")?;
    let options = v8::Object::new(scope);
    let builtins_key = scope.string("builtins");
    options
        .set(scope, builtins_key.into(), builtins.into())
        .context("failed to configure WebAssembly string builtins")?;
    let constants_key = scope.string("importedStringConstants");
    let constants_module = scope.string("_");
    options
        .set(scope, constants_key.into(), constants_module.into())
        .context("failed to configure WebAssembly string constants")?;

    let arguments = [bytes.into(), options.into()];
    let module = module_constructor
        .new_instance(scope, &arguments)
        .and_then(|value| v8::Local::<v8::WasmModuleObject>::try_from(value).ok());
    let Some(module) = module else {
        let message = scope
            .message()
            .map(|message| message.get(scope).to_rust_string_lossy(scope))
            .unwrap_or_else(|| "V8 rejected the WebAssembly module".to_owned());
        anyhow::bail!(message);
    };
    Ok(CompiledModule(module.get_compiled_module()))
}

pub(crate) fn run(
    config: &EngineConfig,
    module_name: &str,
    module: &CompiledModule,
    source_map: Option<&SourceMap>,
    options: RunOptions,
    runtime: Runtime,
) -> anyhow::Result<BackendRunOutcome> {
    initialize(config)?;
    let test_args = options.parsed_test_args()?;

    let isolate = &mut v8::Isolate::new(Default::default());
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    let global_proxy = scope.get_current_context().global(scope);
    let webassembly_key = scope.string("WebAssembly");
    let webassembly =
        v8::Local::<v8::Object>::try_from(global_proxy.get(scope, webassembly_key.into()).unwrap())
            .unwrap();
    let runtime_error_key = scope.string("RuntimeError");
    let runtime_error = v8::Local::<v8::Object>::try_from(
        webassembly.get(scope, runtime_error_key.into()).unwrap(),
    )
    .unwrap();
    let exception_key = scope.string("Exception");
    let wasm_exception =
        v8::Local::<v8::Object>::try_from(webassembly.get(scope, exception_key.into()).unwrap())
            .unwrap();
    let instance_key = scope.string("Instance");
    let instance_constructor =
        v8::Local::<v8::Function>::try_from(webassembly.get(scope, instance_key.into()).unwrap())
            .unwrap();
    let wasm_module = v8::WasmModuleObject::from_compiled_module(scope, &module.0)
        .context("failed to load compiled WebAssembly module into the run isolate")?;
    let memory_sanitizer = crate::memory_sanitizer::MemorySanitizer::default();
    let stdio = Arc::clone(runtime.stdio());

    let installed_imports = host_imports::install(
        scope,
        module_name,
        &options.args,
        runtime,
        &memory_sanitizer,
    )?;
    let module_imports = installed_imports.module_imports;
    let termination_request = &installed_imports.termination_request;
    let memory_binding = &installed_imports.memory_binding;

    let classifier = ExceptionClassifier {
        runtime_error,
        wasm_exception,
        source_map,
        no_stack_trace: options.no_stack_trace,
    };

    let result = (|| -> anyhow::Result<BackendRunOutcome> {
        let instance_arguments = [wasm_module.into(), module_imports.into()];
        let instance = match invoke(scope, termination_request, classifier, |scope| {
            instance_constructor
                .new_instance(scope, &instance_arguments)
                .map(Into::into)
        })
        .with_context(|| format!("failed to instantiate `{module_name}`"))?
        {
            CallOutcome::Returned(instance) => v8::Local::<v8::Object>::try_from(instance).unwrap(),
            CallOutcome::Stopped(outcome) => return complete_run_call(outcome, &stdio),
        };
        let exports_key = scope.string("exports");
        let exports =
            v8::Local::<v8::Object>::try_from(instance.get(scope, exports_key.into()).unwrap())
                .unwrap();

        let memory_key = scope.string("memory");
        let memory = exports.get(scope, memory_key.into()).unwrap();
        if let Ok(memory) = v8::Local::<v8::WasmMemoryObject>::try_from(memory) {
            memory_binding
                .bind(scope, memory)
                .map_err(|error| anyhow::anyhow!("failed to bind exported memory: {error:?}"))?;
        }

        if let Some(test_args) = test_args {
            let execute =
                exported_function(scope, exports, "moonbit_test_driver_internal_execute")?
                    .context(
                        "test module does not export `moonbit_test_driver_internal_execute`",
                    )?;
            let finish = exported_function(scope, exports, "moonbit_test_driver_finish")?
                .context("test module does not export `moonbit_test_driver_finish`")?;
            run_test_driver(
                scope,
                test_args,
                &stdio,
                |scope, file, index| {
                    let file = scope.string(file).into();
                    let index = v8::Integer::new_from_unsigned(scope, index).into();
                    call_export(
                        scope,
                        execute,
                        &[file, index],
                        termination_request,
                        classifier,
                    )
                    .context("failed to execute a MoonBit test")
                },
                |scope| {
                    call_export(scope, finish, &[], termination_request, classifier)
                        .context("failed to finish the MoonBit test driver")
                },
            )
        } else if let Some(start) = exported_function(scope, exports, "_start")? {
            let outcome = call_export(scope, start, &[], termination_request, classifier)?;
            complete_run_call(outcome, &stdio)
        } else {
            Ok(BackendRunOutcome::Completed)
        }
    })();
    drop(installed_imports);

    let outcome = result?;
    if matches!(outcome, BackendRunOutcome::Completed) {
        memory_sanitizer.check_for_leaks()?;
    }
    Ok(outcome)
}
