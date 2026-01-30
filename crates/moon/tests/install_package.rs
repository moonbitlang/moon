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

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Context;
use tempfile::TempDir;

fn moon_bin() -> PathBuf {
    snapbox::cargo_bin!("moon").to_owned()
}

fn expected_bin_name(base: &str) -> String {
    let mut name = base.to_string();
    if cfg!(windows) {
        name.push_str(".exe");
    }
    name
}

fn real_moon_home() -> Option<PathBuf> {
    std::env::var_os("MOON_HOME")
        .map(PathBuf::from)
        .or_else(|| home::home_dir().map(|h| h.join(".moon")))
}

fn prepare_moon_home() -> Option<TempDir> {
    let real_home = real_moon_home()?;
    let real_lib = real_home.join("lib");
    let real_include = real_home.join("include");
    if !real_lib.exists() {
        return None;
    }
    if !real_include.exists() {
        return None;
    }
    let temp = tempfile::tempdir().ok()?;
    std::fs::create_dir_all(temp.path().join("mooncakes_bin")).ok()?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real_lib, temp.path().join("lib")).ok()?;
        std::os::unix::fs::symlink(&real_include, temp.path().join("include")).ok()?;
    }
    #[cfg(windows)]
    {
        // Avoid Windows symlink permission issues for now.
        return None;
    }
    Some(temp)
}

struct LocalPackageSpec<'a> {
    dir: &'a str,
    is_main: bool,
    bin_name: &'a str,
}

fn write_local_module(
    root: &Path,
    module_name: &str,
    preferred_target: Option<&str>,
    packages: &[LocalPackageSpec<'_>],
) -> anyhow::Result<()> {
    let mod_json = serde_json::json!({
        "name": module_name,
        "version": "0.1.0",
    });
    let mod_json = if let Some(preferred) = preferred_target {
        let mut json = mod_json;
        json["preferred-target"] = serde_json::json!(preferred);
        json
    } else {
        mod_json
    };
    std::fs::write(
        root.join("moon.mod.json"),
        serde_json::to_string_pretty(&mod_json)?,
    )?;

    for pkg in packages {
        let pkg_dir = if pkg.dir.is_empty() {
            root.to_path_buf()
        } else {
            root.join(pkg.dir)
        };
        std::fs::create_dir_all(&pkg_dir)?;
        let pkg_json = if pkg.is_main {
            serde_json::json!({
                "is-main": true,
                "bin-name": pkg.bin_name,
            })
        } else {
            serde_json::json!({
                "import": {},
            })
        };
        std::fs::write(
            pkg_dir.join("moon.pkg.json"),
            serde_json::to_string_pretty(&pkg_json)?,
        )?;
        if pkg.is_main {
            std::fs::write(pkg_dir.join("main.mbt"), "fn main {}\n")?;
        } else {
            std::fs::write(pkg_dir.join("lib.mbt"), "let _ = 1\n")?;
        }
    }
    Ok(())
}

fn run_install_local(
    moon_home: &Path,
    module_path: &Path,
    package_path: Option<&str>,
) -> anyhow::Result<std::process::Output> {
    let mut cmd = Command::new(moon_bin());
    cmd.env("MOON_HOME", moon_home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("install")
        .arg("--path")
        .arg(module_path);
    if let Some(package_path) = package_path {
        cmd.arg(package_path);
    }
    Ok(cmd.output()?)
}

#[test]
fn test_moon_install_local_module_installs_main_packages() -> anyhow::Result<()> {
    let Some(moon_home) = prepare_moon_home() else {
        return Ok(());
    };
    let module_dir = tempfile::tempdir().context("failed to create module tempdir")?;
    let module_name = "localuser/localmod";
    let root_bin = "root-tool";
    let sub_bin = "sub-tool";
    write_local_module(
        module_dir.path(),
        module_name,
        None,
        &[
            LocalPackageSpec {
                dir: "",
                is_main: true,
                bin_name: root_bin,
            },
            LocalPackageSpec {
                dir: "tools",
                is_main: true,
                bin_name: sub_bin,
            },
            LocalPackageSpec {
                dir: "lib",
                is_main: false,
                bin_name: "lib-tool",
            },
        ],
    )?;

    let out = run_install_local(moon_home.path(), module_dir.path(), None)?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "moon install --path failed: stdout={} stderr={stderr}",
        String::from_utf8_lossy(&out.stdout)
    );

    let install_dir = moon_home.path().join("mooncakes_bin");
    assert!(install_dir.join(expected_bin_name(root_bin)).exists());
    assert!(install_dir.join(expected_bin_name(sub_bin)).exists());
    assert!(!install_dir.join(expected_bin_name("lib-tool")).exists());
    Ok(())
}

#[test]
fn test_moon_install_local_requires_main_package() -> anyhow::Result<()> {
    let Some(moon_home) = prepare_moon_home() else {
        return Ok(());
    };
    let module_dir = tempfile::tempdir().context("failed to create module tempdir")?;
    let module_name = "localuser/nomains";
    write_local_module(
        module_dir.path(),
        module_name,
        None,
        &[LocalPackageSpec {
            dir: "",
            is_main: false,
            bin_name: "no-main",
        }],
    )?;

    let out = run_install_local(moon_home.path(), module_dir.path(), None)?;
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
    let Some(moon_home) = prepare_moon_home() else {
        return Ok(());
    };
    let module_dir = tempfile::tempdir().context("failed to create module tempdir")?;
    let module_name = "localuser/prefertarget";
    write_local_module(
        module_dir.path(),
        module_name,
        Some("wasm"),
        &[
            LocalPackageSpec {
                dir: "",
                is_main: true,
                bin_name: "prefer-tool",
            },
        ],
    )?;

    let out = run_install_local(moon_home.path(), module_dir.path(), None)?;
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
    let Some(moon_home) = prepare_moon_home() else {
        return Ok(());
    };
    let module_dir = tempfile::tempdir().context("failed to create module tempdir")?;
    let module_name = "localuser/localmod";
    write_local_module(
        module_dir.path(),
        module_name,
        None,
        &[
            LocalPackageSpec {
                dir: "",
                is_main: true,
                bin_name: "root-tool",
            },
            LocalPackageSpec {
                dir: "tools",
                is_main: true,
                bin_name: "sub-tool",
            },
        ],
    )?;

    let out = run_install_local(
        moon_home.path(),
        module_dir.path(),
        Some("localuser/localmod/tools"),
    )?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "expected failure, got success");
    assert!(
        stderr.contains("package path must be in the form of <author>/<module>[@<version>]"),
        "unexpected stderr: {stderr}"
    );
    Ok(())
}
