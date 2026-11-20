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

use super::World;

#[given("a project with one inline snapshot for each backend")]
fn backend_snapshots(world: &mut World) {
    world.copy_case("targets/auto_update");
}

#[when(expr = "I update inline snapshots for {word}")]
fn update_snapshots(world: &mut World, backend: String) {
    // Each update may change only its selected source. Capture the preceding
    // state so later backend switches must also preserve already-saved snapshots.
    world.source_files_before_update = ["wasm", "wasm-gc", "js", "native"]
        .into_iter()
        .map(|backend| {
            let path = format!("lib/x.{backend}.mbt");
            let content =
                fs::read(world.project_path().join(&path)).expect("read source before update");
            (path, content)
        })
        .collect();
    world.output = Some(
        world
            .command()
            .args(["test", "--target", &backend, "-u", "--no-parallelize"])
            .assert(),
    );
}

#[then("these source files are unchanged by the update:")]
fn unselected_sources(world: &mut World, step: &Step) {
    let table = step.table.as_ref().expect("source file table");
    assert_eq!(table.rows[0], ["source file"]);
    assert!(
        table.rows.len() > 1,
        "name at least one unchanged source file"
    );
    for row in table.rows.iter().skip(1) {
        assert_eq!(row.len(), 1, "one source file per row");
        let path = &row[0];
        let before = world
            .source_files_before_update
            .get(path)
            .expect("source was captured before the update");
        let after = fs::read(world.project_path().join(path)).expect("read source after update");
        assert_eq!(
            &after, before,
            "{path} changed during another backend's update"
        );
    }
}

#[when(expr = "I test the saved inline snapshots for {word}")]
fn test_saved_snapshots(world: &mut World, backend: String) {
    world.output = Some(
        world
            .command()
            .args(["test", "--target", &backend, "--no-parallelize"])
            .assert(),
    );
}
