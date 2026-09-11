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

use super::fixture::{
    PlannedPackageRun, PlanningFixture, parse_bench_command, parse_build_command,
    parse_check_command, planned_check_package_runs, planned_root_package_runs,
};
use moonutil::target::TargetBackend;

fn expected_wasm_gc_packages(packages: &[&str]) -> Vec<PlannedPackageRun> {
    vec![PlannedPackageRun {
        target_backend: TargetBackend::WasmGC,
        packages: packages.iter().map(|pkg| (*pkg).to_string()).collect(),
    }]
}

fn expect_build_packages(fixture: &PlanningFixture, path: &str, expected: &[&str]) {
    let (cli, cmd) = parse_build_command(&[
        "build",
        path,
        "--target",
        "wasm-gc",
        "--dry-run",
        "--sort-input",
    ]);
    let runs = fixture
        .plan_build_all_with_cli(&cli, &cmd)
        .expect("build path filter should plan");
    assert_eq!(
        planned_root_package_runs(runs),
        expected_wasm_gc_packages(expected)
    );
}

fn expect_check_packages(fixture: &PlanningFixture, path: &str, expected: &[&str]) {
    let (cli, cmd) = parse_check_command(&[
        "check",
        path,
        "--target",
        "wasm-gc",
        "--dry-run",
        "--sort-input",
    ]);
    let runs = fixture
        .plan_check_all_with_cli(&cli, &cmd)
        .expect("check path filter should plan");
    assert_eq!(
        planned_check_package_runs(runs),
        expected_wasm_gc_packages(expected)
    );
}

fn expect_bench_packages(fixture: &PlanningFixture, path: &str, expected: &[&str]) {
    let (cli, cmd) = parse_bench_command(&[
        "bench",
        path,
        "--target",
        "wasm-gc",
        "--dry-run",
        "--sort-input",
    ]);
    let runs = fixture
        .plan_bench_all_with_cli(&cli, &cmd)
        .expect("bench path filter should plan");
    assert_eq!(
        planned_root_package_runs(runs),
        expected_wasm_gc_packages(expected)
    );
}

#[test]
fn build_path_spellings_select_the_same_root_package() {
    let fixture = PlanningFixture::new("test_filter/test_filter").expect("fixture should resolve");
    let case_dir = fixture.case_dir().display();

    for path in [
        format!("{case_dir}/A"),
        format!("{case_dir}/A/"),
        format!("{case_dir}/A/hello.mbt"),
    ] {
        expect_build_packages(&fixture, &path, &["username/hello/A"]);
    }

    expect_build_packages(
        &fixture,
        &format!("{case_dir}/lib"),
        &["username/hello/lib"],
    );
}

#[test]
fn check_path_spellings_select_the_same_root_package() {
    let fixture = PlanningFixture::new("test_filter/test_filter").expect("fixture should resolve");
    let case_dir = fixture.case_dir().display();

    for path in [
        format!("{case_dir}/A"),
        format!("{case_dir}/A/"),
        format!("{case_dir}/A/hello.mbt"),
    ] {
        expect_check_packages(&fixture, &path, &["username/hello/A"]);
    }

    expect_check_packages(
        &fixture,
        &format!("{case_dir}/lib"),
        &["username/hello/lib"],
    );
}

#[test]
fn bench_path_spellings_select_the_same_root_package() {
    let fixture = PlanningFixture::new("test_filter/test_filter").expect("fixture should resolve");
    let case_dir = fixture.case_dir().display();

    for path in [
        format!("{case_dir}/A"),
        format!("{case_dir}/A/"),
        format!("{case_dir}/A/hello.mbt"),
    ] {
        expect_bench_packages(&fixture, &path, &["username/hello/A"]);
    }

    expect_bench_packages(
        &fixture,
        &format!("{case_dir}/lib"),
        &["username/hello/lib"],
    );
}
