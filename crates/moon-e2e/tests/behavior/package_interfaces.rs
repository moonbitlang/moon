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

use std::fs;

use cucumber::{gherkin::Step, given, then, when};
use snapbox::IntoData;

use super::World;

#[given(expr = "a module has the {string} source layout")]
fn module_source_layout(world: &mut World, layout: String) {
    let case = match layout.as_str() {
        "src directory with a library and main package" => "moon_info_001.in",
        "project root containing a library package" => "moon_info_002.in",
        other => panic!("unknown module source layout: {other}"),
    };
    world.copy_case(case);
}

#[given("a library package has no public declarations")]
fn library_without_public_declarations(world: &mut World) {
    world.copy_case("test_moon_info.in");
}

#[given(expr = "the generated interface at {string} does not exist")]
fn generated_interface_absent(world: &mut World, path: String) {
    assert!(
        !world
            .project_path()
            .join(path)
            .try_exists()
            .expect("check generated package interface"),
        "generated package interface should not exist before moon info",
    );
}

#[when("I generate package interfaces")]
fn run_moon_info(world: &mut World) {
    world.output = Some(world.command().arg("info").assert());
}

#[then(expr = "the generated interface at {string} is exactly:")]
fn generated_interface(world: &mut World, path: String, step: &Step) {
    let actual = fs::read_to_string(world.project_path().join(path))
        .expect("read generated package interface");
    let expected = step.docstring.as_deref().expect("interface docstring");
    // Gherkin keeps a framing newline around docstrings. Remove only those two
    // framing bytes so an intentional final newline in the generated file stays.
    let expected = expected.strip_prefix('\n').unwrap_or(expected);
    let expected = expected.strip_suffix('\n').unwrap_or(expected);
    snapbox::assert_data_eq!(actual, expected.raw());
}
