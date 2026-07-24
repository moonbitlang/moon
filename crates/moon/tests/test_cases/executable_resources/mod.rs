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

use super::*;

fn assert_resource_mapping(mapping: &Path, source: &Path) {
    assert_eq!(
        std::fs::read_to_string(mapping.join("greeting.txt")).unwrap(),
        "hello from resources\n"
    );

    #[cfg(unix)]
    {
        assert!(
            std::fs::symlink_metadata(mapping)
                .unwrap()
                .file_type()
                .is_symlink(),
            "{} should be a symbolic link",
            mapping.display()
        );
        assert_eq!(
            std::fs::canonicalize(mapping).unwrap(),
            std::fs::canonicalize(source).unwrap()
        );
    }

    #[cfg(windows)]
    {
        assert!(
            junction::exists(mapping).unwrap(),
            "{} should be a junction",
            mapping.display()
        );
        assert_eq!(
            dunce::canonicalize(junction::get_target(mapping).unwrap()).unwrap(),
            dunce::canonicalize(source).unwrap()
        );
    }
}

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) {
    junction::create(target, link).unwrap();
}

#[test]
fn moon_build_maps_the_declared_data_directory_beside_the_artifact() {
    let dir = TestDir::new("executable_resources/executable_resources.in");

    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    assert_resource_mapping(
        &dir.join("_build/wasm/debug/build/app/assets"),
        &dir.join("app/assets"),
    );
}

#[test]
fn moon_run_build_only_maps_the_declared_data_directory_before_reporting_the_artifact() {
    let dir = TestDir::new("executable_resources/executable_resources.in");

    let stdout = get_stdout(&dir, ["run", "app", "--target", "wasm", "--build-only"]);
    assert!(
        stdout.contains("_build/wasm/debug/build/app/app.wasm"),
        "build-only output should report the executable artifact: {stdout}"
    );

    assert_resource_mapping(
        &dir.join("_build/wasm/debug/build/app/assets"),
        &dir.join("app/assets"),
    );
}

#[test]
fn moon_build_reuses_an_existing_data_directory_mapping() {
    let dir = TestDir::new("executable_resources/executable_resources.in");

    for _ in 0..2 {
        moon_cmd(&dir)
            .args(["build", "--target", "wasm"])
            .assert()
            .success();
    }

    assert_resource_mapping(
        &dir.join("_build/wasm/debug/build/app/assets"),
        &dir.join("app/assets"),
    );
}

#[test]
fn changing_data_directory_contents_does_not_rebuild_the_executable() {
    let dir = TestDir::new("executable_resources/executable_resources.in");
    let data_file = dir.join("app/assets/template.mbt");
    std::fs::write(&data_file, "not a package source\n").unwrap();

    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();
    std::fs::write(&data_file, "updated without rebuilding\n").unwrap();

    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq("Finished. moon: no work to do\n")
        .stderr_eq("");
    assert_eq!(
        std::fs::read_to_string(dir.join("_build/wasm/debug/build/app/assets/template.mbt"))
            .unwrap(),
        "updated without rebuilding\n"
    );
}

#[test]
fn data_directory_contents_are_not_discovered_as_a_package() {
    let dir = TestDir::new("executable_resources/executable_resources.in");
    std::fs::write(
        dir.join("app/assets/moon.pkg"),
        "this is resource data, not a package manifest\n",
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(dir.join("_build/wasm/debug/build/app/assets/moon.pkg")).unwrap(),
        "this is resource data, not a package manifest\n"
    );
}

#[test]
fn data_directory_matching_uses_filesystem_identity() {
    let dir = TestDir::new("executable_resources/executable_resources.in");
    let differently_cased_data_dir = dir.join("app/ASSETS");

    // This behavior is observable only on a case-preserving,
    // case-insensitive filesystem.
    if !differently_cased_data_dir.is_dir() {
        return;
    }

    std::fs::write(
        dir.join("app/moon.pkg"),
        r#"pkgtype(kind: "executable")

options(
  data_dir: "ASSETS",
)
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("app/assets/moon.pkg"),
        "this is resource data, not a package manifest\n",
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();
}

#[test]
fn package_shaped_data_directory_contents_are_not_built() {
    let dir = TestDir::new("executable_resources/executable_resources.in");
    std::fs::write(
        dir.join("app/assets/moon.pkg"),
        "pkgtype(kind: \"executable\")\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app/assets/not-package-source.mbt"),
        "this is resource data, not MoonBit source\n",
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(
            dir.join("_build/wasm/debug/build/app/assets/not-package-source.mbt")
        )
        .unwrap(),
        "this is resource data, not MoonBit source\n"
    );
}

#[test]
fn declared_data_directory_must_exist() {
    let dir = TestDir::new("executable_resources/executable_resources.in");
    std::fs::remove_dir_all(dir.join("app/assets")).unwrap();

    let assert = moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("declared executable package data directory does not exist"),
        "stderr: {stderr}"
    );
}

#[test]
fn data_directory_must_not_contain_filesystem_links() {
    let dir = TestDir::new("executable_resources/executable_resources.in");
    let data_dir = dir.join("app/assets");
    let actual_data_dir = dir.join("app/actual-assets");
    std::fs::remove_dir_all(&data_dir).unwrap();
    std::fs::create_dir(&actual_data_dir).unwrap();
    create_directory_link(&actual_data_dir, &data_dir);

    let assert = moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("must not be a symbolic link or junction"),
        "stderr: {stderr}"
    );
}

#[test]
fn data_directory_cannot_escape_the_executable_package() {
    let dir = TestDir::new("executable_resources/executable_resources.in");
    std::fs::write(
        dir.join("app/moon.pkg"),
        r#"pkgtype(kind: "executable")

options(
  data_dir: "../shared",
)
"#,
    )
    .unwrap();
    std::fs::create_dir(dir.join("shared")).unwrap();

    let assert = moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("`data_dir` in `moon.pkg` must name a direct child directory"),
        "stderr: {stderr}"
    );
}

#[test]
fn data_directory_is_only_valid_for_an_executable_package() {
    let dir = TestDir::new("executable_resources/executable_resources.in");
    std::fs::write(
        dir.join("app/moon.pkg"),
        r#"options(
  data_dir: "assets",
)
"#,
    )
    .unwrap();

    let assert = moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("`data_dir` in `moon.pkg` is only valid for an executable package"),
        "stderr: {stderr}"
    );
}

#[test]
fn data_directory_must_be_a_direct_child_of_the_package() {
    let dir = TestDir::new("executable_resources/executable_resources.in");
    std::fs::create_dir(dir.join("app/assets/nested")).unwrap();
    std::fs::write(
        dir.join("app/moon.pkg"),
        r#"pkgtype(kind: "executable")

options(
  data_dir: "assets/nested",
)
"#,
    )
    .unwrap();

    let assert = moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("`data_dir` in `moon.pkg` must name a direct child directory"),
        "stderr: {stderr}"
    );
}
