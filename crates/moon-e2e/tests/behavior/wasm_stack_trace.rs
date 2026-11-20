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

use cucumber::{gherkin::Step, given, when};
use moon_test_util::test_dir::TestDir;

use super::{TEST_CASES_ROOT, World, support};

#[given(expr = "the MoonBit sources in fixture {string}")]
fn source_project(world: &mut World, fixture: String) {
    let workspace = TestDir::new_empty();
    world.cwd = Some(Path::new(TEST_CASES_ROOT).join(fixture));
    world.target_dir = Some(workspace.join("_build"));
    world._workspace = Some(workspace);
}

#[when(expr = "I compile and run the tests for WasmGC with flags {string}")]
fn run_test(world: &mut World, flags: String) {
    world.output = Some(
        world
            .command()
            .args([
                "test",
                "--target",
                "wasm-gc",
                "--sort-input",
                "--no-parallelize",
            ])
            .args(flags.split_whitespace())
            .assert(),
    );
}

#[when("I build the main package for WasmGC in release mode with symbols")]
fn build_program(world: &mut World) {
    world.output = Some(
        world
            .command()
            .args([
                "build",
                "--quiet",
                "--release",
                "--no-strip",
                "--target",
                "wasm-gc",
            ])
            .assert(),
    );
}

#[when(expr = "I make the compiled source map {word}")]
fn change_source_map(world: &mut World, state: String) {
    let source_map = world
        .target_dir
        .as_ref()
        .expect("Given a private build directory")
        .join("wasm-gc/release/build/stack_trace.wasm.map");
    assert!(source_map.is_file(), "the compiler produced a source map");
    match state.as_str() {
        "missing" => std::fs::remove_file(source_map).unwrap(),
        "malformed" => std::fs::write(source_map, "invalid source map").unwrap(),
        _ => panic!("unknown source-map state: {state}"),
    }
}

#[when("I replace the compiled source map with:")]
fn replace_source_map(world: &mut World, step: &Step) {
    let source_map = world
        .target_dir
        .as_ref()
        .expect("Given a private build directory")
        .join("wasm-gc/release/build/stack_trace.wasm.map");
    std::fs::write(
        source_map,
        step.docstring.as_deref().expect("source map JSON"),
    )
    .unwrap();
}

#[when("I execute the compiled Wasm with moonrun")]
fn run_wasm(world: &mut World) {
    let wasm = world
        .target_dir
        .as_ref()
        .expect("Given a private build directory")
        .join("wasm-gc/release/build/stack_trace.wasm");
    world.output = Some(
        support::moonrun_cmd(world.project_path())
            .arg(wasm)
            .assert(),
    );
}
