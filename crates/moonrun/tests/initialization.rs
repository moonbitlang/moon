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

#![cfg(feature = "v8")]

use moonrun::{Engine, EngineConfig, RunOptions, RunOutcome, initialize};

// A separate test binary keeps the process-wide configuration independent of
// other tests that initialize the backend with its defaults.
#[test]
fn initialization_preserves_the_process_configuration() {
    let config = EngineConfig::default().with_stack_size(512);
    initialize(config).unwrap();
    initialize(config).unwrap();
    assert_eq!(
        initialize(EngineConfig::default()).unwrap_err().to_string(),
        "Moonrun's V8 engine is already initialized with a different process-wide configuration"
    );
    initialize(config).unwrap();

    let engine = Engine::new(config);
    let module = engine.compile("empty.wasm", b"\0asm\x01\0\0\0").unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}
