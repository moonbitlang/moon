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

use super::fixture::{
    PlanningFixture, parse_build_command, parse_check_command, parse_run_command,
};

// Phase 3: these tests already know the selected backend and only need to
// verify that planning keeps the right packages and commands in the graph.

fn assert_contains_and_absent(graph: &str, present: &[&str], absent: &[&str]) {
    for needle in present {
        assert!(
            graph.contains(needle),
            "expected graph to contain `{needle}`, got:\n{graph}"
        );
    }
    for needle in absent {
        assert!(
            !graph.contains(needle),
            "expected graph to not contain `{needle}`, got:\n{graph}"
        );
    }
}

#[test]
fn target_backend_build_planning_respects_default_and_explicit_backend() {
    let fixture = PlanningFixture::new("target_backend").expect("fixture should resolve");

    let (cli, cmd) = parse_build_command(&["build", "--dry-run", "--nostd", "--sort-input"]);
    let default_graph = fixture
        .plan_build_with_cli(&cli, &cmd)
        .expect("default build graph should plan");
    assert_contains_and_absent(
        &default_graph,
        &[
            "./_build/wasm-gc/debug/build/main/main.wasm",
            "-target wasm-gc",
        ],
        &["./_build/js/debug/build/main/main.js", "-target js"],
    );

    let (cli, cmd) = parse_build_command(&[
        "build",
        "--dry-run",
        "--target",
        "js",
        "--nostd",
        "--sort-input",
    ]);
    let js_graph = fixture
        .plan_build_with_cli(&cli, &cmd)
        .expect("js build graph should plan");
    assert_contains_and_absent(
        &js_graph,
        &["./_build/js/debug/build/main/main.js", "-target js"],
        &[
            "./_build/wasm-gc/debug/build/main/main.wasm",
            "-target wasm-gc",
        ],
    );
}

#[test]
fn mixed_backend_build_and_check_planning_are_target_aware() {
    let fixture =
        PlanningFixture::new("mixed_backend_local_dep.in").expect("fixture should resolve");

    let (cli, cmd) = parse_check_command(&["check", "--target", "js", "--dry-run", "--sort-input"]);
    let check_js = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("js check graph should plan");
    assert_contains_and_absent(
        &check_js,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &[
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
            "./deps/unuseddep/lib/lib.mbt",
        ],
    );

    let (cli, cmd) = parse_build_command(&["build", "--target", "js", "--dry-run", "--sort-input"]);
    let build_js = fixture
        .plan_build_with_cli(&cli, &cmd)
        .expect("js build graph should plan");
    assert_contains_and_absent(
        &build_js,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &["./server/main.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let (cli, cmd) =
        parse_check_command(&["check", "--target", "native", "--dry-run", "--sort-input"]);
    let check_native = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("native check graph should plan");
    assert_contains_and_absent(
        &check_native,
        &[
            "./shared/shared.mbt",
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
        ],
        &[
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
            "./deps/unuseddep/lib/lib.mbt",
        ],
    );

    let (cli, cmd) =
        parse_build_command(&["build", "--target", "native", "--dry-run", "--sort-input"]);
    let build_native = fixture
        .plan_build_with_cli(&cli, &cmd)
        .expect("native build graph should plan");
    assert_contains_and_absent(
        &build_native,
        &[
            "./shared/shared.mbt",
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
        ],
        &["./web/main.mbt", "./deps/jsdep/lib/lib.mbt"],
    );
}

#[test]
fn check_planning_skips_incompatible_test_only_dependencies() {
    let fixture = PlanningFixture::new("check_skip_incompatible_test_import.in")
        .expect("fixture should resolve");

    let (cli, cmd) = parse_check_command(&["check", "--target", "js", "--dry-run", "--sort-input"]);
    let check_js = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("js check graph should plan");
    assert_contains_and_absent(
        &check_js,
        &[
            "./lib/lib.mbt",
            "./lib/lib_test.mbt",
            "./lib/lib_wbtest.mbt",
            "./testonly/dep.mbt",
            "./wbonly/dep.mbt",
        ],
        &[],
    );

    let (cli, cmd) =
        parse_check_command(&["check", "--target", "native", "--dry-run", "--sort-input"]);
    let check_native = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("native check graph should plan");
    assert_contains_and_absent(
        &check_native,
        &["./lib/lib.mbt"],
        &[
            "./lib/lib_test.mbt",
            "./lib/lib_wbtest.mbt",
            "./testonly/dep.mbt",
            "./wbonly/dep.mbt",
        ],
    );
}

#[test]
fn mixed_backend_run_planning_is_target_aware() {
    let fixture =
        PlanningFixture::new("mixed_backend_local_dep.in").expect("fixture should resolve");

    let (cli, cmd) =
        parse_run_command(&["run", "web", "--target", "js", "--dry-run", "--sort-input"]);
    let run_js = fixture
        .plan_run_with_cli(&cli, &cmd)
        .expect("js run graph should plan");
    assert_contains_and_absent(
        &run_js,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &["./server/main.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let (cli, cmd) = parse_run_command(&[
        "run",
        "server",
        "--target",
        "native",
        "--dry-run",
        "--sort-input",
    ]);
    let run_native = fixture
        .plan_run_with_cli(&cli, &cmd)
        .expect("native run graph should plan");
    assert_contains_and_absent(
        &run_native,
        &[
            "./shared/shared.mbt",
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
        ],
        &["./web/main.mbt", "./deps/jsdep/lib/lib.mbt"],
    );
}

#[test]
fn supported_targets_empty_packages_are_skipped_in_check_planning() {
    let fixture =
        PlanningFixture::new("supported_targets_empty.in").expect("fixture should resolve");

    let (cli, cmd) = parse_check_command(&["check", "--target", "js", "--dry-run", "--sort-input"]);
    let check_js = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("js check graph should plan");
    assert_contains_and_absent(
        &check_js,
        &["./main/main.mbt", "./lib/lib.mbt"],
        &["./never/never.mbt"],
    );

    let (cli, cmd) =
        parse_check_command(&["check", "--target", "native", "--dry-run", "--sort-input"]);
    let check_native = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("native check graph should plan");
    assert_contains_and_absent(
        &check_native,
        &["./main/main.mbt", "./lib/lib.mbt"],
        &["./never/never.mbt"],
    );
}

#[test]
fn module_supported_targets_intersection_filters_check_planning() {
    let fixture = PlanningFixture::new("supported_targets_module_intersection.in")
        .expect("fixture should resolve");
    let lib_path = fixture.case_dir().join("lib");
    let lib_path = lib_path.to_str().expect("fixture path should be UTF-8");

    let (cli, cmd) = parse_check_command(&[
        "check",
        lib_path,
        "--target",
        "wasm-gc",
        "--dry-run",
        "--sort-input",
    ]);
    let check_wasm_gc = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("wasm-gc check graph should plan");
    assert_contains_and_absent(&check_wasm_gc, &["./lib/lib.mbt"], &["./main/main.mbt"]);

    let (cli, cmd) = parse_check_command(&[
        "check",
        lib_path,
        "--target",
        "native",
        "--dry-run",
        "--sort-input",
    ]);
    let check_native = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("native check graph should plan");
    assert_contains_and_absent(&check_native, &["./lib/lib.mbt"], &["./main/main.mbt"]);

    let (cli, cmd) = parse_check_command(&[
        "check",
        lib_path,
        "--target",
        "llvm",
        "--dry-run",
        "--sort-input",
    ]);
    let check_llvm = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("llvm check graph should plan");
    assert_contains_and_absent(&check_llvm, &["./lib/lib.mbt"], &["./main/main.mbt"]);
}
