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

use super::fixture::{PlanningFixture, parse_bench_command, parse_test_command};

fn assert_contains_and_absent(output: &str, present: &[&str], absent: &[&str]) {
    for needle in present {
        assert!(
            output.contains(needle),
            "expected output to contain `{needle}`, got:\n{output}"
        );
    }
    for needle in absent {
        assert!(
            !output.contains(needle),
            "expected output to not contain `{needle}`, got:\n{output}"
        );
    }
}

#[test]
fn mixed_backend_test_planning_is_target_aware() {
    let fixture =
        PlanningFixture::new("mixed_backend_local_dep.in").expect("fixture should resolve");
    let (cli, cmd) = parse_test_command(&["test", "--target", "js", "--dry-run", "--sort-input"]);

    let test_js = fixture
        .plan_test_with_cli(&cli, &cmd)
        .expect("js test graph should plan");
    assert_contains_and_absent(
        &test_js,
        &["./web/web_wbtest.mbt", "./deps/jsdep/lib/lib.mbt"],
        &["./server/server_wbtest.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let (cli, cmd) =
        parse_test_command(&["test", "--target", "native", "--dry-run", "--sort-input"]);
    let test_native = fixture
        .plan_test_with_cli(&cli, &cmd)
        .expect("native test graph should plan");
    assert_contains_and_absent(
        &test_native,
        &["./server/server_wbtest.mbt", "./deps/nativedep/lib/lib.mbt"],
        &["./web/web_wbtest.mbt", "./deps/jsdep/lib/lib.mbt"],
    );
}

#[test]
fn mixed_backend_bench_planning_is_target_aware() {
    let fixture =
        PlanningFixture::new("mixed_backend_local_dep.in").expect("fixture should resolve");
    let (cli, cmd) = parse_bench_command(&["bench", "--target", "js", "--dry-run", "--sort-input"]);

    let bench_js = fixture
        .plan_bench_with_cli(&cli, &cmd)
        .expect("js bench graph should plan");
    assert_contains_and_absent(
        &bench_js,
        &["./web/web_wbtest.mbt", "./deps/jsdep/lib/lib.mbt"],
        &["./server/server_wbtest.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let (cli, cmd) =
        parse_bench_command(&["bench", "--target", "native", "--dry-run", "--sort-input"]);
    let bench_native = fixture
        .plan_bench_with_cli(&cli, &cmd)
        .expect("native bench graph should plan");
    assert_contains_and_absent(
        &bench_native,
        &["./server/server_wbtest.mbt", "./deps/nativedep/lib/lib.mbt"],
        &["./web/web_wbtest.mbt", "./deps/jsdep/lib/lib.mbt"],
    );
}

#[test]
fn mixed_backend_explicit_test_selection_rejects_unsupported_backend() {
    let fixture =
        PlanningFixture::new("mixed_backend_local_dep.in").expect("fixture should resolve");
    let (cli, test_cmd) = parse_test_command(&[
        "test",
        "--package",
        "mixed/localdep/server",
        "--target",
        "js",
        "--dry-run",
    ]);
    let test_err = format!(
        "{:?}",
        fixture
            .plan_test_with_cli(&cli, &test_cmd)
            .expect_err("unsupported backend should fail test planning")
    );
    assert!(test_err.contains("Selected package(s) do not support target backend 'js'"));
    assert!(test_err.contains("mixed/localdep/server ([native])"));
}

#[test]
fn explicit_test_and_bench_selectors_are_validated_before_import_resolution() {
    let fixture = PlanningFixture::new("test_selection_before_resolution.in")
        .expect("invalid imports should not prevent discovery");
    let native_path = fixture.case_dir().join("native");
    let native_path = native_path.to_str().expect("fixture path should be UTF-8");

    for command in ["test", "bench"] {
        for (selector, expected_error) in [
            (vec!["--target", "native"], "selection/errors/missing"),
            (
                vec!["--target", "js", "--package", "selection/errors/native"],
                "Selected package(s) do not support target backend 'js'",
            ),
            (
                vec!["--target", "js", native_path],
                "Selected package(s) do not support target backend 'js'",
            ),
            (
                vec![
                    "--target",
                    "native",
                    "--package",
                    "selection/errors",
                    "--file",
                    "test.mbt",
                ],
                "Cannot filter by file or index when multiple packages are specified",
            ),
        ] {
            let args = [vec![command, "--dry-run"], selector].concat();
            let error = if command == "test" {
                let (cli, cmd) = parse_test_command(&args);
                fixture.plan_test_all_with_cli(&cli, &cmd).err()
            } else {
                let (cli, cmd) = parse_bench_command(&args);
                fixture.plan_bench_all_with_cli(&cli, &cmd).err()
            }
            .expect("invalid selection or imports should fail planning");
            let error = format!("{error:#}");
            assert!(error.contains(expected_error), "{args:?}: {error}");
        }
    }
}
