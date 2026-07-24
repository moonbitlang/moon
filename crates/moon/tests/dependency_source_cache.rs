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

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

const MODULE_NAME: &str = "cachetest/shared";
const MODULE_VERSION: &str = "1.0.0";

fn moon_bin() -> PathBuf {
    snapbox::cargo_bin!("moon").to_owned()
}

fn run_moon(
    current_dir: &Path,
    moon_home: &Path,
    dependency_cache: &Path,
) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(current_dir)
        .env("MOON_HOME", moon_home)
        .env("MOON_DEP_CACHE", dependency_cache)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOONCAKES_REGISTRY", "http://127.0.0.1:9")
        .arg("--quiet")
}

fn mbtx_source(module: &str) -> String {
    format!(
        r#"import {{
  "{module}@{MODULE_VERSION}",
}}

fn main {{
  println(@shared.answer())
}}
"#
    )
}

fn write_mbtx(directory: &Path, name: &str, module: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, mbtx_source(module)).unwrap();
    path
}

fn cache_registry_package(moon_home: &Path) -> PathBuf {
    cache_registry_package_with_manifest(
        moon_home,
        MODULE_NAME,
        &format!(r#"{{"name":"{MODULE_NAME}","version":"{MODULE_VERSION}","source":"src"}}"#),
        0,
    )
}

fn cache_registry_package_with_manifest(
    moon_home: &Path,
    published_name: &str,
    manifest: &str,
    padding_files: usize,
) -> PathBuf {
    cache_registry_package_with_manifest_and_files(
        moon_home,
        published_name,
        manifest,
        padding_files,
        &[],
    )
}

fn cache_registry_package_with_manifest_and_files(
    moon_home: &Path,
    published_name: &str,
    manifest: &str,
    padding_files: usize,
    extra_files: &[(&str, &str)],
) -> PathBuf {
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, contents) in [
        ("moon.mod.json", manifest.to_string()),
        ("src/moon.pkg.json", "{}".to_string()),
        ("src/lib.mbt", "pub fn answer() -> Int { 42 }\n".to_string()),
    ] {
        archive
            .start_file(path, zip::write::FileOptions::default())
            .unwrap();
        archive.write_all(contents.as_bytes()).unwrap();
    }
    for &(path, contents) in extra_files {
        archive
            .start_file(path, zip::write::FileOptions::default())
            .unwrap();
        archive.write_all(contents.as_bytes()).unwrap();
    }
    for index in 0..padding_files {
        archive
            .start_file(
                format!("padding/{index}.txt"),
                zip::write::FileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"padding").unwrap();
    }
    let archive = archive.finish().unwrap().into_inner();

    let manifest_json = serde_json::from_str::<serde_json::Value>(manifest).unwrap();
    let module_version = manifest_json["version"].as_str().unwrap();
    let (username, unqualified_name) = published_name.split_once('/').unwrap();
    let archive_path = moon_home
        .join("registry/cache")
        .join(username)
        .join(unqualified_name)
        .join(format!("{module_version}.zip"));
    std::fs::create_dir_all(archive_path.parent().unwrap()).unwrap();
    std::fs::write(&archive_path, &archive).unwrap();

    let mut index_entry = serde_json::json!({
        "name": published_name,
        "version": module_version,
        "checksum": format!("{:x}", Sha256::digest(&archive)),
    });
    if let Some(deps) = manifest_json.get("deps") {
        index_entry["deps"] = deps.clone();
    }
    let index_path = moon_home
        .join("registry/index/user")
        .join(username)
        .join(format!("{unqualified_name}.index"));
    std::fs::create_dir_all(index_path.parent().unwrap()).unwrap();
    std::fs::write(
        index_path,
        format!("{}\n", serde_json::to_string(&index_entry).unwrap()),
    )
    .unwrap();

    archive_path
}

fn registry_index(moon_home: &Path) -> PathBuf {
    moon_home.join("registry/index/user/cachetest/shared.index")
}

fn write_project(project: &Path) {
    std::fs::write(
        project.join("moon.mod.json"),
        format!(
            r#"{{
  "name": "cachetest/project",
  "version": "0.0.1",
  "deps": {{
    "{MODULE_NAME}": "{MODULE_VERSION}"
  }},
  "source": "src"
}}
"#
        ),
    )
    .unwrap();
    let package = project.join("src/main");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        package.join("moon.pkg.json"),
        format!(r#"{{"import":["{MODULE_NAME}"]}}"#),
    )
    .unwrap();
    std::fs::write(
        package.join("main.mbt"),
        "fn use_dependency() -> Int { @shared.answer() }\n",
    )
    .unwrap();
}

#[test]
fn run_mbtx_forms_reuse_prepared_dependency_source() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let archive = cache_registry_package(moon_home.path());
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);
    let source = mbtx_source(MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success()
        .stdout_eq("42\n");

    std::fs::remove_file(archive).unwrap();

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", "-e", &source])
        .assert()
        .success()
        .stdout_eq("42\n");

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", "-"])
        .stdin(source)
        .assert()
        .success()
        .stdout_eq("42\n");

    assert!(!source_dir.path().join(".mooncakes").exists());
}

#[test]
fn frozen_cache_miss_does_not_initialize_dependency_cache() {
    let moon_home = tempfile::tempdir().unwrap();
    let cache_parent = tempfile::tempdir().unwrap();
    let dependency_cache = cache_parent.path().join("missing");
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package(moon_home.path());
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), &dependency_cache)
        .args(["run", script.to_str().unwrap(), "--frozen"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: Failed to resolve the module dependency graph

Caused by:
    0: When preparing cached packages
    1: Failed to sync dependencies: `frozen` is set, so the build system cannot prepare `cachetest/shared@1.0.0` in the dependency cache

"#]]);

    assert!(!dependency_cache.exists());
}

#[test]
fn frozen_cache_hit_does_not_touch_dependency_lock() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package(moon_home.path());
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success()
        .stdout_eq("42\n");

    let lock = dependency_cache
        .path()
        .join("registry/cachetest/shared")
        .join(moonutil::constants::MOON_LOCK);
    std::fs::write(&lock, "unchanged").unwrap();

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap(), "--frozen"])
        .assert()
        .success()
        .stdout_eq("42\n");

    assert_eq!(std::fs::read_to_string(lock).unwrap(), "unchanged");
}

#[test]
fn ordinary_project_commands_keep_project_local_mooncakes() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    cache_registry_package(moon_home.path());
    write_project(project.path());

    run_moon(project.path(), moon_home.path(), dependency_cache.path())
        .arg("check")
        .assert()
        .success();

    assert!(
        project
            .path()
            .join(".mooncakes/cachetest/shared/moon.mod.json")
            .is_file()
    );
    assert!(
        std::fs::read_dir(dependency_cache.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn non_run_single_file_commands_keep_file_local_mooncakes() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package(moon_home.path());
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["check", script.to_str().unwrap()])
        .assert()
        .success();

    assert!(
        source_dir
            .path()
            .join(".mooncakes/cachetest/shared/moon.mod.json")
            .is_file()
    );
    assert!(
        std::fs::read_dir(dependency_cache.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn multi_segment_registry_module_uses_nested_cache_path() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package_with_manifest(
        moon_home.path(),
        "h/e/l/l/o",
        r#"{"name":"h/e/l/l/o","version":"1.0.0","source":"src"}"#,
        0,
    );
    cache_registry_package_with_manifest(
        moon_home.path(),
        MODULE_NAME,
        r#"{
  "name": "cachetest/shared",
  "version": "1.0.0",
  "source": "src",
  "deps": {
    "h/e/l/l/o": "1.0.0"
  }
}"#,
        0,
    );
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success();

    assert!(
        dependency_cache
            .path()
            .join("registry/h/e/l/l/o/1.0.0/source/src/lib.mbt")
            .is_file()
    );
}

#[test]
fn major_version_module_suffixes_coexist_in_nested_cache() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let modules = [("a/b", "1.0.0"), ("a/b/v2", "2.0.0"), ("a/b/v3", "3.0.0")];
    for &(name, version) in &modules[1..] {
        cache_registry_package_with_manifest(
            moon_home.path(),
            name,
            &format!(r#"{{"name":"{name}","version":"{version}","source":"src"}}"#),
            0,
        );
    }
    cache_registry_package_with_manifest(
        moon_home.path(),
        "a/b",
        r#"{
  "name": "a/b",
  "version": "1.0.0",
  "source": "src",
  "deps": {
    "a/b/v2": "2.0.0",
    "a/b/v3": "3.0.0"
  }
}"#,
        0,
    );
    let script = source_dir.path().join("main.mbtx");
    std::fs::write(
        &script,
        r#"import {
  "a/b@1.0.0",
}

fn main {
  println(@b.answer())
}
"#,
    )
    .unwrap();

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success()
        .stdout_eq("42\n");

    for (name, version) in modules {
        assert!(
            dependency_cache
                .path()
                .join("registry")
                .join(name)
                .join(version)
                .join("source/src/lib.mbt")
                .is_file(),
            "missing cached source for {name}@{version}"
        );
    }
}

#[test]
fn changed_checksum_for_published_version_is_rejected() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package(moon_home.path());
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success();

    std::fs::write(
        registry_index(moon_home.path()),
        format!(
            "{{\"name\":\"{MODULE_NAME}\",\"version\":\"{MODULE_VERSION}\",\"checksum\":\"{}\"}}\n",
            "0".repeat(64)
        ),
    )
    .unwrap();

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: Failed to resolve the module dependency graph

Caused by:
    0: When preparing cached packages
    1: registry checksum for `cachetest/shared@1.0.0` changed; published versions are immutable

"#]]);
}

#[test]
fn postadd_is_rejected() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package_with_manifest(
        moon_home.path(),
        MODULE_NAME,
        &format!(
            r#"{{"name":"{MODULE_NAME}","version":"{MODULE_VERSION}","source":"src","scripts":{{"postadd":"command-that-must-not-run"}}}}"#
        ),
        0,
    );
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: Failed to resolve the module dependency graph

Caused by:
    0: When preparing cached packages
    1: dependency `cachetest/shared@1.0.0` declares `scripts.postadd`, which is not supported by the shared dependency cache

"#]]);
}

#[test]
fn module_prebuild_config_runs_with_shared_dependency_source() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let prebuild_runs = source_dir.path().join("prebuild-runs");
    cache_registry_package_with_manifest_and_files(
        moon_home.path(),
        MODULE_NAME,
        &format!(
            r#"{{"name":"{MODULE_NAME}","version":"{MODULE_VERSION}","source":"src","--moonbit-unstable-prebuild":"build.js"}}"#
        ),
        0,
        &[(
            "build.js",
            "require('node:fs').appendFileSync(process.env.MOON_CACHE_TEST_PREBUILD_RUNS, 'run\\n')\nconsole.log('{}')\n",
        )],
    );
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .env("MOON_CACHE_TEST_PREBUILD_RUNS", &prebuild_runs)
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success()
        .stdout_eq("42\n");

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .env("MOON_CACHE_TEST_PREBUILD_RUNS", &prebuild_runs)
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success()
        .stdout_eq("42\n");

    assert_eq!(
        std::fs::read_to_string(prebuild_runs).unwrap(),
        "run\nrun\n"
    );
}

#[test]
fn published_dependency_source_is_read_only_and_cleanable() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package(moon_home.path());
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success();

    let entry = dependency_cache
        .path()
        .join("registry/cachetest/shared/1.0.0");
    for path in [
        entry.clone(),
        entry.join("checksum"),
        entry.join("source"),
        entry.join("source/src/lib.mbt"),
    ] {
        assert!(
            path.metadata().unwrap().permissions().readonly(),
            "{} is writable",
            path.display()
        );
    }
    assert!(std::fs::write(entry.join("checksum"), "replacement\n").is_err());

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["clean", "--dep-cache"])
        .assert()
        .success();
    assert!(!dependency_cache.path().exists());
}

#[test]
fn disabled_dependency_cache_uses_file_local_mooncakes() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package(moon_home.path());
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .env("MOON_DEP_CACHE", "off")
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success();

    assert!(
        source_dir
            .path()
            .join(".mooncakes/cachetest/shared/moon.mod.json")
            .is_file()
    );
    assert!(
        std::fs::read_dir(dependency_cache.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
#[cfg(unix)]
fn cached_dependency_source_must_not_be_a_symlink() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package(moon_home.path());
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success();

    let entry = dependency_cache
        .path()
        .join("registry/cachetest/shared/1.0.0");
    moonutil::cache::make_cache_tree_writable(&entry).unwrap();
    let source = entry.join("source");
    let moved_source = entry.join("moved-source");
    std::fs::rename(&source, &moved_source).unwrap();
    std::os::unix::fs::symlink(&moved_source, &source).unwrap();
    let checksum = entry.join("checksum");
    let mut checksum_permissions = std::fs::metadata(&checksum).unwrap().permissions();
    checksum_permissions.set_readonly(true);
    std::fs::set_permissions(checksum, checksum_permissions).unwrap();
    let mut entry_permissions = std::fs::metadata(&entry).unwrap().permissions();
    entry_permissions.set_readonly(true);
    std::fs::set_permissions(&entry, entry_permissions).unwrap();

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: Failed to resolve the module dependency graph

Caused by:
    0: When preparing cached packages
    1: prepared dependency source `cachetest/shared@1.0.0` has an invalid source directory

"#]]);
}

#[test]
fn concurrent_first_use_publishes_one_complete_source() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let first_source = tempfile::tempdir().unwrap();
    let second_source = tempfile::tempdir().unwrap();
    cache_registry_package_with_manifest(
        moon_home.path(),
        MODULE_NAME,
        &format!(r#"{{"name":"{MODULE_NAME}","version":"{MODULE_VERSION}","source":"src"}}"#),
        512,
    );
    let first_script = write_mbtx(first_source.path(), "main.mbtx", MODULE_NAME);
    let second_script = write_mbtx(second_source.path(), "main.mbtx", MODULE_NAME);

    let command = |source_dir: &Path, script: &Path| {
        let mut command = std::process::Command::new(moon_bin());
        command
            .current_dir(source_dir)
            .env("MOON_HOME", moon_home.path())
            .env("MOON_DEP_CACHE", dependency_cache.path())
            .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
            .env("MOONCAKES_REGISTRY", "http://127.0.0.1:9")
            .args(["--quiet", "run"])
            .arg(script)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        command
    };

    let first = command(first_source.path(), &first_script).spawn().unwrap();
    let second = command(second_source.path(), &second_script)
        .spawn()
        .unwrap();
    let first_output = first.wait_with_output().unwrap();
    let second_output = second.wait_with_output().unwrap();

    snapbox::cmd::OutputAssert::new(first_output).success();
    snapbox::cmd::OutputAssert::new(second_output).success();
    assert!(
        dependency_cache
            .path()
            .join("registry/cachetest/shared/1.0.0/source/src/lib.mbt")
            .is_file()
    );
}

#[test]
fn archive_manifest_must_match_requested_module() {
    let moon_home = tempfile::tempdir().unwrap();
    let dependency_cache = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    cache_registry_package_with_manifest(
        moon_home.path(),
        MODULE_NAME,
        &format!(r#"{{"name":"cachetest/different","version":"{MODULE_VERSION}","source":"src"}}"#),
        0,
    );
    let script = write_mbtx(source_dir.path(), "main.mbtx", MODULE_NAME);

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: Failed to resolve the module dependency graph

Caused by:
    0: When preparing cached packages
    1: registry archive for `cachetest/shared@1.0.0` contains manifest for `cachetest/different@1.0.0`

"#]]);

    cache_registry_package(moon_home.path());
    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .success();
}
