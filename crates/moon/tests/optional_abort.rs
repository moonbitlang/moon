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

// Exercise discovery, planning, and lowering without requiring a compiler/core
// pair that has already removed abort. Artifact contents are irrelevant to dry runs.
#[test]
fn stdlib_abort_is_optional() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("moon.mod"),
        "name = \"test/optional_abort\"\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("moon.pkg"),
        "pkgtype(kind: \"executable\")\n",
    )
    .unwrap();
    std::fs::write(project.path().join("main.mbt"), "fn main { () }\n").unwrap();

    let core = tempfile::tempdir().unwrap();
    std::fs::write(
        core.path().join("moon.mod"),
        "name = \"moonbitlang/core\"\n",
    )
    .unwrap();
    std::fs::create_dir(core.path().join("prelude")).unwrap();
    std::fs::write(core.path().join("prelude/moon.pkg"), "").unwrap();
    let bundle = core.path().join("_build/wasm-gc/release/bundle");
    std::fs::create_dir_all(bundle.join("prelude")).unwrap();
    std::fs::write(bundle.join("prelude/prelude.mi"), "").unwrap();

    for state in [
        "absent",
        "stale",
        "present",
        "missing_artifact",
        "no_std",
        "overridden",
    ] {
        if state == "stale" {
            std::fs::create_dir_all(bundle.join("abort")).unwrap();
            std::fs::write(bundle.join("abort/abort.core"), "").unwrap();
        }
        if state == "present" {
            std::fs::create_dir(core.path().join("abort")).unwrap();
            std::fs::write(
                core.path().join("abort/moon.pkg"),
                "options(\"virtual\": { \"has-default\": true })\n",
            )
            .unwrap();
            std::fs::write(core.path().join("abort/pkg.mbti"), "").unwrap();
            std::fs::write(core.path().join("abort/abort.mbt"), "").unwrap();
        }
        if state == "missing_artifact" {
            std::fs::remove_file(bundle.join("abort/abort.core")).unwrap();
        }
        if state == "overridden" {
            std::fs::create_dir(project.path().join("replacement")).unwrap();
            std::fs::write(
                project.path().join("replacement/moon.pkg"),
                "options(implement: \"moonbitlang/core/abort\")\n",
            )
            .unwrap();
            std::fs::write(project.path().join("replacement/impl.mbt"), "").unwrap();
            std::fs::write(project.path().join("moon.pkg"), "pkgtype(kind: \"executable\")\noptions(overrides: [\"test/optional_abort/replacement\"])\n").unwrap();
        }
        let graph = project.path().join("graph.jsonl");
        if matches!(state, "absent" | "present") {
            snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
                .current_dir(core.path())
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
                state == "present"
            );
            let bundle_node = nodes
                .iter()
                .find(|node| {
                    node["command"]
                        .as_str()
                        .is_some_and(|cmd| cmd.contains(" bundle-core "))
                })
                .unwrap();
            assert!(
                !bundle_node["command"]
                    .as_str()
                    .unwrap()
                    .contains("abort/abort.core")
            );
        }
        let mut args = vec!["build", "--target", "wasm-gc", "--dry-run"];
        if state == "no_std" {
            args.push("--nostd");
        }
        snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
            .current_dir(project.path())
            .env("MOON_CORE_OVERRIDE", core.path())
            .env("MOON_DEP_CACHE", "off")
            .env("MOON_WORK", "off")
            .env("MOON_TEST_DUMP_BUILD_GRAPH", &graph)
            .args(args)
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
        assert_eq!(
            command.contains("abort/abort.core"),
            matches!(state, "present" | "missing_artifact"),
            "{state}: {command}"
        );
        assert_eq!(
            inputs
                .iter()
                .any(|input| input.as_str().unwrap().ends_with("abort/abort.core")),
            matches!(state, "present" | "missing_artifact"),
            "{state}: {inputs:?}"
        );
        // Reachable stdlib packages are linked from their own installed cores.
        assert!(!command.contains("core.core"), "{state}: {command}");
        assert_eq!(
            command.contains("bundle/prelude/prelude.core"),
            state != "no_std",
            "{state}: {command}"
        );
        assert_eq!(
            inputs
                .iter()
                .any(|input| input.as_str().unwrap().ends_with("prelude/prelude.core")),
            state != "no_std",
            "{state}: {inputs:?}"
        );
        if state == "overridden" {
            assert!(command.contains("replacement/replacement.core"));
        }
    }
}
