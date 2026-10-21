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

use super::fixture::{PlanningFixture, parse_build_command, planned_requested_artifacts};

fn requested_build_artifacts(fixture: &PlanningFixture, args: &[&str]) -> Vec<String> {
    let (cli, cmd) = parse_build_command(args);
    fixture
        .plan_build_graph_with_cli(&cli, &cmd)
        .map(planned_requested_artifacts)
        .expect("build command should plan")
}

#[test]
fn native_backends_build_foreign_libraries_as_libraries() {
    let fixture = PlanningFixture::new("need_link.in").expect("fixture should resolve");

    for target in ["native", "llvm"] {
        assert_eq!(
            requested_build_artifacts(
                &fixture,
                &["build", "--dry-run", "--target", target, "--nostd"]
            ),
            [
                "username/hello/lib foreign library",
                "username/hello/main executable",
            ],
            "unexpected artifacts for {target}"
        );
    }

    // Wasm and JS still request `Executable` for a foreign library; see the
    // FIXME in `UserIntent::append_artifacts`.
    assert_eq!(
        requested_build_artifacts(
            &fixture,
            &["build", "--dry-run", "--target", "wasm-gc", "--nostd"]
        ),
        [
            "username/hello/lib executable",
            "username/hello/main executable",
        ],
    );
}
