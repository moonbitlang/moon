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

//! Moon home and selected toolchain layouts.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use semver::Version;

use crate::{
    constants::{BUILD_DIR, PRELUDE_PROOF_DIR},
    resolution::ModuleName,
    target::TargetBackend,
};

/// Paths owned by one Moon home directory.
#[derive(Clone, Debug)]
pub struct MoonHomeLayout {
    root: PathBuf,
}

impl MoonHomeLayout {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// User-installed executables.
    ///
    /// In the default self-contained installation this is also the selected
    /// toolchain's `bin` directory. Package-managed toolchains may live under
    /// a separate root.
    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    /// Default cache of resolved dependency source trees.
    pub fn dependency_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("deps")
    }

    /// Default cache of build artifacts.
    pub fn build_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("build")
    }

    /// Registry metadata and downloaded content.
    pub fn registry_dir(&self) -> PathBuf {
        self.root.join("registry")
    }

    /// Local Git checkout of the registry index.
    pub fn registry_index_dir(&self) -> PathBuf {
        self.registry_dir().join("index")
    }

    /// One module's JSON-lines entry in the local registry index checkout.
    pub fn registry_index_file(&self, name: &ModuleName) -> PathBuf {
        self.registry_index_dir()
            .join("user")
            .join(name.username.as_str())
            .join(format!("{}.index", name.unqual))
    }

    /// Verified registry downloads, including package and executable archives.
    pub fn registry_cache_dir(&self) -> PathBuf {
        self.registry_dir().join("cache")
    }

    /// One verified module source archive.
    pub fn registry_source_archive_path(&self, name: &ModuleName, version: &Version) -> PathBuf {
        self.registry_cache_dir()
            .join(name.username.as_str())
            .join(name.unqual.as_str())
            .join(format!("{version}.zip"))
    }

    /// Directory containing cached executable artifacts.
    pub fn registry_assets_dir(&self) -> PathBuf {
        self.registry_cache_dir().join("assets")
    }

    /// Directory for one registry package's cached executable artifacts.
    pub fn registry_package_assets_dir(
        &self,
        name: &ModuleName,
        version: &Version,
        package_path: &str,
    ) -> PathBuf {
        let mut path = self.registry_assets_dir().join(name.username.as_str());
        path.extend(name.unqual.split('/'));
        path.push(version.to_string());
        path.extend(
            package_path
                .split('/')
                .filter(|segment| !segment.is_empty()),
        );
        path
    }

    /// One cached executable artifact published for a registry package.
    pub fn registry_executable_artifact_path(
        &self,
        name: &ModuleName,
        version: &Version,
        package_path: &str,
        artifact_name: &str,
    ) -> PathBuf {
        self.registry_package_assets_dir(name, version, package_path)
            .join(artifact_name)
    }

    /// Materialized symbol metadata downloaded during registry sync.
    pub fn registry_symbols_dir(&self) -> PathBuf {
        self.registry_dir().join("symbols")
    }

    /// State used to coalesce concurrent registry updates.
    pub fn registry_update_state_path(&self) -> PathBuf {
        self.registry_dir().join(".registry-update-state.json")
    }

    /// Moon-owned global caches selected by `MOON_DEP_CACHE` and
    /// `MOON_BUILD_CACHE` when those variables are unset.
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn credentials_path(&self) -> PathBuf {
        self.root.join("credentials.json")
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }
}

pub struct MoonDirs {
    pub moon_include_path: PathBuf,
    pub moon_lib_path: PathBuf,
    pub moon_bin_path: PathBuf,
    pub internal_tcc_path: PathBuf,
}

pub static MOON_HOME: LazyLock<MoonHomeLayout> =
    LazyLock::new(|| MoonHomeLayout::new(resolve_moon_home()));
static TOOLCHAIN_ROOT: LazyLock<PathBuf> = LazyLock::new(resolve_toolchain_root);

pub static MOON_DIRS: LazyLock<MoonDirs> = LazyLock::new(|| {
    let toolchain_root = toolchain_root();
    let moon_include_path = toolchain_root.join("include");
    let moon_lib_path = toolchain_root.join("lib");
    let moon_bin_path = toolchain_root.join("bin");
    let internal_tcc_path = moon_bin_path.join("internal").join("tcc");
    MoonDirs {
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

fn resolve_moon_home() -> PathBuf {
    std::env::var_os("MOON_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let Some(home) = home::home_dir() else {
                eprintln!("Failed to get home directory");
                std::process::exit(1);
            };
            home.join(".moon")
        })
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

    MOON_HOME.root().to_path_buf()
}

pub fn toolchain_root() -> PathBuf {
    TOOLCHAIN_ROOT.clone()
}

pub fn bin() -> PathBuf {
    toolchain_root().join("bin")
}

pub fn include() -> PathBuf {
    toolchain_root().join("include")
}

pub fn lib() -> PathBuf {
    toolchain_root().join("lib")
}

pub fn prelude_proof() -> PathBuf {
    lib().join(PRELUDE_PROOF_DIR)
}

pub fn share() -> PathBuf {
    toolchain_root().join("share")
}

pub fn why3_datadir() -> PathBuf {
    share().join("why3")
}

pub fn why3_libdir() -> PathBuf {
    lib().join("why3")
}

pub fn core() -> PathBuf {
    let env_var = std::env::var_os("MOON_CORE_OVERRIDE");
    if let Some(path) = env_var {
        return PathBuf::from(path);
    }
    lib().join("core")
}

pub fn core_bundle_in(core_root: &Path, backend: TargetBackend) -> PathBuf {
    core_root
        .join(BUILD_DIR)
        .join(backend.to_dir_name())
        .join("release")
        .join("bundle")
}

pub fn core_bundle(backend: TargetBackend) -> PathBuf {
    core_bundle_in(&core(), backend)
}

pub fn abort_core_in(core_root: &Path, backend: TargetBackend) -> PathBuf {
    core_bundle_in(core_root, backend)
        .join("abort")
        .join("abort.core")
}

pub fn core_core_in(core_root: &Path, backend: TargetBackend) -> PathBuf {
    core_bundle_in(core_root, backend).join("core.core")
}

pub fn core_package_mi_in(
    core_root: &Path,
    backend: TargetBackend,
    package_path: &str,
    package_last_segment: &str,
) -> PathBuf {
    core_bundle_in(core_root, backend)
        .join(package_path)
        .join(format!("{package_last_segment}.mi"))
}

// core.core & abort.core(virtual pkg default impl)
pub fn core_core(backend: TargetBackend) -> Vec<String> {
    vec![
        abort_core_in(&core(), backend).display().to_string(),
        core_core_in(&core(), backend).display().to_string(),
    ]
}

#[test]
fn derives_paths_from_one_home_root() {
    let layout = MoonHomeLayout::new(PathBuf::from("moon-home"));

    assert_eq!(layout.root(), Path::new("moon-home"));
    assert_eq!(layout.bin_dir(), Path::new("moon-home/bin"));
    assert_eq!(
        layout.dependency_cache_dir(),
        Path::new("moon-home/cache/deps")
    );
    assert_eq!(layout.build_cache_dir(), Path::new("moon-home/cache/build"));
    assert_eq!(layout.registry_dir(), Path::new("moon-home/registry"));
    assert_eq!(
        layout.registry_index_dir(),
        Path::new("moon-home/registry/index")
    );
    assert_eq!(
        layout.registry_cache_dir(),
        Path::new("moon-home/registry/cache")
    );
    let name: ModuleName = "moonbitlang/parser".parse().unwrap();
    let version = Version::new(0, 3, 3);
    assert_eq!(
        layout.registry_index_file(&name),
        Path::new("moon-home/registry/index/user/moonbitlang/parser.index")
    );
    assert_eq!(
        layout.registry_source_archive_path(&name, &version),
        Path::new("moon-home/registry/cache/moonbitlang/parser/0.3.3.zip")
    );
    assert_eq!(
        layout.registry_executable_artifact_path(&name, &version, "cmd/moonfmt", "moonfmt.wasm"),
        Path::new(
            "moon-home/registry/cache/assets/moonbitlang/parser/0.3.3/cmd/moonfmt/moonfmt.wasm"
        )
    );
    assert_eq!(
        layout.registry_symbols_dir(),
        Path::new("moon-home/registry/symbols")
    );
    assert_eq!(
        layout.registry_update_state_path(),
        Path::new("moon-home/registry/.registry-update-state.json")
    );
    assert_eq!(layout.cache_dir(), Path::new("moon-home/cache"));
    assert_eq!(
        layout.credentials_path(),
        Path::new("moon-home/credentials.json")
    );
    assert_eq!(layout.config_path(), Path::new("moon-home/config.json"));
}

#[test]
fn test_moon_dir() {
    use expect_test::expect;

    let toolchain_dirs = [
        bin(),
        include(),
        lib(),
        prelude_proof(),
        why3_datadir(),
        why3_libdir(),
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
            "lib|prelude_proof",
            "share|why3",
            "lib|why3",
            "lib|core|_build|wasm|release|bundle",
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
