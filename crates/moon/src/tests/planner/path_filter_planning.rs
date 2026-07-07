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
    let (cli, cmd) = parse_build_command(&["build", path, "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_build_all_with_cli(&cli, &cmd)
        .expect("build path filter should plan");
    assert_eq!(
        planned_root_package_runs(runs),
        expected_wasm_gc_packages(expected)
    );
}

fn expect_check_packages(fixture: &PlanningFixture, path: &str, expected: &[&str]) {
    let (cli, cmd) = parse_check_command(&["check", path, "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_check_all_with_cli(&cli, &cmd)
        .expect("check path filter should plan");
    assert_eq!(
        planned_check_package_runs(runs),
        expected_wasm_gc_packages(expected)
    );
}

fn expect_bench_packages(fixture: &PlanningFixture, path: &str, expected: &[&str]) {
    let (cli, cmd) = parse_bench_command(&["bench", path, "--dry-run", "--sort-input"]);
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
