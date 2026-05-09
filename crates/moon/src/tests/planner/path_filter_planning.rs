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

use super::fixture::{PlanningFixture, parse_build_command, parse_check_command};
use moonbuild_rupes_recta::model::BuildPlanNode;
use moonutil::common::TargetBackend;

fn build_packages(
    runs: Vec<(crate::rr_build::BuildMeta, crate::rr_build::BuildInput)>,
) -> Vec<(TargetBackend, Vec<String>)> {
    runs.into_iter()
        .map(|(meta, _)| {
            let packages = meta
                .artifacts
                .keys()
                .filter_map(|node| node.extract_target().map(|target| target.package))
                .map(|pkg_id| {
                    meta.resolve_output
                        .pkg_dirs
                        .get_package(pkg_id)
                        .fqn
                        .to_string()
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            (meta.target_backend.into(), packages)
        })
        .collect()
}

fn check_packages(
    runs: Vec<(crate::rr_build::BuildMeta, crate::rr_build::BuildInput)>,
) -> Vec<(TargetBackend, Vec<String>)> {
    runs.into_iter()
        .map(|(meta, _)| {
            let packages = meta
                .artifacts
                .keys()
                .filter_map(|node| match node {
                    BuildPlanNode::Check(target) => Some(target.package),
                    _ => None,
                })
                .map(|pkg_id| {
                    meta.resolve_output
                        .pkg_dirs
                        .get_package(pkg_id)
                        .fqn
                        .to_string()
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            (meta.target_backend.into(), packages)
        })
        .collect()
}

fn expect_build_packages(fixture: &PlanningFixture, path: &str, expected: &[&str]) {
    let (cli, cmd) = parse_build_command(&["build", path, "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_build_all_with_cli(&cli, &cmd)
        .expect("build path filter should plan");
    assert_eq!(
        build_packages(runs),
        vec![(
            TargetBackend::WasmGC,
            expected.iter().map(|pkg| (*pkg).to_string()).collect()
        )]
    );
}

fn expect_check_packages(fixture: &PlanningFixture, path: &str, expected: &[&str]) {
    let (cli, cmd) = parse_check_command(&["check", path, "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_check_all_with_cli(&cli, &cmd)
        .expect("check path filter should plan");
    assert_eq!(
        check_packages(runs),
        vec![(
            TargetBackend::WasmGC,
            expected.iter().map(|pkg| (*pkg).to_string()).collect()
        )]
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
