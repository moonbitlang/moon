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
    path::{Component, Path, PathBuf},
};

#[derive(Clone)]
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

    pub fn make_rel_path(&self) -> String {
        self.rel_path.full_name()
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
