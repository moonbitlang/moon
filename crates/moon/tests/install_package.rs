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
use mooncake::registry::calc_sha2;
use tempfile::TempDir;
use walkdir::WalkDir;
use zip::write::FileOptions;

fn moon_bin() -> PathBuf {
    snapbox::cargo_bin!("moon").to_owned()
}

fn git_writes_allowed() -> bool {
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return false,
    };
    std::fs::create_dir(dir.path().join(".git")).is_ok()
}

fn git_available() -> bool {
    which::which("git").is_ok()
}

fn real_moon_home() -> Option<PathBuf> {
    std::env::var_os("MOON_HOME")
        .map(PathBuf::from)
        .or_else(|| home::home_dir().map(|h| h.join(".moon")))
}

fn prepare_moon_home() -> Option<TempDir> {
    if !git_writes_allowed() || !git_available() {
        return None;
    }
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
    std::fs::create_dir_all(temp.path().join("bin")).ok()?;
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

fn run_git(cwd: &Path, args: &[&str]) -> anyhow::Result<()> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run git")?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

fn init_local_registry(
    author: &str,
    pkg: &str,
    version: &str,
    checksum: &str,
) -> anyhow::Result<TempDir> {
    let base = tempfile::tempdir().context("failed to create registry tempdir")?;
    let work = base.path().join("work");
    let index_dir = work.join("user").join(author);
    std::fs::create_dir_all(&index_dir)?;
    let index_path = index_dir.join(format!("{pkg}.index"));
    let index_line =
        format!(r#"{{"name":"{author}/{pkg}","version":"{version}","checksum":"{checksum}"}}"#);
    std::fs::write(&index_path, format!("{index_line}\n"))?;

    run_git(&work, &["init"])?;
    run_git(&work, &["config", "user.email", "ci@example.com"])?;
    run_git(&work, &["config", "user.name", "ci"])?;
    run_git(&work, &["add", "."])?;
    run_git(&work, &["commit", "-m", "init index"])?;

    let bare_parent = base.path().join("git");
    std::fs::create_dir_all(&bare_parent)?;
    run_git(
        &bare_parent,
        &["clone", "--bare", work.to_str().unwrap(), "index"],
    )?;
    Ok(base)
}

fn write_package_zip(
    dest: &Path,
    module_name: &str,
    version: &str,
    bin_name: &str,
    preferred_target: Option<&str>,
    is_main: bool,
) -> anyhow::Result<()> {
    let temp = tempfile::tempdir().context("failed to create package tempdir")?;
    let root = temp.path();

    let mut mod_json = serde_json::json!({
        "name": module_name,
        "version": version,
    });
    if let Some(preferred) = preferred_target {
        mod_json["preferred-target"] = serde_json::json!(preferred);
    }
    std::fs::write(
        root.join("moon.mod.json"),
        serde_json::to_string_pretty(&mod_json)?,
    )?;

    let pkg_json = if is_main {
        serde_json::json!({
            "is-main": true,
            "bin-name": bin_name,
        })
    } else {
        serde_json::json!({
            "import": {},
        })
    };
    std::fs::write(
        root.join("moon.pkg.json"),
        serde_json::to_string_pretty(&pkg_json)?,
    )?;
    std::fs::write(root.join("main.mbt"), "fn main {}\n")?;

    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default();
    for entry in WalkDir::new(root) {
        let entry = entry?;
        let path = entry.path();
        let name = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        if name.is_empty() {
            continue;
        }
        if entry.file_type().is_dir() {
            zip.add_directory(name, options)?;
        } else {
            zip.start_file(name, options)?;
            let mut src = std::fs::File::open(path)?;
            std::io::copy(&mut src, &mut zip)?;
        }
    }
    zip.finish()?;
    Ok(())
}

fn cache_zip(
    moon_home: &Path,
    author: &str,
    pkg: &str,
    version: &str,
    src: &Path,
) -> anyhow::Result<PathBuf> {
    let cache_path = moon_home
        .join("registry")
        .join("cache")
        .join(author)
        .join(pkg)
        .join(format!("{version}.zip"));
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, &cache_path)?;
    Ok(cache_path)
}

fn run_install(
    moon_home: &Path,
    registry_base: &Path,
    package_path: &str,
) -> anyhow::Result<std::process::Output> {
    let out = Command::new(moon_bin())
        .env("MOON_HOME", moon_home)
        .env("MOONCAKES_REGISTRY", registry_base)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(["install", package_path])
        .output()?;
    Ok(out)
}

#[test]
fn test_moon_install_package_to_moon_home_bin() -> anyhow::Result<()> {
    let Some(moon_home) = prepare_moon_home() else {
        return Ok(());
    };
    let author = "testuser";
    let pkg = "installpkg";
    let version = "0.1.0";
    let bin_name = "install-tool";
    let module_name = format!("{author}/{pkg}");

    let zip_path = moon_home.path().join("pkg.zip");
    write_package_zip(&zip_path, &module_name, version, bin_name, None, true)?;
    let checksum = calc_sha2(&zip_path)?;
    let registry_base = init_local_registry(author, pkg, version, &checksum)?;
    cache_zip(moon_home.path(), author, pkg, version, &zip_path)?;

    let out = run_install(
        moon_home.path(),
        registry_base.path(),
        &format!("{author}/{pkg}@{version}"),
    )?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "moon install failed: stdout={} stderr={stderr}",
        String::from_utf8_lossy(&out.stdout)
    );

    let mut expected = bin_name.to_string();
    if cfg!(windows) {
        expected.push_str(".exe");
    }
    let installed = moon_home.path().join("mooncakes_bin").join(expected);
    assert!(
        installed.exists(),
        "expected installed binary at {}",
        installed.display()
    );
    Ok(())
}

#[test]
fn test_moon_install_rejects_non_native_preferred_target() -> anyhow::Result<()> {
    let Some(moon_home) = prepare_moon_home() else {
        return Ok(());
    };
    let author = "testuser";
    let pkg = "prefertarget";
    let version = "0.1.0";
    let bin_name = "prefer-tool";
    let module_name = format!("{author}/{pkg}");

    let zip_path = moon_home.path().join("prefers.zip");
    write_package_zip(
        &zip_path,
        &module_name,
        version,
        bin_name,
        Some("wasm"),
        true,
    )?;
    let checksum = calc_sha2(&zip_path)?;
    let registry_base = init_local_registry(author, pkg, version, &checksum)?;
    cache_zip(moon_home.path(), author, pkg, version, &zip_path)?;

    let out = run_install(
        moon_home.path(),
        registry_base.path(),
        &format!("{author}/{pkg}@{version}"),
    )?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "expected failure, got success");
    assert!(
        stderr.contains("prefers target `wasm`"),
        "unexpected stderr: {stderr}"
    );
    Ok(())
}

#[test]
fn test_moon_install_requires_main_package() -> anyhow::Result<()> {
    let Some(moon_home) = prepare_moon_home() else {
        return Ok(());
    };
    let author = "testuser";
    let pkg = "nomains";
    let version = "0.1.0";
    let bin_name = "no-main";
    let module_name = format!("{author}/{pkg}");

    let zip_path = moon_home.path().join("nomains.zip");
    write_package_zip(&zip_path, &module_name, version, bin_name, None, false)?;
    let checksum = calc_sha2(&zip_path)?;
    let registry_base = init_local_registry(author, pkg, version, &checksum)?;
    cache_zip(moon_home.path(), author, pkg, version, &zip_path)?;

    let out = run_install(
        moon_home.path(),
        registry_base.path(),
        &format!("{author}/{pkg}@{version}"),
    )?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "expected failure, got success");
    assert!(
        stderr.contains("no `is_main` packages found"),
        "unexpected stderr: {stderr}"
    );
    Ok(())
}
