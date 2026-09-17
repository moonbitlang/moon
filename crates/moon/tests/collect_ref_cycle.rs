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

use std::path::PathBuf;

use moon_test_util::test_dir::TestDir;

fn fixture(case: &str) -> TestDir {
    TestDir::from_case_root(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases"),
        case,
        true,
    )
}

fn moon(dir: &TestDir) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(dir)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .env_remove("MOON_COLLECT_REF_CYCLE")
        .env_remove("MOON_WASM_NEW_ALLOCATOR")
        .env_remove("MOONBIT_ALLOCATOR")
        .env_remove("MOON_CC")
        .env("MOONBIT_NEW_NATIVE", "1")
}

fn commands(dir: &TestDir, command: snapbox::cmd::Command) -> Vec<Vec<String>> {
    let graph = dir.join("graph.jsonl");
    command
        .arg("--dry-run")
        .env("MOON_TEST_DUMP_BUILD_GRAPH", &graph)
        .assert()
        .success();
    std::fs::read_to_string(graph)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter_map(|node| node["command"].as_str().map(str::to_owned))
        // Build graph dumps use Unix quoting on every platform.
        .map(|command| shlex::split(&command).expect("invalid build graph command"))
        .collect()
}

fn assert_collection_flags(commands: &[Vec<String>], backend: &str, enabled: bool) {
    let mut links = 0;
    let mut c_compiles = 0;
    for args in commands {
        let is_link_core = args.get(1).is_some_and(|arg| arg == "link-core");
        let is_c_compile = !is_link_core && args.iter().any(|arg| arg.ends_with(".c"));
        links += usize::from(is_link_core);
        c_compiles += usize::from(is_c_compile);
        assert_eq!(
            args.iter().any(|arg| arg == "-enable-trial-deletion"),
            enabled && backend == "wasm" && is_link_core,
            "{args:?}"
        );
        assert_eq!(
            args.iter().any(|arg| matches!(
                arg.as_str(),
                "-DMOONBIT_TRIAL_DELETION=1" | "/DMOONBIT_TRIAL_DELETION=1"
            )),
            enabled && backend == "native" && is_c_compile,
            "{args:?}"
        );
    }
    assert!(links > 0, "expected at least one link-core command");
    if backend == "native" {
        assert!(c_compiles > 0, "expected native runtime compilation");
    }
}

#[test]
fn collect_ref_cycle_is_opt_in_and_backend_specific() {
    let dir = fixture("native_backend/new_native_e2e");
    for backend in ["native", "wasm", "wasm-gc", "js", "llvm"] {
        for value in [None, Some("0"), Some(""), Some("1")] {
            let mut command = moon(&dir).args(["build", "--target", backend]);
            if let Some(value) = value {
                command = command.env("MOON_COLLECT_REF_CYCLE", value);
            }
            let commands = commands(&dir, command);
            assert_collection_flags(&commands, backend, value == Some("1"));
        }
    }
}

#[test]
fn collect_ref_cycle_applies_to_executable_commands_and_standalone_files() {
    let dir = fixture("native_backend/new_native_e2e");
    for backend in ["native", "wasm"] {
        for args in [
            vec!["build", "--release"],
            vec!["run", "main"],
            vec!["test"],
            vec!["bench"],
            vec!["run", "main/main.mbt"],
            vec!["test", "main/main.mbt"],
        ] {
            let mut args = args;
            args.extend(["--target", backend]);
            let commands = commands(
                &dir,
                moon(&dir).args(&args).env("MOON_COLLECT_REF_CYCLE", "1"),
            );
            assert_collection_flags(&commands, backend, true);
        }
    }
}

#[test]
fn collect_ref_cycle_preserves_wasm_allocator_flags_and_wat_output() {
    let dir = fixture("native_backend/new_native_e2e");
    for (allocator, use_env_var) in [("tlsf", false), ("tlsf-mbt", false), ("tlsf-mbt", true)] {
        let mut manifest = serde_json::json!({
            "is-main": true,
        });
        if !use_env_var {
            manifest["link"] =
                serde_json::json!({ "wasm": { "flags": ["-allocator", allocator] } });
        }
        std::fs::write(dir.join("main/moon.pkg.json"), manifest.to_string()).unwrap();
        let commands = commands(
            &dir,
            moon(&dir)
                .args(["build", "--target", "wasm", "--output-wat"])
                .env("MOON_COLLECT_REF_CYCLE", "1")
                .env(
                    "MOON_WASM_NEW_ALLOCATOR",
                    if use_env_var { "1" } else { "0" },
                ),
        );
        assert_collection_flags(&commands, "wasm", true);
        let link = commands
            .iter()
            .find(|args| args.get(1).is_some_and(|arg| arg == "link-core"))
            .unwrap();
        assert!(
            link.windows(2)
                .any(|pair| pair == ["-allocator", allocator])
        );
        assert!(
            link.windows(2)
                .any(|pair| pair[0] == "-o" && pair[1].ends_with(".wat"))
        );
    }
}

#[test]
fn collect_ref_cycle_preserves_native_backend_selection() {
    let dir = fixture("native_backend/new_native_e2e");
    for profile in ["--debug", "--release"] {
        for new_native in ["0", "1"] {
            let links = |value| {
                commands(
                    &dir,
                    moon(&dir)
                        .args(["build", "--target", "native", profile])
                        .env("MOONBIT_NEW_NATIVE", new_native)
                        .env("MOON_COLLECT_REF_CYCLE", value),
                )
                .into_iter()
                .filter(|args| args.get(1).is_some_and(|arg| arg == "link-core"))
                .collect::<Vec<_>>()
            };
            let disabled = links("0");
            assert!(!disabled.is_empty());
            assert_eq!(
                links("1"),
                disabled,
                "profile={profile}, new_native={new_native}"
            );
        }
    }
}

#[test]
fn collect_ref_cycle_native_reclaims_cycles_across_incremental_toggles() {
    let dir = fixture("collect_ref_cycle.in");
    for profile in ["--debug", "--release"] {
        for value in ["0", "1", "0"] {
            moon(&dir)
                .args(["run", ".", "--target", "native", profile])
                .env("MOONBIT_ALLOCATOR", "system")
                .env("MOON_COLLECT_REF_CYCLE", value)
                .assert()
                .success()
                .stdout_eq(format!("before collection: 0\nafter collection: {value}\n"));
        }
        moon(&dir)
            .args(["run", ".", "--target", "native", profile])
            .env("MOON_COLLECT_REF_CYCLE", "1")
            .assert()
            .success()
            .stdout_eq("before collection: 0\nafter collection: 1\n");
    }
}
