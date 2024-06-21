use std::path::{Path, PathBuf};

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
        ]
    "#]]
    .assert_debug_eq(&dirs);
}
