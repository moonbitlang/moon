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

fn registry_index(moon_home: &Path, module: &str) -> PathBuf {
    let (username, unqualified_name) = module.split_once('/').unwrap();
    moon_home
        .join("registry/index/user")
        .join(username)
        .join(format!("{unqualified_name}.index"))
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
        registry_index(moon_home.path(), MODULE_NAME),
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
    1: prepared dependency source `cachetest/shared@1.0.0` has an invalid entry
    2: prepared dependency source contains symlink `[..]/source`

"#]]);
}

#[test]
#[cfg(unix)]
fn cached_dependency_source_rejects_nested_symlink() {
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
    let source_file = entry.join("source/src/lib.mbt");
    let moved_source_file = entry.join("source/src/lib.real.mbt");
    std::fs::rename(&source_file, &moved_source_file).unwrap();
    std::os::unix::fs::symlink(&moved_source_file, &source_file).unwrap();
    for cache_entry in walkdir::WalkDir::new(&entry) {
        let cache_entry = cache_entry.unwrap();
        let metadata = std::fs::symlink_metadata(cache_entry.path()).unwrap();
        if !metadata.file_type().is_symlink() {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(cache_entry.path(), permissions).unwrap();
        }
    }

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: Failed to resolve the module dependency graph

Caused by:
    0: When preparing cached packages
    1: prepared dependency source `cachetest/shared@1.0.0` has an invalid entry
    2: prepared dependency source contains symlink `[..]/source/src/lib.mbt`

"#]]);
}

#[test]
#[cfg(unix)]
fn cached_dependency_source_rejects_nested_writable_file() {
    use std::os::unix::fs::PermissionsExt;

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
    let source_file = entry.join("source/src/lib.mbt");
    let mut permissions = std::fs::metadata(&source_file).unwrap().permissions();
    permissions.set_mode(permissions.mode() | 0o200);
    std::fs::set_permissions(&source_file, permissions).unwrap();

    run_moon(source_dir.path(), moon_home.path(), dependency_cache.path())
        .args(["run", script.to_str().unwrap()])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: Failed to resolve the module dependency graph

Caused by:
    0: When preparing cached packages
    1: prepared dependency source `cachetest/shared@1.0.0` has an invalid entry
    2: prepared dependency source contains writable entry `[..]/source/src/lib.mbt`

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

#[cfg(unix)]
fn compiler_logger(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join("moonc-logger");
    write_compiler_logger(&path, "initial");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
fn write_compiler_logger(path: &Path, identity: &str) {
    std::fs::write(
        path,
        format!(
            "#!/bin/sh\n# {identity}\nprintf '%s\\n' \"$*\" >> \"$MOONC_LOG\"\nexec \"$MOON_REAL_MOONC\" \"$@\"\n"
        ),
    )
    .unwrap();
}

#[cfg(windows)]
fn compiler_logger(root: &Path) -> PathBuf {
    let path = root.join("moonc-logger.cmd");
    write_compiler_logger(&path, "initial");
    path
}

#[cfg(windows)]
fn write_compiler_logger(path: &Path, identity: &str) {
    std::fs::write(
        path,
        format!(
            "@echo off\r\nrem {identity}\r\necho %*>>\"%MOONC_LOG%\"\r\n\"%MOON_REAL_MOONC%\" %*\r\n"
        ),
    )
    .unwrap();
}

struct BuildCacheFixture {
    workspace: tempfile::TempDir,
    moon_home: tempfile::TempDir,
    dependency_cache: tempfile::TempDir,
    build_cache: tempfile::TempDir,
    compiler_log: PathBuf,
    compiler_logger: PathBuf,
}

impl BuildCacheFixture {
    fn new() -> Self {
        let workspace = tempfile::tempdir().unwrap();
        let moon_home = tempfile::tempdir().unwrap();
        cache_registry_package(moon_home.path());
        let compiler_log = workspace.path().join("moonc.log");
        let compiler_logger = compiler_logger(workspace.path());
        Self {
            workspace,
            moon_home,
            dependency_cache: tempfile::tempdir().unwrap(),
            build_cache: tempfile::tempdir().unwrap(),
            compiler_log,
            compiler_logger,
        }
    }

    fn command(&self, current_dir: &Path) -> snapbox::cmd::Command {
        run_moon(
            current_dir,
            self.moon_home.path(),
            self.dependency_cache.path(),
        )
        .env("MOON_BUILD_CACHE", self.build_cache.path())
        .env("MOONC_OVERRIDE", &self.compiler_logger)
        .env(
            "MOON_REAL_MOONC",
            moonutil::toolchain::BINARIES.moonc.as_path(),
        )
        .env("MOONC_LOG", &self.compiler_log)
    }

    fn process_command(&self, current_dir: &Path) -> std::process::Command {
        let mut command = std::process::Command::new(moon_bin());
        command
            .current_dir(current_dir)
            .env("MOON_HOME", self.moon_home.path())
            .env("MOON_DEP_CACHE", self.dependency_cache.path())
            .env("MOON_BUILD_CACHE", self.build_cache.path())
            .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
            .env("MOONCAKES_REGISTRY", "http://127.0.0.1:9")
            .env("MOONC_OVERRIDE", &self.compiler_logger)
            .env(
                "MOON_REAL_MOONC",
                moonutil::toolchain::BINARIES.moonc.as_path(),
            )
            .env("MOONC_LOG", &self.compiler_log)
            .arg("--quiet");
        command
    }

    fn dependency_builds(&self) -> usize {
        std::fs::read_to_string(&self.compiler_log)
            .unwrap()
            .lines()
            .filter(|line| line.contains("build-package") && line.contains("-pkg cachetest/shared"))
            .count()
    }
}

#[test]
fn standalone_scripts_reuse_registry_dependency_build_graph() {
    let fixture = BuildCacheFixture::new();
    let first = fixture.workspace.path().join("first");
    let second = fixture.workspace.path().join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let second_script = write_mbtx(&second, "main.mbtx", MODULE_NAME);

    fixture
        .command(&first)
        .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
        .arg(mbtx_source(MODULE_NAME))
        .assert()
        .success();
    fixture
        .command(&second)
        .args([
            "run",
            second_script.to_str().unwrap(),
            "--target",
            "wasm-gc",
            "--build-only",
        ])
        .assert()
        .success();

    assert_eq!(
        fixture.dependency_builds(),
        1,
        "the second standalone script should reuse the complete registry dependency graph"
    );
}

#[test]
fn disabled_build_cache_rebuilds_script_dependencies() {
    let fixture = BuildCacheFixture::new();
    let first = fixture.workspace.path().join("first");
    let second = fixture.workspace.path().join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();

    for directory in [&first, &second] {
        fixture
            .command(directory)
            .env("MOON_BUILD_CACHE", "off")
            .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
            .arg(mbtx_source(MODULE_NAME))
            .assert()
            .success();
    }

    assert_eq!(
        fixture.dependency_builds(),
        2,
        "`MOON_BUILD_CACHE=off` should leave both script dependency graphs local"
    );
}

#[test]
fn compiler_contents_are_part_of_script_dependency_graph_identity() {
    let fixture = BuildCacheFixture::new();
    let directories = ["first", "second", "third"].map(|name| {
        let directory = fixture.workspace.path().join(name);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    });

    fixture
        .command(&directories[0])
        .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
        .arg(mbtx_source(MODULE_NAME))
        .assert()
        .success();

    write_compiler_logger(&fixture.compiler_logger, "changed");
    for directory in &directories[1..] {
        fixture
            .command(directory)
            .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
            .arg(mbtx_source(MODULE_NAME))
            .assert()
            .success();
    }

    assert_eq!(
        fixture.dependency_builds(),
        2,
        "changed compiler contents must miss, while the third matching script hits"
    );
}

#[test]
fn target_is_part_of_script_dependency_graph_identity() {
    let fixture = BuildCacheFixture::new();
    let directories = ["first", "second", "third"].map(|name| {
        let directory = fixture.workspace.path().join(name);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    });

    for (directory, target) in directories.iter().zip(["wasm", "wasm-gc", "wasm-gc"]) {
        fixture
            .command(directory)
            .args(["run", "--target", target, "--build-only", "-e"])
            .arg(mbtx_source(MODULE_NAME))
            .assert()
            .success();
    }

    assert_eq!(
        fixture.dependency_builds(),
        2,
        "a new target must miss, while a matching target hits"
    );
}

#[test]
fn corrupt_script_dependency_graph_is_rebuilt() {
    let fixture = BuildCacheFixture::new();
    let directories = ["first", "second", "third"].map(|name| {
        let directory = fixture.workspace.path().join(name);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    });

    fixture
        .command(&directories[0])
        .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
        .arg(mbtx_source(MODULE_NAME))
        .assert()
        .success();

    let graphs = fixture
        .build_cache
        .path()
        .join("graphs/script-dependencies");
    let shard = std::fs::read_dir(graphs)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let entry = std::fs::read_dir(shard)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(entry.join("outputs/0"), "corrupt").unwrap();

    for directory in &directories[1..] {
        fixture
            .command(directory)
            .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
            .arg(mbtx_source(MODULE_NAME))
            .assert()
            .success();
    }

    assert_eq!(
        fixture.dependency_builds(),
        2,
        "a corrupt graph should be rebuilt once and then be reusable"
    );
}

#[test]
fn resolved_upstream_versions_are_part_of_script_dependency_graph_identity() {
    let fixture = BuildCacheFixture::new();
    let upstream = "cachetest/upstream";
    cache_registry_package_with_manifest(
        fixture.moon_home.path(),
        upstream,
        r#"{"name":"cachetest/upstream","version":"1.0.0","source":"src"}"#,
        0,
    );
    let upstream_index = registry_index(fixture.moon_home.path(), upstream);
    let first_index = std::fs::read_to_string(&upstream_index).unwrap();
    cache_registry_package_with_manifest(
        fixture.moon_home.path(),
        upstream,
        r#"{"name":"cachetest/upstream","version":"1.1.0","source":"src"}"#,
        0,
    );
    let second_index = std::fs::read_to_string(&upstream_index).unwrap();
    std::fs::write(&upstream_index, format!("{first_index}{second_index}")).unwrap();
    cache_registry_package_with_manifest(
        fixture.moon_home.path(),
        MODULE_NAME,
        &format!(
            r#"{{
  "name": "{MODULE_NAME}",
  "version": "{MODULE_VERSION}",
  "source": "src",
  "deps": {{
    "{upstream}": "1.0.0"
  }}
}}"#
        ),
        0,
    );

    let directories = ["first", "second", "third"].map(|name| {
        let directory = fixture.workspace.path().join(name);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    });
    let first_source = mbtx_source(MODULE_NAME);
    let later_source = format!(
        r#"import {{
  "{MODULE_NAME}@{MODULE_VERSION}",
  "{upstream}@1.1.0",
}}

fn main {{
  println(@shared.answer())
}}
"#
    );
    for (directory, source) in directories
        .iter()
        .zip([&first_source, &later_source, &later_source])
    {
        fixture
            .command(directory)
            .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
            .arg(source)
            .assert()
            .success();
    }

    assert_eq!(
        fixture.dependency_builds(),
        2,
        "changing a dependency's resolved upstream version must miss, while a matching resolution hits"
    );
}

#[test]
fn concurrent_scripts_share_one_registry_dependency_build() {
    let fixture = BuildCacheFixture::new();
    let directories = ["first", "second"].map(|name| {
        let directory = fixture.workspace.path().join(name);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    });
    let source = mbtx_source(MODULE_NAME);

    let children = directories.map(|directory| {
        fixture
            .process_command(&directory)
            .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
            .arg(&source)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    });
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "concurrent script build failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert_eq!(
        fixture.dependency_builds(),
        1,
        "the dependency graph lock should turn the second cold build into a restore"
    );
}
