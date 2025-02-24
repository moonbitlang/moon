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

use anyhow::Context;

use crate::common::TargetBackend;

pub fn home() -> PathBuf {
    if let Ok(moon_home) = std::env::var("MOON_HOME") {
        return PathBuf::from(moon_home);
    }

    let h = home::home_dir();
    if h.is_none() {
        eprintln!("Failed to get home directory");
        std::process::exit(1);
    }
    let hm = h.unwrap().join(".moon");
    if !hm.exists() {
        std::fs::create_dir_all(&hm).unwrap();
    }
    hm
}

pub fn bin() -> PathBuf {
    let bin = home().join("bin");
    if !bin.exists() {
        std::fs::create_dir_all(&bin).unwrap();
    }
    bin
}

pub fn lib() -> PathBuf {
    let lib = home().join("lib");
    if !lib.exists() {
        std::fs::create_dir_all(&lib).unwrap();
    }
    lib
}

pub fn core() -> PathBuf {
    let env_var = std::env::var("MOON_CORE_OVERRIDE");
    if let Ok(path) = env_var {
        return PathBuf::from(path);
    }
    home().join("lib").join("core")
}

pub fn core_bundle(backend: TargetBackend) -> PathBuf {
    core()
        .join("target")
        .join(backend.to_dir_name())
        .join("release")
        .join("bundle")
}

pub fn core_packages_list(backend: TargetBackend) -> PathBuf {
    core()
        .join("target")
        .join(backend.to_dir_name())
        .join("release")
        .join("bundle")
        .join("packages.json")
}

pub fn core_core(backend: TargetBackend) -> PathBuf {
    core()
        .join("target")
        .join(backend.to_dir_name())
        .join("release")
        .join("bundle")
        .join("core.core")
}

pub fn cache() -> PathBuf {
    home().join("registry").join("cache")
}

pub fn index() -> PathBuf {
    home().join("registry").join("index")
}

/// Get the path of the index file of a package. [`base`] should be the path of
/// the index directory, for example, returned from [`index()`].
pub fn index_of_pkg(base: &Path, user: &str, pkg: &str) -> PathBuf {
    base.join("user")
        .join(user)
        .join(pkg)
        .with_extension("index")
}

pub fn credentials_json() -> PathBuf {
    home().join("credentials.json")
}

pub fn config_json() -> PathBuf {
    home().join("config.json")
}

pub fn moon_tmp_dir() -> anyhow::Result<PathBuf> {
    let p = home().join("tmp");
    if !p.exists() {
        std::fs::create_dir_all(&p)
            .with_context(|| format!("failed to create tmp directory `{}`", p.display()))?;
    }
    Ok(p)
}

pub fn git_dir() -> PathBuf {
    home().join("git")
}

pub fn git_repos_dir() -> PathBuf {
    git_dir().join("repos")
}

pub fn git_checkouts_dir() -> PathBuf {
    git_dir().join("checkouts")
}

#[test]
fn test_moon_dir() {
    use expect_test::expect;

    let dirs = [
        home(),
        core_bundle(TargetBackend::default()),
        cache(),
        index(),
        credentials_json(),
        config_json(),
        moon_tmp_dir().unwrap(),
    ];
    dbg!(&dirs);
    let dirs = dirs
        .iter()
        .map(|p| {
            p.strip_prefix(home())
                .unwrap()
                .to_str()
                .unwrap()
                .replace(['\\', '/'], "|")
        })
        .collect::<Vec<_>>();
    expect![[r#"
        [
            "",
            "lib|core|target|wasm-gc|release|bundle",
            "registry|cache",
            "registry|index",
            "credentials.json",
            "config.json",
            "tmp",
        ]
    "#]]
    .assert_debug_eq(&dirs);
}
