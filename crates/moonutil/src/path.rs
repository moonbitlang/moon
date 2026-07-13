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

use anyhow::bail;
use std::fmt::Debug;
use std::{
    fmt::Formatter,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use crate::constants::SUB_PKG_POSTFIX;

/// Return platform spellings that can identify the same path in textual output.
///
/// Windows build graphs may contain both verbatim paths retained internally and
/// legacy paths simplified at external command boundaries. The alternate form
/// is only intended for comparison and normalization; it may not be usable for
/// filesystem access when the path exceeds legacy Windows limits.
pub fn path_spellings_for_comparison(path: &Path) -> Vec<PathBuf> {
    let path = path.to_path_buf();

    #[cfg(windows)]
    if let Some(legacy) = legacy_windows_spelling(&path) {
        // Keep the verbatim form first because it contains the legacy spelling
        // as a suffix when these are used as textual match keys.
        return vec![path, legacy];
    }

    vec![path]
}

#[cfg(windows)]
fn legacy_windows_spelling(path: &Path) -> Option<PathBuf> {
    use std::path::Prefix;

    let mut components = path.components();
    let Component::Prefix(prefix) = components.next()? else {
        return None;
    };
    let mut legacy = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => PathBuf::from(r"\\").join(server).join(share),
        _ => return None,
    };
    legacy.extend(components);
    Some(legacy)
}

#[cfg(windows)]
#[test]
fn comparison_spellings_cover_legacy_and_verbatim_windows_paths() {
    assert_eq!(
        path_spellings_for_comparison(Path::new(r"\\?\C:\workspace\src")),
        [
            PathBuf::from(r"\\?\C:\workspace\src"),
            PathBuf::from(r"C:\workspace\src"),
        ]
    );
    assert_eq!(
        path_spellings_for_comparison(Path::new(r"\\?\UNC\server\share\workspace")),
        [
            PathBuf::from(r"\\?\UNC\server\share\workspace"),
            PathBuf::from(r"\\server\share\workspace"),
        ]
    );
}

#[derive(Clone, Hash)]
pub struct PathComponent {
    pub components: Vec<String>,
}

impl PathComponent {
    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn is_internal(&self) -> bool {
        self.components.iter().any(|x| x == "internal")
    }

    pub fn can_import(&self, other: &PathComponent) -> bool {
        if !other.is_internal() {
            return true;
        }
        let mut i = 0;
        let mut j = 0;
        let internal_index = other
            .components
            .iter()
            .position(|c| *c == "internal")
            .unwrap();

        while i < self.len() && j < internal_index {
            if self.components[i] != other.components[j] {
                return false;
            }
            i += 1;
            j += 1;
        }
        true
    }
}

impl std::fmt::Display for PathComponent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.components.join("/"))
    }
}

#[test]
#[cfg(unix)]
fn test_path_component_1() {
    let pc = PathComponent { components: vec![] };
    assert!(pc.full_name() == "");
    let pc = PathComponent {
        components: vec!["a".into()],
    };
    assert!(pc.full_name() == "a");
    let pc = PathComponent {
        components: vec!["a".into(), "b".into()],
    };
    assert!(pc.full_name() == "a/b");
}

#[test]
fn test_import_component_1() {
    let ic = ImportComponent {
        path: ImportPath {
            module_name: "a/b".into(),
            rel_path: PathComponent { components: vec![] },
            is_3rd: true,
        },
        alias: None,
        sub_package: false,
    };
    assert!(ic.path.make_full_path() == "a/b");
    let ic = ImportComponent {
        path: ImportPath {
            module_name: "a".into(),
            rel_path: PathComponent {
                components: vec!["b".into()],
            },
            is_3rd: true,
        },
        alias: None,
        sub_package: false,
    };
    assert!(ic.path.make_full_path() == "a/b");
}

impl Debug for PathComponent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "#[{}]", self.components.join("|"))
    }
}

impl PathComponent {
    pub fn short_name(&self) -> &str {
        if self.components.is_empty() {
            ""
        } else {
            self.components.last().unwrap()
        }
    }

    pub fn full_name(&self) -> String {
        self.components.join("/")
    }

    pub fn fs_full_name(&self) -> String {
        #[cfg(unix)]
        return self.components.join("/");

        #[cfg(windows)]
        return self.components.join("\\");
    }

    pub fn from_path(p: &Path) -> anyhow::Result<PathComponent> {
        let mut comps = vec![];
        for comp in p.components() {
            match comp {
                Component::Normal(s) => {
                    comps.push(s.to_str().unwrap().to_string());
                }
                _ => {
                    bail!("invalid package path `{:?}`", p)
                }
            }
        }
        Ok(Self { components: comps })
    }
}

impl std::str::FromStr for PathComponent {
    type Err = anyhow::Error;
    // like a/b/c
    fn from_str(p: &str) -> anyhow::Result<PathComponent> {
        let buf = PathBuf::from(p);
        PathComponent::from_path(&buf)
    }
}

#[derive(Clone)]
pub struct ImportPath {
    pub module_name: String,
    pub rel_path: PathComponent,
    pub is_3rd: bool,
}

impl ImportPath {
    pub fn make_full_path(&self) -> String {
        let mut p = self.module_name.clone();
        if !self.rel_path.components.is_empty() {
            p.push('/');
            p.push_str(&self.rel_path.full_name())
        }
        p
    }
}

impl Debug for ImportPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}({}){}",
            if self.is_3rd { "*" } else { "" },
            self.module_name,
            self.rel_path.full_name()
        )
    }
}

#[derive(Clone)]
pub struct ImportComponent {
    pub path: ImportPath,
    pub alias: Option<String>,
    pub sub_package: bool,
}

impl ImportComponent {
    pub fn full_components(&self) -> PathComponent {
        let mut components: Vec<String> = PathBuf::from(&self.path.module_name)
            .components()
            .map(|x| x.as_os_str().to_str().unwrap().to_string())
            .collect();
        components.extend(self.path.rel_path.components.iter().cloned());
        PathComponent { components }
    }

    pub fn make_full_path(&self) -> String {
        if self.sub_package {
            format!("{}{}", self.path.make_full_path(), SUB_PKG_POSTFIX)
        } else {
            self.path.make_full_path()
        }
    }
}

impl Debug for ImportComponent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.alias {
            None => {
                write!(f, "import {:?}", self.path)
            }
            Some(alias) => {
                write!(f, "import {:?} as {}", self.path, alias)
            }
        }
    }
}

#[test]
fn test_internal() {
    let x = PathComponent {
        components: vec!["a".to_string(), "b".to_string()],
    };
    let y = PathComponent {
        components: vec!["a".to_string(), "b".to_string(), "internal".to_string()],
    };
    assert!(x.can_import(&y));

    let x = PathComponent {
        components: vec!["x".to_string(), "y".to_string()],
    };
    let y = PathComponent {
        components: vec!["a".to_string(), "b".to_string(), "internal".to_string()],
    };
    assert!(!x.can_import(&y));
}

// Copy from https://github.com/rust-lang/cargo/blob/e52e360/crates/cargo-test-support/src/paths.rs#L113
pub trait CargoPathExt {
    fn rm_rf(&self);
}

impl CargoPathExt for Path {
    fn rm_rf(&self) {
        let meta = match self.symlink_metadata() {
            Ok(meta) => meta,
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    return;
                }
                panic!("failed to remove {self:?}, could not read: {e:?}");
            }
        };
        // There is a race condition between fetching the metadata and
        // actually performing the removal, but we don't care all that much
        // for our tests.
        if meta.is_dir() {
            if let Err(e) = fs::remove_dir_all(self) {
                panic!("failed to remove {self:?}: {e:?}")
            }
        } else if let Err(e) = fs::remove_file(self) {
            panic!("failed to remove {self:?}: {e:?}")
        }
    }
}

pub fn get_desc_name(package_name: &str, artifact: &str) -> String {
    if artifact.contains("internal_test") {
        format!("{}_{}", package_name, "internal_test")
    } else if artifact.contains("whitebox_test") {
        format!("{}_{}", package_name, "whitebox_test")
    } else {
        package_name.to_string()
    }
}
