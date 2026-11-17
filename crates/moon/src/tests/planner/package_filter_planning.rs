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

use std::collections::BTreeSet;

use super::fixture::{GraphAction, PlanningFixture, graph_actions, parse_test_command};

const TEST_ROOT: &str = "./_build/wasm-gc/debug/test";

fn plan_test(args: &[&str]) -> Vec<GraphAction> {
    let fixture = PlanningFixture::with_core_dependency("test_filter/test_filter")
        .expect("fixture should resolve");
    let (cli, cmd) = parse_test_command(args);
    graph_actions(
        &fixture
            .plan_test_with_cli(&cli, &cmd)
            .expect("planner should build the test graph"),
    )
}

fn produced_test_files(actions: &[GraphAction]) -> BTreeSet<String> {
    actions
        .iter()
        .flat_map(|action| &action.outputs)
        .filter_map(|path| path.strip_prefix(&format!("{TEST_ROOT}/")))
        .map(str::to_owned)
        .collect()
}

fn action_producing<'a>(actions: &'a [GraphAction], output: &str) -> &'a GraphAction {
    actions
        .iter()
        .find(|action| action.outputs.iter().any(|path| path == output))
        .unwrap_or_else(|| panic!("no planner action produces {output}"))
}

fn assert_test_target_chain(actions: &[GraphAction], package: &str, kind: &str) {
    let directory = format!("{TEST_ROOT}/{package}/");
    let driver = format!("{directory}__generated_driver_for_{kind}_test.mbt");
    let test_core = format!("{directory}{package}.{kind}_test.core");
    let test_wasm = format!("{directory}{package}.{kind}_test.wasm");

    // Each selected test kind has a generated driver, its compile action, and a linked artifact.
    let generator = action_producing(actions, &driver);
    assert!(generator.command.starts_with("moon generate-test-driver "));
    let args = shlex::split(&generator.command).unwrap();
    for (flag, value) in [
        ("--pkg-name", format!("username/hello/{package}")),
        ("--driver-kind", kind.to_owned()),
    ] {
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == flag && pair[1] == value),
            "{}",
            generator.command
        );
    }
    assert!(
        action_producing(actions, &test_core)
            .inputs
            .contains(&driver)
    );
    assert!(
        action_producing(actions, &test_wasm)
            .inputs
            .contains(&test_core)
    );
}

#[test]
fn selecting_a_keeps_its_source_and_three_test_kinds_only() {
    // Given a package selector for A in a module with unrelated sibling packages.
    let actions = plan_test(&[
        "test",
        "--target",
        "wasm-gc",
        "-p",
        "username/hello/A",
        "--sort-input",
        "--dry-run",
    ]);

    // Then A's source and all of its declared test kinds have complete output chains.
    let expected_files = BTreeSet::from([
        "A/A.mi".to_owned(),
        "A/A.core".to_owned(),
        "A/__generated_driver_for_internal_test.mbt".to_owned(),
        "A/__internal_test_info.json".to_owned(),
        "A/A.internal_test.core".to_owned(),
        "A/A.internal_test.wasm".to_owned(),
        "A/__generated_driver_for_whitebox_test.mbt".to_owned(),
        "A/__whitebox_test_info.json".to_owned(),
        "A/A.whitebox_test.core".to_owned(),
        "A/A.whitebox_test.wasm".to_owned(),
        "A/__generated_driver_for_blackbox_test.mbt".to_owned(),
        "A/__blackbox_test_info.json".to_owned(),
        "A/A.blackbox_test.core".to_owned(),
        "A/A.blackbox_test.wasm".to_owned(),
    ]);
    assert_eq!(produced_test_files(&actions), expected_files);
    for kind in ["internal", "whitebox", "blackbox"] {
        assert_test_target_chain(&actions, "A", kind);
    }

    // A's source files are inputs; no sibling package source is part of this filtered plan.
    let source_files: BTreeSet<_> = actions
        .iter()
        .flat_map(|action| &action.inputs)
        .filter(|path| {
            path.starts_with("./") && !path.starts_with("./_build/") && path.ends_with(".mbt")
        })
        .cloned()
        .collect();
    assert_eq!(
        source_files,
        BTreeSet::from([
            "./A/hello.mbt".to_owned(),
            "./A/test.mbt".to_owned(),
            "./A/hello_wbtest.mbt".to_owned(),
        ])
    );
}

#[test]
fn selecting_main_builds_its_lib_dependency_without_running_lib_tests() {
    // Given a selector for main, whose source imports lib, while A and lib2 are siblings.
    let actions = plan_test(&[
        "test",
        "--target",
        "wasm-gc",
        "-p",
        "username/hello/main",
        "--sort-input",
        "--dry-run",
    ]);

    // Then main's own inline and blackbox tests are built along with its source.
    let expected_files = BTreeSet::from([
        "main/main.mi".to_owned(),
        "main/main.core".to_owned(),
        "main/__generated_driver_for_internal_test.mbt".to_owned(),
        "main/__internal_test_info.json".to_owned(),
        "main/main.internal_test.core".to_owned(),
        "main/main.internal_test.wasm".to_owned(),
        "main/__generated_driver_for_blackbox_test.mbt".to_owned(),
        "main/__blackbox_test_info.json".to_owned(),
        "main/main.blackbox_test.core".to_owned(),
        "main/main.blackbox_test.wasm".to_owned(),
        "lib/lib.mi".to_owned(),
        "lib/lib.core".to_owned(),
    ]);
    assert_eq!(produced_test_files(&actions), expected_files);
    assert_test_target_chain(&actions, "main", "internal");
    assert_test_target_chain(&actions, "main", "blackbox");

    // The dependency's interface is an actual main compile input; its test targets stay unselected.
    let main_source = action_producing(&actions, "./_build/wasm-gc/debug/test/main/main.core");
    assert!(
        main_source
            .inputs
            .contains(&"./_build/wasm-gc/debug/test/lib/lib.mi".to_owned())
    );
    let source_files: BTreeSet<_> = actions
        .iter()
        .flat_map(|action| &action.inputs)
        .filter(|path| {
            path.starts_with("./") && !path.starts_with("./_build/") && path.ends_with(".mbt")
        })
        .cloned()
        .collect();
    assert_eq!(
        source_files,
        BTreeSet::from(["./lib/hello.mbt".to_owned(), "./main/main.mbt".to_owned(),])
    );
}

#[test]
fn without_a_filter_each_module_package_gets_its_test_kinds() {
    // Given the same module without a package selector.
    let actions = plan_test(&["test", "--target", "wasm-gc", "--sort-input", "--dry-run"]);

    // Then the module includes each source package and only the test kinds it defines.
    let packages = [
        ("A", &["internal", "whitebox", "blackbox"][..]),
        ("lib", &["internal", "whitebox", "blackbox"][..]),
        ("lib2", &["internal", "blackbox"][..]),
        ("main", &["internal", "blackbox"][..]),
    ];
    let mut expected_files = BTreeSet::new();
    for (package, kinds) in packages {
        expected_files.insert(format!("{package}/{package}.mi"));
        expected_files.insert(format!("{package}/{package}.core"));
        for kind in kinds {
            expected_files.insert(format!("{package}/__generated_driver_for_{kind}_test.mbt"));
            expected_files.insert(format!("{package}/__{kind}_test_info.json"));
            expected_files.insert(format!("{package}/{package}.{kind}_test.core"));
            expected_files.insert(format!("{package}/{package}.{kind}_test.wasm"));
            assert_test_target_chain(&actions, package, kind);
        }
    }
    assert_eq!(produced_test_files(&actions), expected_files);

    // Main consumes lib's interface; this unfiltered plan also includes lib's own tests.
    let main_source = action_producing(&actions, "./_build/wasm-gc/debug/test/main/main.core");
    assert!(
        main_source
            .inputs
            .contains(&"./_build/wasm-gc/debug/test/lib/lib.mi".to_owned())
    );
}
