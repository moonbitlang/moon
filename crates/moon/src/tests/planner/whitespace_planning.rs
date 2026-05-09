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

use super::fixture::{PlanningFixture, parse_build_command, parse_run_command};
use moonutil::{common::TargetBackend, cond_expr::OptLevel};

#[derive(Debug, PartialEq, Eq)]
struct PlannedCliIntent {
    target_backend: TargetBackend,
    profile: OptLevel,
    packages: Vec<String>,
}

fn planned_cli_intent(
    (meta, _): (crate::rr_build::BuildMeta, crate::rr_build::BuildInput),
) -> PlannedCliIntent {
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

    PlannedCliIntent {
        target_backend: meta.target_backend.into(),
        profile: meta.opt_level,
        packages,
    }
}

#[test]
fn whitespace_cli_variants_resolve_to_same_main_package_intention() {
    let fixture = PlanningFixture::new("whitespace_test.in").expect("fixture should resolve");
    let expected = PlannedCliIntent {
        target_backend: TargetBackend::WasmGC,
        profile: OptLevel::Debug,
        packages: vec!["username/hello/main exe".to_string()],
    };

    for args in [
        &["build", "--dry-run", "--nostd"][..],
        &["build", "--dry-run", "--debug", "--nostd"],
        &["build", "--dry-run", "--target", "wasm-gc", "--nostd"],
        &[
            "build",
            "--dry-run",
            "--target",
            "wasm-gc",
            "--debug",
            "--nostd",
        ],
    ] {
        let (cli, cmd) = parse_build_command(args);
        let actual = fixture
            .plan_build_meta_with_cli(&cli, &cmd)
            .map(planned_cli_intent)
            .expect("build command should resolve");
        assert_eq!(actual, expected, "unexpected build intention for {args:?}");
    }

    for args in [
        &["run", "main exe", "--dry-run", "--nostd"][..],
        &["run", "main exe", "--dry-run", "--debug", "--nostd"],
        &[
            "run",
            "main exe",
            "--dry-run",
            "--target",
            "wasm-gc",
            "--nostd",
        ],
        &[
            "run",
            "main exe",
            "--dry-run",
            "--target",
            "wasm-gc",
            "--debug",
            "--nostd",
        ],
    ] {
        let (cli, cmd) = parse_run_command(args);
        let actual = fixture
            .plan_run_meta_with_cli(&cli, &cmd)
            .map(planned_cli_intent)
            .expect("run command should resolve");
        assert_eq!(actual, expected, "unexpected run intention for {args:?}");
    }
}
