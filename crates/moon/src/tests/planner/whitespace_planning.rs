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
    PlannedPackageIntent, PlanningFixture, parse_build_command, parse_run_command,
    planned_root_package_intent,
};
use moonutil::{cond_expr::OptLevel, target::TargetBackend};

#[test]
fn whitespace_cli_variants_resolve_to_same_main_package_intention() {
    let fixture = PlanningFixture::new("whitespace_test.in").expect("fixture should resolve");
    let expected = PlannedPackageIntent {
        target_backend: TargetBackend::WasmGC,
        profile: OptLevel::Debug,
        packages: vec!["username/hello/main exe".to_string()],
    };

    for args in [
        &["build", "--dry-run", "--target", "wasm-gc", "--nostd"][..],
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
            .plan_build_graph_with_cli(&cli, &cmd)
            .map(planned_root_package_intent)
            .expect("build command should resolve");
        assert_eq!(actual, expected, "unexpected build intention for {args:?}");
    }

    for args in [
        &[
            "run",
            "main exe",
            "--dry-run",
            "--target",
            "wasm-gc",
            "--nostd",
        ][..],
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
            .plan_run_graph_with_cli(&cli, &cmd)
            .map(planned_root_package_intent)
            .expect("run command should resolve");
        assert_eq!(actual, expected, "unexpected run intention for {args:?}");
    }
}
