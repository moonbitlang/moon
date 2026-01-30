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

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

fn expected_bin_name(base: &str) -> String {
    let mut name = base.to_string();
    if cfg!(windows) {
        name.push_str(".exe");
    }
    name
}

fn moon_bin() -> PathBuf {
    snapbox::cargo_bin!("moon").to_owned()
}

fn prepare_install_dir() -> Option<TempDir> {
    let temp = tempfile::tempdir().ok()?;
    std::fs::create_dir_all(temp.path()).ok()?;
    Some(temp)
}

fn write_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn write_mod_json(dir: &Path, name: &str, preferred_target: Option<&str>) -> anyhow::Result<()> {
    let content = match preferred_target {
        Some(target) => format!(
            r#"{{
  "name": "{name}",
  "version": "0.1.0",
  "preferred-target": "{target}"
}}
"#
        ),
        None => format!(
            r#"{{
  "name": "{name}",
  "version": "0.1.0"
}}
"#
        ),
    };
    write_file(&dir.join("moon.mod.json"), &content)
}

fn write_main_pkg(dir: &Path, bin_name: &str) -> anyhow::Result<()> {
    write_file(
        &dir.join("moon.pkg.json"),
        &format!(
            r#"{{
  "is-main": true,
  "bin-name": "{bin_name}"
}}
"#
        ),
    )?;
    write_file(&dir.join("main.mbt"), "fn main {}\n")
}

fn write_lib_pkg(dir: &Path) -> anyhow::Result<()> {
    write_file(
        &dir.join("moon.pkg.json"),
        r#"{
  "import": {}
}
"#,
    )?;
    write_file(&dir.join("lib.mbt"), "let _ = 1\n")
}

fn run_install_local(
    install_dir: &Path,
    module_path: &Path,
    package_path: Option<&str>,
) -> anyhow::Result<std::process::Output> {
    let mut cmd = Command::new(moon_bin());
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("install")
        .arg("--path")
        .arg(module_path)
        .arg("--bin")
        .arg(install_dir);
    if let Some(package_path) = package_path {
        cmd.arg(package_path);
    }
    Ok(cmd.output()?)
}

#[test]
fn test_moon_install_local_module_installs_main_packages() -> anyhow::Result<()> {
    let Some(install_dir) = prepare_install_dir() else {
        return Ok(());
    };
    let module_dir = tempfile::tempdir()?;
    write_mod_json(module_dir.path(), "localuser/localmod", None)?;
    write_main_pkg(module_dir.path(), "root-tool")?;
    write_main_pkg(&module_dir.path().join("tools"), "sub-tool")?;
    write_lib_pkg(&module_dir.path().join("lib"))?;
    let root_bin = "root-tool";
    let sub_bin = "sub-tool";

    let out = run_install_local(install_dir.path(), module_dir.path(), None)?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "moon install --path failed: stdout={} stderr={stderr}",
        String::from_utf8_lossy(&out.stdout)
    );

    assert!(
        install_dir
            .path()
            .join(expected_bin_name(root_bin))
            .exists()
    );
    assert!(install_dir.path().join(expected_bin_name(sub_bin)).exists());
    assert!(
        !install_dir
            .path()
            .join(expected_bin_name("lib-tool"))
            .exists()
    );
    Ok(())
}

#[test]
fn test_moon_install_local_requires_main_package() -> anyhow::Result<()> {
    let Some(install_dir) = prepare_install_dir() else {
        return Ok(());
    };
    let module_dir = tempfile::tempdir()?;
    write_mod_json(module_dir.path(), "localuser/nomains", None)?;
    write_lib_pkg(module_dir.path())?;

    let out = run_install_local(install_dir.path(), module_dir.path(), None)?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "expected failure, got success");
    assert!(
        stderr.contains("no `is_main` packages found"),
        "unexpected stderr: {stderr}"
    );
    Ok(())
}

#[test]
fn test_moon_install_local_rejects_non_native_preferred_target() -> anyhow::Result<()> {
    let Some(install_dir) = prepare_install_dir() else {
        return Ok(());
    };
    let module_dir = tempfile::tempdir()?;
    write_mod_json(module_dir.path(), "localuser/prefertarget", Some("wasm"))?;
    write_main_pkg(module_dir.path(), "root-tool")?;

    let out = run_install_local(install_dir.path(), module_dir.path(), None)?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "expected failure, got success");
    assert!(
        stderr.contains("prefers target `wasm`"),
        "unexpected stderr: {stderr}"
    );
    Ok(())
}

#[test]
fn test_moon_install_local_rejects_package_path() -> anyhow::Result<()> {
    let Some(install_dir) = prepare_install_dir() else {
        return Ok(());
    };
    let module_dir = tempfile::tempdir()?;
    write_mod_json(module_dir.path(), "localuser/localmod", None)?;
    write_main_pkg(module_dir.path(), "root-tool")?;
    write_main_pkg(&module_dir.path().join("tools"), "sub-tool")?;

    let out = run_install_local(
        install_dir.path(),
        module_dir.path(),
        Some("localuser/localmod"),
    )?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "expected failure, got success");
    assert!(
        stderr.contains("cannot be used with '[MODULE_PATH]'"),
        "unexpected stderr: {stderr}"
    );
    Ok(())
}
