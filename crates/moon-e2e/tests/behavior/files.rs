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

use cucumber::{gherkin::Step, given, then};
use snapbox::IntoData;

use super::World;

#[given(expr = "the file {string} is exactly:")]
#[then(expr = "the file {string} is exactly:")]
fn file_contents(world: &mut World, path: String, step: &Step) {
    let actual = std::fs::read_to_string(world.project_path().join(&path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"));
    let expected = step.docstring.as_deref().expect("file content docstring");
    // Remove Gherkin's framing newlines, preserving intentional file newlines.
    let expected = expected.strip_prefix('\n').unwrap_or(expected);
    let expected = expected.strip_suffix('\n').unwrap_or(expected);
    snapbox::assert_data_eq!(actual, expected.raw());
}
