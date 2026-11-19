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

use cucumber::{gherkin::Step, then};
use snapbox::IntoData;

use super::World;

#[then("the command succeeds")]
fn succeeds(world: &mut World) {
    world.output = Some(
        world
            .output
            .take()
            .expect("When a command is run")
            .success(),
    );
}

#[then(expr = "the command exits with status {int}")]
fn exits_with_status(world: &mut World, status: i32) {
    world.output = Some(
        world
            .output
            .take()
            .expect("When a command is run")
            .code(status),
    );
}

#[then("stdout is empty")]
fn stdout_is_empty(world: &mut World) {
    world.output = Some(
        world
            .output
            .take()
            .expect("When a command is run")
            .stdout_eq(""),
    );
}

#[then("stderr is empty")]
fn stderr_is_empty(world: &mut World) {
    world.output = Some(
        world
            .output
            .take()
            .expect("When a command is run")
            .stderr_eq(""),
    );
}

#[then("stdout is exactly:")]
fn stdout_is_exactly(world: &mut World, step: &Step) {
    world.output = Some(
        world
            .output
            .take()
            .expect("When a command is run")
            .stdout_eq(output_text(step).raw()),
    );
}

#[then("stderr is exactly:")]
fn stderr_is_exactly(world: &mut World, step: &Step) {
    world.output = Some(
        world
            .output
            .take()
            .expect("When a command is run")
            .stderr_eq(output_text(step).raw()),
    );
}

#[then("stderr matches:")]
fn stderr_matches(world: &mut World, step: &Step) {
    world.output = Some(
        world
            .output
            .take()
            .expect("When a command is run")
            .stderr_eq(output_text(step)),
    );
}

// Gherkin wraps docstrings in newlines. Remove only the opening one: the
// closing newline represents the command's final newline, and blank lines
// inside the docstring remain significant. Every expected byte comes from it.
fn output_text(step: &Step) -> &str {
    let text = step.docstring.as_deref().expect("Then an output docstring");
    text.strip_prefix('\n').unwrap_or(text)
}
