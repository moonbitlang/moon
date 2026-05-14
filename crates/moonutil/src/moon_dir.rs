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
use std::sync::LazyLock;

use crate::common::{BUILD_DIR, TargetBackend};

pub struct MoonDirs {
    pub moon_home: PathBuf,
    pub moon_include_path: PathBuf,
    pub moon_lib_path: PathBuf,
    pub moon_bin_path: PathBuf,
    pub internal_tcc_path: PathBuf,
}

static MOON_HOME: LazyLock<PathBuf> = LazyLock::new(resolve_home);
static TOOLCHAIN_ROOT: LazyLock<PathBuf> = LazyLock::new(resolve_toolchain_root);

pub static MOON_DIRS: LazyLock<MoonDirs> = LazyLock::new(|| {
    let moon_home = home();
    let toolchain_root = toolchain_root();
    let moon_include_path = toolchain_root.join("include");
    let moon_lib_path = toolchain_root.join("lib");
    let moon_bin_path = toolchain_root.join("bin");
    let internal_tcc_path = moon_bin_path.join("internal").join("tcc");
    MoonDirs {
        moon_home,
        moon_include_path,
        moon_lib_path,
        moon_bin_path,
        internal_tcc_path,
    }
});

pub fn is_toolchain_root(root: &Path) -> bool {
    root.join("include").is_dir()
        && root.join("lib").join("core").is_dir()
        && root
            .join("bin")
            .join(format!("moonc{}", std::env::consts::EXE_SUFFIX))
            .is_file()
}

fn infer_toolchain_root_from_exe(current_exe: &Path) -> Option<PathBuf> {
    let current_exe =
        dunce::canonicalize(current_exe).unwrap_or_else(|_| current_exe.to_path_buf());
    let bin_dir = current_exe.parent()?;
    if bin_dir.file_name().is_none_or(|name| name != "bin") {
        return None;
    }
    let root = bin_dir.parent()?;
    if !is_toolchain_root(root) {
        return None;
    }
    Some(root.to_path_buf())
}

fn resolve_toolchain_root() -> PathBuf {
    if let Some(path) = std::env::var_os("MOON_TOOLCHAIN_ROOT") {
        return PathBuf::from(path);
    }

    if let Ok(current_exe) = std::env::current_exe()
        && let Some(root) = infer_toolchain_root_from_exe(&current_exe)
    {
        return root;
    }

    home()
}

fn resolve_home() -> PathBuf {
    if let Some(moon_home) = std::env::var_os("MOON_HOME") {
        PathBuf::from(moon_home)
    } else {
        let Some(h) = home::home_dir() else {
            eprintln!("Failed to get home directory");
            std::process::exit(1);
        };
        h.join(".moon")
    }
}

pub fn toolchain_root() -> PathBuf {
    TOOLCHAIN_ROOT.clone()
}

pub fn home() -> PathBuf {
    MOON_HOME.clone()
}

pub fn bin() -> PathBuf {
    toolchain_root().join("bin")
}

pub fn user_bin() -> PathBuf {
    home().join("bin")
}

pub fn include() -> PathBuf {
    toolchain_root().join("include")
}

pub fn lib() -> PathBuf {
    toolchain_root().join("lib")
}

pub fn core() -> PathBuf {
    let env_var = std::env::var_os("MOON_CORE_OVERRIDE");
    if let Some(path) = env_var {
        return PathBuf::from(path);
    }
    lib().join("core")
}

pub fn core_bundle(backend: TargetBackend) -> PathBuf {
    core()
        .join(BUILD_DIR)
        .join(backend.to_dir_name())
        .join("release")
        .join("bundle")
}

// core.core & abort.core(virtual pkg default impl)
pub fn core_core(backend: TargetBackend) -> Vec<String> {
    vec![
        core_bundle(backend)
            .join("abort")
            .join("abort.core")
            .display()
            .to_string(),
        core_bundle(backend).join("core.core").display().to_string(),
    ]
}

pub fn cache() -> PathBuf {
    home().join("registry").join("cache")
}

/// Reserved binary names that cannot be overwritten by user-installed packages.
pub const RESERVED_BIN_NAMES: &[&str] = &[
    "moon",
    "moonc",
    "mooncake",
    "moondoc",
    "moonfmt",
    "mooninfo",
    "moonrun",
    "moon_cove_report",
    "moon-ide",
    "moon-lsp",
    "moon-wasm-opt",
    "moonbit-lsp",
];

pub fn index() -> PathBuf {
    home().join("registry").join("index")
}

pub fn credentials_json() -> PathBuf {
    home().join("credentials.json")
}

pub fn config_json() -> PathBuf {
    home().join("config.json")
}

#[test]
fn test_moon_dir() {
    use expect_test::expect;

    let home_dirs = [
        home(),
        user_bin(),
        cache(),
        index(),
        credentials_json(),
        config_json(),
    ];
    dbg!(&home_dirs);
    let home_dirs = home_dirs
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
            "bin",
            "registry|cache",
            "registry|index",
            "credentials.json",
            "config.json",
        ]
    "#]]
    .assert_debug_eq(&home_dirs);

    let toolchain_dirs = [
        bin(),
        include(),
        lib(),
        core_bundle(TargetBackend::default()),
    ];
    dbg!(&toolchain_dirs);
    let toolchain_dirs = toolchain_dirs
        .iter()
        .map(|p| {
            p.strip_prefix(toolchain_root())
                .unwrap()
                .to_str()
                .unwrap()
                .replace(['\\', '/'], "|")
        })
        .collect::<Vec<_>>();
    expect![[r#"
        [
            "bin",
            "include",
            "lib",
            "lib|core|_build|wasm-gc|release|bundle",
        ]
    "#]]
    .assert_debug_eq(&toolchain_dirs);
}

#[test]
fn detects_toolchain_root_shape() {
    let dir = std::env::temp_dir().join(format!(
        "moonutil-toolchain-root-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::create_dir_all(dir.join("include")).unwrap();
    std::fs::create_dir_all(dir.join("lib").join("core")).unwrap();
    std::fs::write(
        dir.join("bin")
            .join(format!("moonc{}", std::env::consts::EXE_SUFFIX)),
        [],
    )
    .unwrap();

    assert!(is_toolchain_root(&dir));
    assert!(!is_toolchain_root(dir.parent().unwrap()));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn infers_toolchain_root_from_exe_only_for_valid_toolchain_layout() {
    let dir = std::env::temp_dir().join(format!(
        "moonutil-infer-toolchain-root-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::create_dir_all(dir.join("include")).unwrap();
    std::fs::create_dir_all(dir.join("lib").join("core")).unwrap();
    std::fs::write(
        dir.join("bin")
            .join(format!("moonc{}", std::env::consts::EXE_SUFFIX)),
        [],
    )
    .unwrap();

    let moon = dir
        .join("bin")
        .join(format!("moon{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&moon, []).unwrap();
    assert_eq!(
        infer_toolchain_root_from_exe(&moon).unwrap(),
        dunce::canonicalize(&dir).unwrap()
    );

    let invalid_root = std::env::temp_dir().join(format!(
        "moonutil-invalid-toolchain-root-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&invalid_root);
    std::fs::create_dir_all(invalid_root.join("bin")).unwrap();
    let loose_moon = invalid_root
        .join("bin")
        .join(format!("moon{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&loose_moon, []).unwrap();
    assert_eq!(infer_toolchain_root_from_exe(&loose_moon), None);

    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&invalid_root).unwrap();
}
