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

fn registry_index(moon_home: &Path, module: &str) -> PathBuf {
    let (username, unqualified_name) = module.split_once('/').unwrap();
    moon_home
        .join("registry/index/user")
        .join(username)
        .join(format!("{unqualified_name}.index"))
}

fn cache_registry_package(moon_home: &Path, module: &str, version: &str, manifest: &str) {
    cache_registry_package_with_files(moon_home, module, version, manifest, &[]);
}

fn cache_registry_package_with_files(
    moon_home: &Path,
    module: &str,
    version: &str,
    manifest: &str,
    extra_files: &[(&str, &str)],
) {
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, contents) in [
        ("moon.mod.json", manifest.to_owned()),
        ("src/moon.pkg.json", "{}".to_owned()),
        ("src/lib.mbt", "pub fn answer() -> Int { 42 }\n".to_owned()),
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
    let archive = archive.finish().unwrap().into_inner();
    let checksum = format!("{:x}", Sha256::digest(&archive));
    let (username, unqualified_name) = module.split_once('/').unwrap();
    let archive_path = moon_home
        .join("registry/cache")
        .join(username)
        .join(unqualified_name)
        .join(format!("{version}.zip"));
    std::fs::create_dir_all(archive_path.parent().unwrap()).unwrap();
    std::fs::write(archive_path, archive).unwrap();

    let manifest_json = serde_json::from_str::<serde_json::Value>(manifest).unwrap();
    let mut index_entry = serde_json::json!({
        "name": module,
        "version": version,
        "checksum": checksum,
    });
    if let Some(deps) = manifest_json.get("deps") {
        index_entry["deps"] = deps.clone();
    }
    let index_path = registry_index(moon_home, module);
    std::fs::create_dir_all(index_path.parent().unwrap()).unwrap();
    std::fs::write(index_path, format!("{index_entry}\n")).unwrap();
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
        cache_registry_package(
            moon_home.path(),
            MODULE_NAME,
            MODULE_VERSION,
            &format!(r#"{{"name":"{MODULE_NAME}","version":"{MODULE_VERSION}","source":"src"}}"#),
        );
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
        snapbox::cmd::Command::new(moon_bin())
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
            .arg("--quiet")
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

    fn directory(&self, name: &str) -> PathBuf {
        let directory = self.workspace.path().join(name);
        std::fs::create_dir_all(&directory).unwrap();
        directory
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
fn standalone_script_forms_reuse_registry_dependency_build_graph() {
    let fixture = BuildCacheFixture::new();
    let first = fixture.directory("first");
    let second = fixture.directory("second");
    let third = fixture.directory("third");
    let script = write_mbtx(&third, "main.mbtx", MODULE_NAME);
    let source = mbtx_source(MODULE_NAME);

    fixture
        .command(&first)
        .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
        .arg(&source)
        .assert()
        .success();
    fixture
        .command(&second)
        .args(["run", "--target", "wasm-gc", "--build-only", "-"])
        .stdin(&source)
        .assert()
        .success();
    fixture
        .command(&third)
        .args([
            "run",
            script.to_str().unwrap(),
            "--target",
            "wasm-gc",
            "--build-only",
        ])
        .assert()
        .success();

    assert_eq!(fixture.dependency_builds(), 1);
}

#[test]
fn disabled_build_cache_rebuilds_script_dependencies() {
    let fixture = BuildCacheFixture::new();
    for name in ["first", "second"] {
        let directory = fixture.directory(name);
        fixture
            .command(&directory)
            .env("MOON_BUILD_CACHE", "off")
            .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
            .arg(mbtx_source(MODULE_NAME))
            .assert()
            .success();
    }
    assert_eq!(fixture.dependency_builds(), 2);
}

#[test]
fn native_scripts_keep_the_existing_local_build_graph() {
    let fixture = BuildCacheFixture::new();
    for name in ["first", "second"] {
        let directory = fixture.directory(name);
        fixture
            .command(&directory)
            .args(["run", "--target", "native", "--build-only", "-e"])
            .arg(mbtx_source(MODULE_NAME))
            .assert()
            .success();
    }
    assert_eq!(fixture.dependency_builds(), 2);
    assert!(!fixture.build_cache.path().join(".moon-cache").exists());
}

#[test]
fn compiler_contents_are_part_of_script_dependency_graph_identity() {
    let fixture = BuildCacheFixture::new();
    let directories = ["first", "second", "third"].map(|name| fixture.directory(name));

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
    assert_eq!(fixture.dependency_builds(), 2);
}

#[test]
fn prepared_source_checksum_is_part_of_script_dependency_graph_identity() {
    let fixture = BuildCacheFixture::new();
    let first = fixture.directory("first");
    let second = fixture.directory("second");

    fixture
        .command(&first)
        .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
        .arg(mbtx_source(MODULE_NAME))
        .assert()
        .success();

    let entry = fixture
        .dependency_cache
        .path()
        .join("registry/cachetest/shared/1.0.0");
    moonutil::cache::make_cache_tree_writable(&entry).unwrap();
    std::fs::remove_dir_all(entry).unwrap();
    cache_registry_package_with_files(
        fixture.moon_home.path(),
        MODULE_NAME,
        MODULE_VERSION,
        &format!(r#"{{"name":"{MODULE_NAME}","version":"{MODULE_VERSION}","source":"src"}}"#),
        &[("new-archive-entry.txt", "new bytes")],
    );

    fixture
        .command(&second)
        .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
        .arg(mbtx_source(MODULE_NAME))
        .assert()
        .success();
    assert_eq!(fixture.dependency_builds(), 2);
}

#[test]
fn target_is_part_of_script_dependency_graph_identity() {
    let fixture = BuildCacheFixture::new();
    for (name, target) in [
        ("first", "wasm"),
        ("second", "wasm-gc"),
        ("third", "wasm-gc"),
    ] {
        let directory = fixture.directory(name);
        fixture
            .command(&directory)
            .args(["run", "--target", target, "--build-only", "-e"])
            .arg(mbtx_source(MODULE_NAME))
            .assert()
            .success();
    }
    assert_eq!(fixture.dependency_builds(), 2);
}

#[test]
fn resolved_upstream_versions_are_part_of_script_dependency_graph_identity() {
    let fixture = BuildCacheFixture::new();
    let upstream = "cachetest/upstream";
    cache_registry_package(
        fixture.moon_home.path(),
        upstream,
        "1.0.0",
        r#"{"name":"cachetest/upstream","version":"1.0.0","source":"src"}"#,
    );
    let index = registry_index(fixture.moon_home.path(), upstream);
    let first_index = std::fs::read_to_string(&index).unwrap();
    cache_registry_package(
        fixture.moon_home.path(),
        upstream,
        "1.1.0",
        r#"{"name":"cachetest/upstream","version":"1.1.0","source":"src"}"#,
    );
    let second_index = std::fs::read_to_string(&index).unwrap();
    std::fs::write(index, format!("{first_index}{second_index}")).unwrap();
    cache_registry_package(
        fixture.moon_home.path(),
        MODULE_NAME,
        MODULE_VERSION,
        &format!(
            r#"{{
  "name": "{MODULE_NAME}",
  "version": "{MODULE_VERSION}",
  "source": "src",
  "deps": {{ "{upstream}": "1.0.0" }}
}}"#
        ),
    );

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
    for (name, source) in [
        ("first", &first_source),
        ("second", &later_source),
        ("third", &later_source),
    ] {
        let directory = fixture.directory(name);
        fixture
            .command(&directory)
            .args(["run", "--target", "wasm-gc", "--build-only", "-e"])
            .arg(source)
            .assert()
            .success();
    }
    assert_eq!(fixture.dependency_builds(), 2);
}

#[test]
fn corrupt_script_dependency_graph_is_rebuilt() {
    let fixture = BuildCacheFixture::new();
    let directories = ["first", "second", "third"].map(|name| fixture.directory(name));
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
    assert_eq!(fixture.dependency_builds(), 2);
}

#[test]
fn concurrent_scripts_share_one_registry_dependency_build() {
    let fixture = BuildCacheFixture::new();
    let directories = ["first", "second"].map(|name| fixture.directory(name));
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
    assert_eq!(fixture.dependency_builds(), 1);
}
