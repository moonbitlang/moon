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

#[test]
fn cross_platform_compiles_application_and_core_with_the_same_cfg() {
    let project = tempfile::tempdir().unwrap();
    let core = tempfile::tempdir().unwrap();
    let (predicate, compiler, probe) = if cfg!(windows) {
        (
            "not(platform=\"windows\")",
            "cross-cc.cmd",
            "@echo x86_64-linux-gnu\r\n",
        )
    } else {
        (
            "platform=\"windows\"",
            "cross-cc",
            "#!/bin/sh\necho x86_64-w64-mingw32\n",
        )
    };
    let compiler = project.path().join(compiler);
    std::fs::write(&compiler, probe).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(project.path().join("moon.mod"), "name = \"test/cross\"\n").unwrap();
    std::fs::write(project.path().join("moon.pkg"), "").unwrap();
    std::fs::write(project.path().join("lib.mbt"), format!(
        "#cfg({predicate})\nfn selected() -> Unit {{ @prelude.cross_only() }}\npub fn entry() -> Unit {{ selected() }}\n"
    )).unwrap();
    std::fs::write(
        core.path().join("moon.mod"),
        "name = \"moonbitlang/core\"\n",
    )
    .unwrap();
    for package in ["builtin", "prelude"] {
        std::fs::create_dir(core.path().join(package)).unwrap();
        // A complete host bundle passes the usual file-existence check.
        let bundle = core
            .path()
            .join("_build/native/release/bundle")
            .join(package);
        std::fs::create_dir_all(&bundle).unwrap();
        for ext in ["mi", "core"] {
            std::fs::write(bundle.join(format!("{package}.{ext}")), "host artifact").unwrap();
        }
    }
    std::fs::write(core.path().join("builtin/moon.pkg"), "").unwrap();
    std::fs::write(
        core.path().join("builtin/builtin.mbt"),
        "#builtin\npub type Unit\n",
    )
    .unwrap();
    std::fs::write(
        core.path().join("prelude/moon.pkg"),
        "import { \"moonbitlang/core/builtin\" }\n",
    )
    .unwrap();
    std::fs::write(core.path().join("prelude/prelude.mbt"), format!(
        "pub using @builtin {{type Unit}}\n#cfg({predicate})\npub fn cross_only() -> Unit {{ () }}\n"
    )).unwrap();
    for verb in ["check", "build"] {
        snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
            .current_dir(project.path())
            .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
            .env("MOON_CORE_OVERRIDE", core.path())
            .env("MOON_CC", &compiler)
            .env("MOON_DEP_CACHE", "off")
            .env("MOON_WORK", "off")
            .args([verb, "--target", "native", "--release"])
            .assert()
            .success();
    }
}

#[test]
fn incomplete_stdlib_builds_interfaces_in_the_project() {
    let project = tempfile::tempdir().unwrap();
    let core = tempfile::tempdir().unwrap();
    for (name, contents) in [
        ("moon.mod", "name = \"test/fallback\"\n"),
        (
            "moon.pkg",
            "import { \"moonbitlang/core/api\" }\npkgtype(kind: \"executable\")\n",
        ),
        ("main.mbt", "fn main { let _ = @api.message() }\n"),
    ] {
        std::fs::write(project.path().join(name), contents).unwrap();
    }
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases/query_symbol.in/core");
    for name in [
        "moon.mod",
        "builtin/moon.pkg",
        "builtin/builtin.mbt",
        "prelude/moon.pkg",
    ] {
        let output = core.path().join(name);
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::copy(fixture.join(name), output).unwrap();
    }
    // Default virtual implementations need both a local contract and Core IR.
    std::fs::create_dir(core.path().join("api")).unwrap();
    for (name, contents) in [
        (
            "moon.pkg",
            "import { \"moonbitlang/core/builtin\" }\nvirtual(has_default: true)\n",
        ),
        (
            "pkg.mbti",
            "package \"moonbitlang/core/api\"\n\nfn message() -> @builtin.String\n",
        ),
        (
            "api.mbt",
            "pub fn message() -> @builtin.String { @builtin.dummy() }\n",
        ),
    ] {
        std::fs::write(core.path().join("api").join(name), contents).unwrap();
    }
    // The compiler preloads builtin-type extension interfaces outside core,
    // even when neither the application nor prelude imports them explicitly.
    std::fs::create_dir(core.path().join("result")).unwrap();
    std::fs::write(core.path().join("result/moon.pkg"), "").unwrap();
    // A partial bundle must not be mixed with freshly compiled interfaces.
    let partial = core.path().join("_build/wasm-gc/release/bundle/prelude");
    std::fs::create_dir_all(&partial).unwrap();
    for extension in ["mi", "core"] {
        std::fs::write(
            partial.join(format!("prelude.{extension}")),
            "stale artifact",
        )
        .unwrap();
    }

    for _ in 0..2 {
        snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
            .current_dir(project.path())
            .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
            .env("MOON_CORE_OVERRIDE", core.path())
            .env("MOON_DEP_CACHE", "off")
            .env("MOON_WORK", "off")
            .args(["build", "--target", "wasm-gc"])
            .assert()
            .success()
            .stdout_eq(snapbox::str![[r#"
Artifacts:
  test/fallback  wasm-gc  [..]/_build/wasm-gc/debug/build/fallback.wasm

"#]]);
    }
    // Documentation consumes packages.json as well as the compiler index, so
    // it must see locally built core interfaces as third-party dependencies.
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(project.path())
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_CORE_OVERRIDE", core.path())
        .env("MOON_DEP_CACHE", "off")
        .env("MOON_WORK", "off")
        .arg("doc")
        .assert()
        .success();
    let artifacts = project
        .path()
        .join("_build/wasm-gc/debug/build/.mooncakes/moonbitlang/core");
    for package in ["builtin", "prelude", "api", "result"] {
        for extension in ["mi", "core"] {
            assert!(
                artifacts
                    .join(format!("{package}/{package}.{extension}"))
                    .is_file()
            );
        }
    }
    assert!(
        project
            .path()
            .join("_build/wasm-gc/debug/build/fallback.wasm")
            .is_file()
    );
    assert!(
        !core
            .path()
            .join("_build/wasm-gc/release/bundle/builtin")
            .exists()
    );
    assert!(!core.path().join("_build/wasm-gc/debug").exists());
    assert_eq!(
        std::fs::read_to_string(partial.join("prelude.mi")).unwrap(),
        "stale artifact"
    );
}

#[test]
fn missing_stdlib_declarations_fail_before_invoking_the_compiler() {
    for missing in ["builtin", "prelude", "coverage"] {
        let project = tempfile::tempdir().unwrap();
        let core = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("moon.mod"), "name = \"test/missing\"\n").unwrap();
        std::fs::write(project.path().join("moon.pkg"), "").unwrap();
        std::fs::write(project.path().join("lib.mbt"), "").unwrap();
        std::fs::write(
            core.path().join("moon.mod"),
            "name = \"moonbitlang/core\"\n",
        )
        .unwrap();
        for package in ["builtin", "prelude", "coverage"] {
            if package != missing {
                std::fs::create_dir(core.path().join(package)).unwrap();
                std::fs::write(core.path().join(package).join("moon.pkg"), "").unwrap();
            }
        }
        let mut command = snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
            .current_dir(project.path())
            .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
            .env("MOON_CORE_OVERRIDE", core.path())
            .env("MOONC_OVERRIDE", project.path().join("must-not-run-moonc"))
            .env("MOON_DEP_CACHE", "off")
            .env("MOON_WORK", "off")
            .args(["check", "--target", "wasm-gc"]);
        if missing == "coverage" {
            command = command.arg("--enable-coverage");
        }
        command.assert().failure().stderr_eq(format!(
            "...\n[..]toolchain is incomplete: missing standard-library package `moonbitlang/core/{missing}`; reinstall the toolchain or check MOON_CORE_OVERRIDE\n"
        ));
    }
}

#[test]
fn unusable_core_manifests_keep_the_underlying_error() {
    for (manifest, cause) in [
        (
            None,
            "    3: Failed to find `moon.mod` or `moon.mod.json` for module at path `[..]`\n",
        ),
        (
            Some("name = 42\n"),
            "    3: invalid `name` in moon.mod\n    4: invalid type: integer `42`, expected a string\n",
        ),
    ] {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join("moon.mod"),
            "name = \"test/manifest\"\n",
        )
        .unwrap();
        std::fs::write(project.path().join("moon.pkg"), "").unwrap();
        let core = project.path().join("core");
        if let Some(contents) = manifest {
            std::fs::create_dir(&core).unwrap();
            std::fs::write(core.join("moon.mod"), contents).unwrap();
        }
        snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
            .current_dir(project.path())
            .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
            .env("MOON_CORE_OVERRIDE", core)
            .env("MOON_DEP_CACHE", "off")
            .env("MOON_WORK", "off")
            .args(["check", "--target", "wasm-gc"])
            .assert()
            .failure()
            .stdout_eq("")
            .stderr_eq(format!(
                "Error: Failed to calculate build plan\n\nCaused by:\n    0: Failed to resolve the module dependency graph\n    1: Cannot inject the standard library `moonbitlang/core`\n    2: Cannot load the standard-library module manifest in `[..]`; reinstall the toolchain or set MOON_CORE_OVERRIDE to a valid core checkout\n{cause}"
            ));
    }
}
