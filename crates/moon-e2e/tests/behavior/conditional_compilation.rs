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

use std::{collections::BTreeMap, fs, path::Path};

use cucumber::{gherkin::Step, given, then, when};
use serde_json::Value;

use super::{World, preview};

#[given("a project with backend and profile conditions on library source files")]
fn conditional_project(world: &mut World) {
    world.copy_case("cond_comp.in");
}

#[given("a project declaring the module root as its source directory")]
fn dot_source(world: &mut World) {
    world.copy_case("test_dot_source.in");
}

#[when(expr = "I check the conditional project for {word}")]
fn check(world: &mut World, backend: String) {
    world.output = Some(
        world
            .command()
            .args(["check", "--target", &backend, "--sort-input"])
            .assert(),
    );
}

#[when(expr = "I read the package index at {string}")]
fn read_index(world: &mut World, path: String) {
    world.package_index = Some(
        serde_json::from_slice(
            &fs::read(world.project_path().join(path)).expect("package index exists"),
        )
        .expect("valid package index JSON"),
    );
}

#[then(expr = "the index describes backend {string} and profile {string}")]
fn index_configuration(world: &mut World, backend: String, profile: String) {
    let index = world
        .package_index
        .as_ref()
        .expect("When the index is read");
    assert_eq!(index["backend"], backend);
    assert_eq!(index["opt_level"], profile);
}

#[then("the index retains exactly these source files and conditions:")]
fn index_conditions(world: &mut World, step: &Step) {
    let index = world
        .package_index
        .as_ref()
        .expect("When the index is read");
    let mut actual = Vec::new();
    for package in index["packages"].as_array().expect("index packages") {
        let name = package["rel"].as_str().expect("relative package name");
        for category in ["files", "test-files", "wbtest-files"] {
            for (path, conditions) in package[category].as_object().expect("source category") {
                let file = Path::new(path).file_name().unwrap().to_str().unwrap();
                let values = |key: &str| {
                    let mut values = conditions[key]
                        .as_array()
                        .expect("condition values")
                        .iter()
                        .map(|v| v.as_str().unwrap())
                        .collect::<Vec<_>>();
                    values.sort();
                    values.join(" ")
                };
                actual.push(vec![
                    name.to_owned(),
                    category.to_owned(),
                    file.to_owned(),
                    values("backend"),
                    values("optlevel"),
                ]);
            }
        }
    }
    let mut expected = step.table.as_ref().expect("file conditions table").rows[1..].to_vec();
    for row in &mut expected {
        for cell in &mut row[3..] {
            let mut values = cell.split_whitespace().collect::<Vec<_>>();
            values.sort();
            *cell = values.join(" ");
        }
    }
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "all ordinary, blackbox and whitebox source metadata"
    );
}

#[then("the index check commands use exactly these source inputs:")]
fn index_commands(world: &mut World, step: &Step) {
    let index = world
        .package_index
        .as_ref()
        .expect("When the index is read");
    let mut actual = Vec::new();
    for package in index["packages"].as_array().expect("index packages") {
        for field in [
            "check-command",
            "test-check-command",
            "wbtest-check-command",
        ] {
            if package[field] == Value::Null {
                continue;
            }
            let args = package[field]
                .as_array()
                .expect("check command argv")
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            actual.push((
                package["rel"].as_str().unwrap().to_owned(),
                field.to_owned(),
                preview::source_inputs(&args, &BTreeMap::new(), world.project_path()),
            ));
        }
    }
    let mut expected = step
        .table
        .as_ref()
        .expect("index command inputs table")
        .rows[1..]
        .iter()
        .map(|row| {
            let mut inputs = [
                row[3]
                    .split_whitespace()
                    .map(|name| format!("{}/{name}", row[2]))
                    .collect::<Vec<_>>(),
                row[4]
                    .split_whitespace()
                    .map(|name| format!("{}/{name}", row[2]))
                    .collect::<Vec<_>>(),
                Vec::new(),
            ];
            for files in &mut inputs {
                files.sort();
            }
            (row[0].clone(), row[1].clone(), inputs)
        })
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected, "backend-filtered check command inputs");
}
