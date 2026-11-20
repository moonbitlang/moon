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

use std::path::Path;

use cucumber::{given, when};
use moon_test_util::test_dir::TestDir;

use super::{TEST_CASES_ROOT, World};

#[given("a versioned Wasm executable that aborts")]
fn failing_program(world: &mut World) {
    let workspace = TestDir::new_empty();
    world.cwd = Some(Path::new(TEST_CASES_ROOT).join("wasm_stack_trace"));
    world.target_dir = Some(workspace.join("_build"));
    world._workspace = Some(workspace);
}

#[when("I run the executable in release mode for WasmGC")]
fn run_program(world: &mut World) {
    world.output = Some(
        world
            .command()
            .args([
                "run",
                ".",
                "--release",
                "--no-strip",
                "--target",
                "wasm-gc",
                "--sort-input",
            ])
            .assert(),
    );
}
