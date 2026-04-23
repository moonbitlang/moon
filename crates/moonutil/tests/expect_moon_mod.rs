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

use moonutil::common::{
    read_module_desc_file_in_dir, read_module_from_dsl, write_module_dsl_to_file,
};
use moonutil::module::{MoonModJSON, convert_module_to_mod_json};
use moonutil::package::SupportedTargetsConfig;
use semver::Version;

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "moonutil-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn read_module_from_dsl_basic() {
    let path = fixture_dir("module_dsl_only").join("moon.mod");
    let module = read_module_from_dsl(&path).expect("read moon.mod");
    let json = convert_module_to_mod_json(module);
    let actual = serde_json_lenient::to_string_pretty(&json).expect("serialize module");

    expect_test::expect![[r#"
        {
          "name": "example/dsl_only",
          "version": "0.1.0",
          "deps": {
            "moonbitlang/cli": "0.1.2",
            "moonbitlang/core": "0.4.8",
            "moonbitlang/x": "0.4.6"
          },
          "bin-deps": {
            "tool/cli": {
              "path": "../bin",
              "bin_pkg": [
                "cli"
              ]
            }
          },
          "readme": "README.mbt.md",
          "repository": "https://example.com/repo",
          "license": "Apache-2.0",
          "keywords": [
            "dsl",
            "fixture"
          ],
          "description": "Fixture for module DSL parsing",
          "compile-flags": [
            "-DDEBUG",
            "-Wall"
          ],
          "link-flags": [
            "-lm"
          ],
          "source": "src",
          "warn-list": "-unused-deprecated",
          "include": [
            "src/**",
            "README.mbt.md"
          ],
          "exclude": [
            "target/**",
            "**/*.tmp"
          ],
          "scripts": {
            "prebuild": "node ./prebuild.js",
            "postbuild": "echo done"
          },
          "preferred-target": "wasm-gc"
        }"#]]
    .assert_eq(&actual);
}

#[test]
fn read_module_desc_prefers_dsl() {
    let dir = fixture_dir("module_both");
    let module = read_module_desc_file_in_dir(&dir).expect("read module descriptor");
    assert_eq!(module.name, "example/dsl_prefers");
}

#[test]
fn read_module_from_dsl_rejects_local_deps() {
    let dir = temp_dir("local-module-read");
    let path = dir.join("moon.mod");
    std::fs::write(
        &path,
        r#"name = "example/mod"

options(
  deps: {
    "example/local": { "path": "../local" },
  },
)
"#,
    )
    .unwrap();

    let err = read_module_from_dsl(&path).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains(
            "moon.mod does not support local dependency `example/local` in `import`; use workspace configuration in `moon.work` instead"
        ),
        "{err:?}"
    );
}

#[test]
fn read_module_from_dsl_supports_supported_targets_shorthand() {
    let dir = temp_dir("supported-targets-module-read");
    let path = dir.join("moon.mod");
    std::fs::write(
        &path,
        r#"name = "example/mod"
supported_targets = "js"
"#,
    )
    .unwrap();

    let module = read_module_from_dsl(&path).unwrap();
    match module.supported_targets {
        Some(SupportedTargetsConfig::Expr(expr)) => assert_eq!(expr, "js"),
        other => panic!("unexpected supported_targets: {other:?}"),
    }
}

#[test]
fn read_module_from_dsl_merges_import_into_existing_deps() {
    let dir = temp_dir("merge-import-deps-module-read");
    let path = dir.join("moon.mod");
    std::fs::write(
        &path,
        r#"name = "example/mod"

import {
  "example/new@2.0.0",
}

options(
  deps: {
    "example/existing": "1.0.0",
  },
)
"#,
    )
    .unwrap();

    let module = read_module_from_dsl(&path).unwrap();

    let existing_version = Version::parse("1.0.0").unwrap();
    let new_version = Version::parse("2.0.0").unwrap();
    assert_eq!(
        module.deps.get("example/existing").unwrap().version(),
        Some(&existing_version)
    );
    assert_eq!(
        module.deps.get("example/new").unwrap().version(),
        Some(&new_version)
    );
}

#[test]
fn read_module_from_dsl_rejects_unversioned_registry_deps() {
    let dir = temp_dir("unversioned-registry-module-read");
    let path = dir.join("moon.mod");
    std::fs::write(
        &path,
        r#"name = "example/mod"

import {
  "example/no-version",
}
"#,
    )
    .unwrap();

    let err = read_module_from_dsl(&path).unwrap_err();
    assert!(
        err.to_string()
            .contains("moon.mod only supports versioned registry dependencies in `import`, found `example/no-version`"),
        "{err:?}"
    );
}

#[test]
fn read_module_from_dsl_rejects_aliased_imports() {
    let dir = temp_dir("aliased-import-module-read");
    let path = dir.join("moon.mod");
    std::fs::write(
        &path,
        r#"name = "example/mod"

import {
  "example/dep@1.2.3" @dep,
}
"#,
    )
    .unwrap();

    let err = read_module_from_dsl(&path).unwrap_err();
    assert!(
        err.to_string()
            .contains("\"xxx\"@pkg is not supported in moon.mod"),
        "{err:?}"
    );
}

#[test]
fn write_module_dsl_uses_canonical_sections() {
    let dir = temp_dir("canonical-module");
    let module: MoonModJSON = serde_json_lenient::from_str(
        r#"{
          "name": "example/mod",
          "version": "0.1.0",
          "deps": {
            "example/dep": "1.2.3"
          },
          "warn-list": "+w1-w2",
          "readme": "README.md",
          "license": "Apache-2.0",
          "include": ["src/**", "README.md"],
          "supported-targets": ["wasm-gc", "js"]
        }"#,
    )
    .unwrap();

    write_module_dsl_to_file(&module, &dir).unwrap();

    let actual = std::fs::read_to_string(dir.join("moon.mod")).unwrap();
    expect_test::expect![[r#"
        name = "example/mod"

        version = "0.1.0"

        import {
          "example/dep@1.2.3",
        }

        warnings = "+w1-w2"

        options(
          "include": [ "src/**", "README.md" ],
          license: "Apache-2.0",
          readme: "README.md",
          "supported-targets": [ "wasm-gc", "js" ],
        )"#]]
    .assert_eq(&actual);
}

#[test]
fn write_module_dsl_rejects_unversioned_registry_deps() {
    let dir = temp_dir("unversioned-registry-module");
    let module: MoonModJSON = serde_json_lenient::from_str(
        r#"{
          "name": "example/mod",
          "deps": {
            "example/no-version": {}
          }
        }"#,
    )
    .unwrap();

    let err = write_module_dsl_to_file(&module, &dir).unwrap_err();
    assert!(
        err.to_string()
            .contains("moon.mod only supports versioned registry dependencies in `import`, found `example/no-version`"),
        "{err:?}"
    );
}

#[test]
fn write_module_dsl_rejects_local_deps() {
    let dir = temp_dir("local-module");
    let module: MoonModJSON = serde_json_lenient::from_str(
        r#"{
          "name": "example/mod",
          "deps": {
            "example/local": { "path": "../local" }
          }
        }"#,
    )
    .unwrap();

    let err = write_module_dsl_to_file(&module, &dir).unwrap_err();
    assert!(
        err.to_string().contains(
            "moon.mod does not support local dependency `example/local` in `import`; use workspace configuration in `moon.work` instead"
        ),
        "{err:?}"
    );
}

#[test]
fn write_module_dsl_rejects_git_deps() {
    let dir = temp_dir("git-module");
    let module: MoonModJSON = serde_json_lenient::from_str(
        r#"{
          "name": "example/mod",
          "deps": {
            "example/git": { "git": "https://example.com/repo.git" }
          }
        }"#,
    )
    .unwrap();

    let err = write_module_dsl_to_file(&module, &dir).unwrap_err();
    assert!(
        err.to_string().contains(
            "moon.mod only supports registry dependencies in `import`, found structured dependency `example/git`"
        ),
        "{err:?}"
    );
}
