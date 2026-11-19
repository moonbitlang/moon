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

use std::{collections::BTreeMap, path::Path};

use cucumber::{gherkin::Step, then, when};

use super::World;

#[when(expr = "I preview moon {string}")]
fn preview(world: &mut World, arguments: String) {
    // Feature arguments use simple whitespace-separated words, not shell scripts.
    world.output = Some(
        world
            .command()
            .args(arguments.split_whitespace())
            .args(["--dry-run", "--sort-input"])
            .assert(),
    );
}

pub(super) fn commands(world: &World) -> Vec<Vec<String>> {
    let output = world.output.as_ref().expect("When a command is previewed");
    std::str::from_utf8(&output.get_output().stdout)
        .expect("UTF-8 preview")
        .lines()
        // Moon's PathNormalizer renders Unix quoting on every platform.
        .map(|line| shlex::split(line).expect("valid dry-run command quoting"))
        .collect()
}

pub(super) fn option<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

/// Partition every .mbt input by its command-line role. A generated source is
/// recognized only by an actual driver action's output, not an _build prefix.
pub(super) fn source_inputs(
    args: &[String],
    drivers: &BTreeMap<String, String>,
    project: &Path,
) -> [Vec<String>; 3] {
    let canonical_project = dunce::canonicalize(project).expect("project exists");
    let mut inputs: [Vec<String>; 3] = Default::default();
    for (index, arg) in args
        .iter()
        .enumerate()
        .filter(|(_, arg)| arg.ends_with(".mbt"))
    {
        let previous = index.checked_sub(1).map(|i| args[i].as_str());
        if previous == Some("--output-driver") {
            continue;
        }
        if let Some(kind) = drivers.get(arg) {
            inputs[2].push(kind.clone());
            continue;
        }
        let path = dunce::simplified(Path::new(arg));
        let path = path
            .strip_prefix(&canonical_project)
            .or_else(|_| path.strip_prefix(dunce::simplified(project)))
            .unwrap_or(path);
        let path = path.strip_prefix(".").unwrap_or(path);
        let path = path.to_string_lossy().replace('\\', "/");
        let doctest = matches!(previous, Some("-doctest-only" | "--doctest-only"));
        inputs[usize::from(doctest)].push(path);
    }
    for files in &mut inputs {
        files.sort();
    }
    inputs
}

#[then(expr = "the {string} commands use exactly these source inputs:")]
fn compiler_inputs(world: &mut World, kind: String, step: &Step) {
    let commands = commands(world);
    let drivers = commands
        .iter()
        .filter(|args| args.get(1).is_some_and(|arg| arg == "generate-test-driver"))
        .map(|args| {
            (
                option(args, "--output-driver")
                    .expect("driver output")
                    .to_owned(),
                format!(
                    "{}:{}",
                    option(args, "--pkg-name").expect("driver package"),
                    option(args, "--driver-kind").expect("driver kind")
                ),
            )
        })
        .collect();
    let mut actual = commands
        .iter()
        .filter(|args| args.get(1) == Some(&kind))
        .map(|args| {
            let package = option(args, "-pkg")
                .or_else(|| option(args, "--pkg-name"))
                .expect("package of compiler or driver command");
            let role = option(args, "--driver-kind").unwrap_or("");
            (
                package.to_owned(),
                role.to_owned(),
                source_inputs(args, &drivers, world.project_path()),
            )
        })
        .collect::<Vec<_>>();
    // Compare the complete collection for this action kind, including duplicate
    // commands. Table order is explanatory; source input order is not the contract.
    let mut expected = step.table.as_ref().expect("source input table").rows[1..]
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
                row[5]
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            ];
            for files in &mut inputs {
                files.sort();
            }
            (row[0].clone(), row[1].clone(), inputs)
        })
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected, "{kind} source inputs");
}

#[then(expr = "the preview has {int} {string} commands")]
fn command_count(world: &mut World, count: usize, kind: String) {
    assert_eq!(
        commands(world)
            .iter()
            .filter(|args| args.get(1) == Some(&kind))
            .count(),
        count,
        "number of {kind} commands"
    );
}
