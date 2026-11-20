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

use cucumber::{given, then, when};

use super::World;

#[given(expr = "a {string} project to clean")]
fn project_to_clean(world: &mut World, project: String) {
    let case = match project.as_str() {
        "single-module" => "clean/clean.in",
        "workspace" => "workspace_basic.in",
        other => panic!("unknown project kind: {other}"),
    };
    world.copy_case(case);
}

#[when("I build the project")]
fn build(world: &mut World) {
    world.output = Some(world.command().arg("build").assert());
}

#[when("I clean the project")]
fn clean(world: &mut World) {
    world.output = Some(world.command().arg("clean").assert());
}

#[then(expr = "the project build directory {string} exists")]
fn build_directory_exists(world: &mut World, path: String) {
    assert!(
        world.project_path().join(&path).is_dir(),
        "project build directory {path} should exist after moon build",
    );
}

#[then(expr = "the project build directory {string} does not exist")]
fn build_directory_absent(world: &mut World, path: String) {
    assert!(
        !world
            .project_path()
            .join(&path)
            .try_exists()
            .expect("check project build directory"),
        "project build directory {path} should not exist after moon clean",
    );
}
