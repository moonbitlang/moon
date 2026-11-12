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

// Exercise ordinary stdlib packages through discovery, bundling, and linking.
// Artifact contents are irrelevant to dry runs.
use moon_test_util::Fixture;

#[test]
fn stdlib_abort_uses_normal_build_pipeline() {
    let project = Fixture::new().with_files([
        ("moon.mod", "name = \"test/stdlib_abort\"\n"),
        ("moon.pkg", "pkgtype(kind: \"executable\")\n"),
        ("main.mbt", "fn main { () }\n"),
    ]);

    let core = Fixture::new().with_files([
        ("moon.mod", "name = \"moonbitlang/core\"\n"),
        ("prelude/moon.pkg", ""),
        ("_build/wasm-gc/release/bundle/prelude/prelude.mi", ""),
    ]);
    let bundle = core.join("_build/wasm-gc/release/bundle");

    for state in ["absent", "stale", "bundled", "missing_artifact"] {
        if state == "stale" {
            std::fs::create_dir_all(bundle.join("abort")).unwrap();
            std::fs::write(bundle.join("abort/abort.core"), "").unwrap();
        }
        if state == "bundled" {
            std::fs::create_dir(core.join("abort")).unwrap();
            std::fs::write(core.join("abort/moon.pkg"), "").unwrap();
            std::fs::write(core.join("abort/abort.mbt"), "").unwrap();
        }
        if state == "missing_artifact" {
            std::fs::remove_file(bundle.join("abort/abort.core")).unwrap();
        }
        let graph = project.join("graph.jsonl");
        if matches!(state, "absent" | "bundled") {
            core.moon_cmd(snapbox::cargo_bin!("moon"))
                .env("MOON_DEP_CACHE", "off")
                .env("MOON_WORK", "off")
                .env("MOON_TEST_DUMP_BUILD_GRAPH", &graph)
                .args(["bundle", "--target", "wasm-gc", "--dry-run"])
                .assert()
                .success();
            let nodes: Vec<serde_json::Value> = std::fs::read_to_string(&graph)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(
                nodes.iter().any(|node| node["outputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|output| output.as_str().unwrap().ends_with("abort/abort.core"))),
                state != "absent"
            );
            // Toolchain packaging needs the individual package artifacts only.
            for node in &nodes {
                assert!(
                    !node["command"]
                        .as_str()
                        .is_some_and(|cmd| cmd.contains(" bundle-core "))
                );
                assert!(
                    !node["outputs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|output| { output.as_str().unwrap().ends_with("bundle/core.core") })
                );
            }
        }
        project
            .moon_cmd(snapbox::cargo_bin!("moon"))
            .env("MOON_CORE_OVERRIDE", core.cwd())
            .env("MOON_DEP_CACHE", "off")
            .env("MOON_WORK", "off")
            .env("MOON_TEST_DUMP_BUILD_GRAPH", &graph)
            .args(["build", "--target", "wasm-gc", "--dry-run"])
            .assert()
            .success();
        let nodes: Vec<serde_json::Value> = std::fs::read_to_string(&graph)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let link = nodes
            .iter()
            .find(|node| {
                node["command"]
                    .as_str()
                    .is_some_and(|cmd| cmd.contains(" link-core "))
            })
            .unwrap();
        let command = link["command"].as_str().unwrap();
        let inputs = link["inputs"].as_array().unwrap();
        assert!(!command.contains("abort/abort.core"), "{state}: {command}");
        assert!(
            !inputs
                .iter()
                .any(|input| input.as_str().unwrap().ends_with("abort/abort.core")),
            "{state}: {inputs:?}"
        );
        // Reachable stdlib packages are linked from their own installed cores.
        assert!(!command.contains("core.core"), "{state}: {command}");
        assert!(
            command.contains("bundle/prelude/prelude.core"),
            "{state}: {command}"
        );
        assert!(
            inputs
                .iter()
                .any(|input| input.as_str().unwrap().ends_with("prelude/prelude.core")),
            "{state}: {inputs:?}"
        );
    }

    // Core's abort package can be tested and instrumented like any other package.
    std::fs::create_dir(core.join("coverage")).unwrap();
    std::fs::write(core.join("coverage/moon.pkg"), "").unwrap();
    let graph = project.join("test-graph.jsonl");
    core.moon_cmd(snapbox::cargo_bin!("moon"))
        .env("MOON_DEP_CACHE", "off")
        .env("MOON_WORK", "off")
        .env("MOON_TEST_DUMP_BUILD_GRAPH", &graph)
        .args([
            "test",
            "--package",
            "moonbitlang/core/abort",
            "--target",
            "wasm-gc",
            "--enable-coverage",
            "--dry-run",
        ])
        .assert()
        .success();
    let nodes: Vec<serde_json::Value> = std::fs::read_to_string(graph)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let abort_test = nodes
        .iter()
        .find(|node| {
            node["outputs"].as_array().unwrap().iter().any(|output| {
                output
                    .as_str()
                    .unwrap()
                    .ends_with("abort/abort.internal_test.core")
            })
        })
        .unwrap();
    assert!(
        abort_test["command"]
            .as_str()
            .unwrap()
            .contains(" -enable-coverage")
    );
    assert!(
        abort_test["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|input| input.as_str().unwrap().ends_with("coverage/coverage.mi"))
    );
}
