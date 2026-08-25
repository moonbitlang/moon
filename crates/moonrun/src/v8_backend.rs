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

use crate::engine::{EngineConfig, RunOptions, RunOutcome};
use crate::env::Env;
use crate::policy::Policy;
use crate::run_termination::RunTermination;
use crate::v8_builder::{ObjectExt, ScopeExt};
use crate::{demangle_js_template, host_imports, memory_sanitizer_api};
use anyhow::Context;
use std::sync::{Arc, OnceLock};

const BUILTIN_SCRIPT_ORIGIN_PREFIX: &str = "__$moonrun_v8_builtin_script$__";
const JS_GLUE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/template/js_glue.js"
));

pub(crate) struct CompiledModule(v8::CompiledWasmModule);

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
    source_map: Option<&str>,
    options: RunOptions,
    policy: Arc<Policy>,
    environment: Arc<Env>,
) -> anyhow::Result<RunOutcome> {
    initialize(config)?;

    let isolate = &mut v8::Isolate::new(Default::default());
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    let mut entrypoint_source =
        format!(r#"const BUILTIN_SCRIPT_ORIGIN_PREFIX = "{BUILTIN_SCRIPT_ORIGIN_PREFIX}";"#);

    let global_proxy = scope.get_current_context().global(scope);
    let wasm_module = v8::WasmModuleObject::from_compiled_module(scope, &module.0)
        .context("failed to load compiled WebAssembly module into the run isolate")?;
    let memory_sanitizer = memory_sanitizer_api::MemorySanitizer::default();

    let mut dtors = Vec::new();
    let termination_request = host_imports::install(
        &mut dtors,
        scope,
        module_name,
        &options.args,
        policy,
        environment,
    );

    let memory_sanitizer_imports =
        global_proxy.child(scope, memory_sanitizer_api::MEMORY_SANITIZER_MODULE);
    memory_sanitizer_api::init_env(memory_sanitizer_imports, scope, &memory_sanitizer);

    if let Some(ref test_args) = options.test_args {
        let test_args = serde_json_lenient::from_str::<TestArgs>(test_args)
            .context("invalid MoonBit test arguments")?;
        let file_and_index = test_args.file_and_index;

        let mut test_params: Vec<[String; 2]> = vec![];
        for (file, index) in file_and_index {
            for range in index {
                for i in range {
                    test_params.push([file.clone(), i.to_string()]);
                }
            }
        }
        entrypoint_source.push_str(&format!("const packageName = {:?};", test_args.package));
        entrypoint_source.push_str(&format!("const testParams = {test_params:?};"));
    }
    entrypoint_source.push_str(&format!(
        "const no_stack_trace = {};",
        options.no_stack_trace
    ));
    entrypoint_source.push_str(&format!(
        "const test_mode = {};",
        options.test_args.is_some()
    ));
    entrypoint_source.push_str(demangle_js_template::DEMANGLE_JS_TEMPLATE);
    entrypoint_source.push('\n');
    entrypoint_source.push_str(JS_GLUE);

    let code = scope.string(&entrypoint_source);
    let script_origin = create_script_origin(scope, "wasm_mode_entry");
    let mut source = v8::script_compiler::Source::new(code, Some(&script_origin));
    let module_argument = scope.string("module");
    let module_name_argument = scope.string("module_name");
    let source_map_argument = scope.string("source_map");
    let entry = v8::script_compiler::compile_function(
        scope,
        &mut source,
        &[module_argument, module_name_argument, source_map_argument],
        &[],
        v8::script_compiler::CompileOptions::NoCompileOptions,
        v8::script_compiler::NoCacheReason::BecauseCachingDisabled,
    )
    .context("failed to compile Moonrun's Wasm entrypoint")?;
    let receiver = v8::undefined(scope).into();
    let module_name = scope.string(module_name);
    let source_map = source_map
        .map(|source_map| scope.string(source_map).into())
        .unwrap_or_else(|| v8::undefined(scope).into());
    entry.call(
        scope,
        receiver,
        &[wasm_module.into(), module_name.into(), source_map],
    );
    let termination = termination_request.take();
    drop(dtors);
    if let Some(termination) = termination {
        return Ok(match termination {
            RunTermination::Exit(code) => RunOutcome::Exited(code),
            RunTermination::KilledBySignal(signal) => RunOutcome::KilledBySignal(signal),
        });
    }
    memory_sanitizer.check_for_leaks()?;
    Ok(RunOutcome::Completed)
}

fn create_script_origin<'s>(scope: &mut v8::HandleScope<'s>, name: &str) -> v8::ScriptOrigin<'s> {
    let name = format!("{BUILTIN_SCRIPT_ORIGIN_PREFIX}{name}");
    let name = scope.string(&name);
    v8::ScriptOrigin::new(
        scope,
        name.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    )
}

#[derive(serde::Deserialize)]
struct TestArgs {
    package: String,
    file_and_index: Vec<(String, Vec<std::ops::Range<u32>>)>,
}

#[cfg(test)]
mod tests {
    use super::JS_GLUE;

    #[test]
    fn js_glue_does_not_own_runtime_values() {
        assert!(!JS_GLUE.contains("__moonbit_run_env"));
        assert!(!JS_GLUE.contains("function env_get_var"));
        assert!(!JS_GLUE.contains("function args_get"));
    }
}
