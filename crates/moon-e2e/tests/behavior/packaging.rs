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

use cucumber::{given, when};

use super::World;

#[given(expr = "a packaging project with {string}")]
fn packaging_project(world: &mut World, policy: String) {
    let case = match policy.as_str() {
        "re-included directories" => "test_include_001.in",
        "re-included extensions" => "test_include_002.in",
        "a later file re-inclusion" => "test_include_003.in",
        "an ignored directory" => "test_exclude_001.in",
        "an ignored extension" => "test_exclude_002.in",
        _ => panic!("unknown packaging policy: {policy}"),
    };
    world.copy_case(case);
}

#[when("I list the package archive contents")]
fn list_archive(world: &mut World) {
    world.output = Some(world.command().args(["package", "--list"]).assert());
}
