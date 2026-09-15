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

use std::path::PathBuf;

use moon_test_util::test_dir::TestDir;
use moonbuild_debug::graph::{BuildGraphDump, ENV_VAR};

fn fixture() -> TestDir {
    TestDir::from_case_root(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases"),
        "native_backend/new_native_e2e",
        true,
    )
}

fn moon(dir: &TestDir) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(dir)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .env_remove("MOON_COLLECT_REF_CYCLE")
        .env_remove("MOONBIT_ALLOCATOR")
        .env_remove("MOON_CC")
        .env("MOONBIT_NEW_NATIVE", "1")
}

fn commands(dir: &TestDir, args: &[&str], value: Option<&str>) -> Vec<Vec<String>> {
    let graph = dir.join("graph.jsonl");
    let mut command = moon(dir).args(args).arg("--dry-run").env(ENV_VAR, &graph);
    if let Some(value) = value {
        command = command.env("MOON_COLLECT_REF_CYCLE", value);
    }
    command.assert().success();
    BuildGraphDump::read_from(std::fs::File::open(graph).unwrap())
        .unwrap()
        .nodes
        .into_iter()
        .filter_map(|node| node.command)
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
        if enabled && backend == "native" && is_link_core {
            assert!(
                args.windows(2)
                    .any(|pair| pair[0] == "-o" && pair[1].ends_with(".c")),
                "cycle collection requires generated C: {args:?}"
            );
        }
    }
    assert!(links > 0, "expected at least one link-core command");
    if backend == "native" {
        assert!(c_compiles > 0, "expected native runtime compilation");
    }
}

#[test]
fn collect_ref_cycle_is_opt_in_and_backend_specific() {
    let dir = fixture();
    for backend in ["native", "wasm", "wasm-gc", "js", "llvm"] {
        for value in [None, Some("0"), Some(""), Some("1")] {
            let commands = commands(&dir, &["build", "--target", backend], value);
            assert_collection_flags(&commands, backend, value == Some("1"));
        }
    }
}

#[test]
fn collect_ref_cycle_applies_to_executable_commands_and_standalone_files() {
    let dir = fixture();
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
            let commands = commands(&dir, &args, Some("1"));
            assert_collection_flags(&commands, backend, true);
        }
    }
}

#[test]
fn collect_ref_cycle_preserves_wasm_allocator_flags_and_wat_output() {
    let dir = fixture();
    for allocator in ["tlsf", "tlsf-mbt"] {
        let manifest = serde_json::json!({
            "is-main": true,
            "link": { "wasm": { "flags": ["-allocator", allocator] } },
        });
        std::fs::write(dir.join("main/moon.pkg.json"), manifest.to_string()).unwrap();
        let commands = commands(
            &dir,
            &["build", "--target", "wasm", "--output-wat"],
            Some("1"),
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
fn collect_ref_cycle_native_runs_across_incremental_toggles() {
    let dir = fixture();
    for profile in ["--debug", "--release"] {
        for value in ["0", "1", "0"] {
            moon(&dir)
                .args(["run", "main", "--target", "native", profile])
                .env("MOONBIT_ALLOCATOR", "system")
                .env("MOON_COLLECT_REF_CYCLE", value)
                .assert()
                .success()
                .stdout_eq("new native run ok\n");
        }
    }
    moon(&dir)
        .args(["run", "main", "--target", "native", "--release"])
        .env("MOON_COLLECT_REF_CYCLE", "1")
        .assert()
        .success()
        .stdout_eq("new native run ok\n");
}
