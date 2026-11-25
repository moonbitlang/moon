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

use cucumber::{gherkin::Step, given, when};
use moon_test_util::test_dir::TestDir;

use super::World;

#[given("an isolated workspace containing these files:")]
fn workspace_files(world: &mut World, step: &Step) {
    let workspace = TestDir::new_empty();
    let table = step.table.as_ref().expect("file table");
    assert_eq!(table.rows[0], ["path", "contents"]);
    for row in table.rows.iter().skip(1) {
        assert_eq!(row.len(), 2);
        let path = workspace.join(&row[0]);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &row[1]).unwrap();
    }
    world.cwd = Some(workspace.as_ref().to_owned());
    world._workspace = Some(workspace);
}

#[when("I compile and run this MoonBit script for Wasm:")]
fn run_script(world: &mut World, step: &Step) {
    let source = step.docstring.as_deref().expect("MoonBit script");
    let source = source.strip_prefix('\n').unwrap_or(source);
    world.output = Some(
        world
            .command()
            .args(["run", "--target", "wasm", "-"])
            .stdin(source)
            .assert(),
    );
}
