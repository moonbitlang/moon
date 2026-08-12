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

use expect_test::{expect, expect_file};

use crate::{
    build_graph::compare_graphs,
    registry_fixtures::third_party_registry,
    support::{TestDir, get_stdout_with_envs, snap_dry_run_graph_with_envs},
};

#[test]
fn test_third_party() {
    let registry = third_party_registry();
    let dir = TestDir::new("third_party");
    get_stdout_with_envs(&dir, ["update"], registry.envs());

    let graph = dir.join("test_dry_run.jsonl");
    snap_dry_run_graph_with_envs(
        &dir,
        ["test", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        &graph,
        registry.envs(),
    );
    compare_graphs(
        &graph,
        expect_file!["../test_cases/third_party/third_party_dry_run.jsonl"],
    );

    let stdout = get_stdout_with_envs(
        &dir,
        ["test", "--target", "wasm-gc", "--sort-input"],
        registry.envs(),
    );
    expect![[r#"
        Hello, world!
        Hello, world!
        Total tests: 2, passed: 2, failed: 0.
    "#]]
    .assert_eq(&stdout);
    registry.assert_used();
}
