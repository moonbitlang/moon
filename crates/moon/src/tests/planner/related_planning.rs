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

use moonutil::common::TargetBackend;

use super::fixture::{
    PlannedPackageRun, PlanningFixture, parse_bench_command, parse_test_command,
    planned_root_package_runs,
};

fn affected_lib4_dependents() -> Vec<PlannedPackageRun> {
    vec![PlannedPackageRun {
        target_backend: TargetBackend::WasmGC,
        packages: vec![
            "username/hello/lib".to_string(),
            "username/hello/lib1".to_string(),
            "username/hello/lib2".to_string(),
            "username/hello/lib3".to_string(),
            "username/hello/lib4".to_string(),
            "username/hello/main".to_string(),
        ],
    }]
}

#[test]
fn test_related_selects_reverse_package_dependencies() {
    let fixture =
        PlanningFixture::new("test_filter/pkg_with_deps").expect("fixture should resolve");
    let related = fixture.case_dir().join("lib4/lib.mbt");
    let related = related.to_str().expect("fixture path should be UTF-8");
    let (cli, cmd) = parse_test_command(&["test", "--related", related, "--dry-run"]);

    let runs = fixture
        .plan_test_all_with_cli(&cli, &cmd)
        .expect("related test plans should resolve");

    assert_eq!(planned_root_package_runs(runs), affected_lib4_dependents());
}

#[test]
fn bench_related_selects_reverse_package_dependencies() {
    let fixture =
        PlanningFixture::new("test_filter/pkg_with_deps").expect("fixture should resolve");
    let related = fixture.case_dir().join("lib4/lib.mbt");
    let related = related.to_str().expect("fixture path should be UTF-8");
    let (cli, cmd) = parse_bench_command(&["bench", "--related", related, "--dry-run"]);

    let runs = fixture
        .plan_bench_all_with_cli(&cli, &cmd)
        .expect("related bench plans should resolve");

    assert_eq!(planned_root_package_runs(runs), affected_lib4_dependents());
}

#[test]
fn test_related_test_file_selects_only_owning_package() {
    let fixture = PlanningFixture::new("test_filter/test_filter").expect("fixture should resolve");
    let related = fixture.case_dir().join("A/hello_wbtest.mbt");
    let related = related.to_str().expect("fixture path should be UTF-8");
    let (cli, cmd) = parse_test_command(&["test", "--related", related, "--dry-run"]);

    let runs = fixture
        .plan_test_all_with_cli(&cli, &cmd)
        .expect("related test file plans should resolve");

    assert_eq!(
        planned_root_package_runs(runs),
        vec![PlannedPackageRun {
            target_backend: TargetBackend::WasmGC,
            packages: vec!["username/hello/A".to_string()],
        }]
    );
}
