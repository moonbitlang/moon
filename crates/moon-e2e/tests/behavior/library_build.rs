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

use cucumber::{given, then, when};

use super::{World, preview};

#[given("a root package importing a nested library")]
fn library_project(world: &mut World) {
    world.copy_case("moon_build_package.in");
}

#[then(expr = "{string} is compiled before {string} and imported as {string}")]
fn library_dependency(world: &mut World, dependency: String, consumer: String, alias: String) {
    let commands = preview::commands(world);
    let builds = commands
        .iter()
        .filter(|args| args.get(1).is_some_and(|arg| arg == "build-package"))
        .collect::<Vec<_>>();
    let dependency_index = builds
        .iter()
        .position(|args| preview::option(args, "-pkg") == Some(&dependency))
        .expect("dependency compilation");
    let consumer_index = builds
        .iter()
        .position(|args| preview::option(args, "-pkg") == Some(&consumer))
        .expect("consumer compilation");
    assert!(
        dependency_index < consumer_index,
        "dependency must be compiled first"
    );
    // build-package emits a sibling .mi alongside its -o code output. Follow
    // that relationship rather than fixing a target directory layout.
    let interface =
        Path::new(preview::option(builds[dependency_index], "-o").expect("compiler output"))
            .with_extension("mi");
    let import = format!("{}:{alias}", interface.to_string_lossy());
    assert!(
        builds[consumer_index]
            .windows(2)
            .any(|pair| pair[0] == "-i" && pair[1] == import),
        "{consumer} must import {dependency}'s generated interface as {alias}"
    );
}

#[when("I build the libraries")]
fn build(world: &mut World) {
    world.output = Some(
        world
            .command()
            .args(["build", "--target", "wasm-gc", "--sort-input"])
            .assert(),
    );
}
