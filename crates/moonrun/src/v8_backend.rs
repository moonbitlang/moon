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

use crate::async_policy::AsyncPolicy;
use crate::instance_signal::signal_channel;
use crate::run_termination::RunTermination;
use crate::run_termination::TerminationRequest;
use crate::runtime::{RunOptions, RunOutcome, RuntimeConfig};
use crate::v8_builder::{ObjectExt, ScopeExt};
use crate::{demangle_js_template, host_imports, memory_sanitizer_api};
use anyhow::Context;
use std::path::Path;
use std::sync::{Arc, OnceLock};

const BUILTIN_SCRIPT_ORIGIN_PREFIX: &str = "__$moonrun_v8_builtin_script$__";

pub(crate) fn initialize(config: &RuntimeConfig) -> anyhow::Result<()> {
    static ACTIVE_CONFIG: OnceLock<RuntimeConfig> = OnceLock::new();

    let active = ACTIVE_CONFIG.get_or_init(|| {
        v8::V8::set_flags_from_string("--experimental-wasm-exnref");
        v8::V8::set_flags_from_string("--experimental-wasm-imported-strings");
        if let Some(stack_size) = &config.stack_size {
            v8::V8::set_flags_from_string(&format!("--stack-size={stack_size}"));
        }
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
        config.clone()
    });

    if active != config {
        anyhow::bail!(
            "Moonrun's V8 runtime is already initialized with a different process-wide configuration"
        );
    }
    Ok(())
}

pub(crate) fn run(
    config: &RuntimeConfig,
    file: &Path,
    options: RunOptions,
    async_policy: Arc<AsyncPolicy>,
) -> anyhow::Result<RunOutcome> {
    initialize(config)?;

    let isolate = &mut v8::Isolate::new(Default::default());
    let termination_request = TerminationRequest::default();
    let signal_receiver = options
        .signal_receiver
        .unwrap_or_else(|| signal_channel().1);
    let _engine_signal_attachment =
        signal_receiver.attach_engine(isolate.thread_safe_handle(), termination_request.clone());
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);

    let mut script =
        format!(r#"const BUILTIN_SCRIPT_ORIGIN_PREFIX = "{BUILTIN_SCRIPT_ORIGIN_PREFIX}";"#);

    let global_proxy = scope.get_current_context().global(scope);
    let wasm_file_name = file.to_string_lossy().to_string();
    let module_key = scope.string("module_name").into();
    let module_name = scope.string(file.to_string_lossy().as_ref()).into();
    global_proxy.set(scope, module_key, module_name);
    script.push_str("let bytes;");
    let memory_sanitizer = memory_sanitizer_api::MemorySanitizer::default();

    let mut dtors = Vec::new();
    host_imports::install(
        &mut dtors,
        scope,
        &wasm_file_name,
        &options.args,
        async_policy,
        termination_request.clone(),
        signal_receiver,
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
        script.push_str(&format!("const packageName = {:?};", test_args.package));
        script.push_str(&format!("const testParams = {test_params:?};"));
    }
    script.push_str(&format!(
        "const no_stack_trace = {};",
        options.no_stack_trace
    ));
    script.push_str(&format!(
        "const test_mode = {};",
        options.test_args.is_some()
    ));
    script.push_str(demangle_js_template::DEMANGLE_JS_TEMPLATE);
    script.push('\n');
    let js_glue = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/template/js_glue.js"
    ));
    script.push_str(js_glue);

    let code = scope.string(&script);
    let script_origin = create_script_origin(scope, "wasm_mode_entry");
    let script = v8::Script::compile(scope, code, Some(&script_origin)).unwrap();

    script.run(scope);
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
