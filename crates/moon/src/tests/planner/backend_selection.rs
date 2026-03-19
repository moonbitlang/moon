// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
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
