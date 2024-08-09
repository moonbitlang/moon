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

use crate::common::MooncOpt;
use crate::common::MOON_PKG_JSON;
use crate::dependency::{DependencyInfo, DependencyInfoJson};
use crate::package::{AliasJSON, Package, PackageJSON};
use crate::path::ImportPath;
use anyhow::bail;
use indexmap::map::IndexMap;
use petgraph::graph::DiGraph;
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ModuleDB {
    pub source_dir: PathBuf,
    pub name: String,
    pub packages: IndexMap<String, Package>,
    pub entries: Vec<usize>, // index of entry packages
    pub deps: Vec<String>,
    pub graph: DiGraph<String, usize>,
    pub backend: String,
    pub source: Option<String>,
}

impl ModuleDB {
    pub fn make_pkg_import_path(&self, pkg_idx: usize) -> String {
        let pkg = &self.packages[pkg_idx];

        let p = ImportPath {
            module_name: self.name.clone(),
            rel_path: pkg.rel.clone(),
            is_3rd: false,
        };

        p.make_full_path()
    }

    pub fn get_package_dir(&self, index: usize) -> PathBuf {
        self.source_dir
            .join(self.packages[index].rel.fs_full_name())
    }

    pub fn make_pkg_core_path(&self, target_dir: &Path, pkg_full_name: &str) -> PathBuf {
        let pkg = &self.packages[pkg_full_name];
        target_dir
            .join(pkg.rel.fs_full_name())
            .join(format!("{}.core", pkg.rel.short_name()))
    }

    pub fn make_pkg_mi_path(&self, target_dir: &Path, pkg_idx: usize) -> PathBuf {
        let pkg = &self.packages[pkg_idx];
        target_dir
            .join(pkg.rel.fs_full_name())
            .join(format!("{}.mi", pkg.rel.short_name()))
    }

    pub fn get_pkg_mi_deps(&self, target_dir: &Path, pkg_idx: usize) -> Vec<String> {
        let mut deps: Vec<String> = vec![];
        let pkg = &self.packages[pkg_idx];
        for dep in pkg.imports.iter() {
            let mi_path = target_dir
                .join(dep.path.make_rel_path())
                .join(format!("{}.mi", dep.path.rel_path.short_name()));

            deps.push(mi_path.display().to_string());
        }
        deps
    }

    pub fn get_pkg_mi_deps_with_alias(&self, target_dir: &Path, pkg_idx: usize) -> Vec<String> {
        let mut deps: Vec<String> = vec![];
        let pkg = &self.packages[pkg_idx];
        for dep in pkg.imports.iter() {
            let alias = if let Some(a) = &dep.alias {
                a.clone()
            } else {
                dep.path.rel_path.short_name().into()
            };
            let mi_path = target_dir
                .join(dep.path.make_rel_path())
                .join(format!("{}.mi", dep.path.rel_path.short_name()));

            deps.push(format!("{}:{}", mi_path.display(), alias));
        }
        deps
    }

    pub fn make_output_path(
        &self,
        target_dir: &Path,
        pkg_idx: usize,
        moonc_opt: &MooncOpt,
    ) -> PathBuf {
        let pkg = &self.packages[pkg_idx];
        target_dir.join(pkg.rel.fs_full_name()).join(format!(
            "{}.{}",
            pkg.rel.short_name(),
            moonc_opt.link_opt.output_format.to_str()
        ))
    }

    fn get_core_dep_rec(
        &self,
        visited: &mut HashSet<String>,
        target_dir: &Path,
        pkg_full_name: &str,
        cores: &mut Vec<PathBuf>,
    ) {
        if visited.contains(pkg_full_name) {
            return;
        }
        visited.insert(pkg_full_name.into());
        let c = self.make_pkg_core_path(target_dir, pkg_full_name);
        cores.push(c);
        let pkg = &self.packages[pkg_full_name];
        for d in pkg.imports.iter() {
            let pkgname = d.path.make_full_path();
            if self.packages.contains_key(&pkgname) {
                self.get_core_dep_rec(visited, target_dir, &pkgname, cores);
            }
        }
    }

    pub fn get_all_dep_cores(&self, target_dir: &Path, pkg_full_name: &str) -> Vec<PathBuf> {
        let mut cores = vec![];
        let mut visited = HashSet::<String>::new();
        self.get_core_dep_rec(&mut visited, target_dir, pkg_full_name, &mut cores);
        cores.sort();
        cores.dedup();
        cores
    }
}

impl ModuleDB {
    pub fn validate(&self) -> anyhow::Result<()> {
        let mut errors = vec![];
        for (_, pkg) in &self.packages {
            for item in pkg
                .imports
                .iter()
                .chain(pkg.wbtest_imports.iter())
                .chain(pkg.test_imports.iter())
            {
                let imported = item.path.make_full_path();
                if !pkg.full_components().can_import(&item.full_components()) {
                    errors.push(format!(
                        "{}: cannot import internal package `{}` in `{}`",
                        self.source_dir
                            .join(pkg.rel.fs_full_name())
                            .join(MOON_PKG_JSON)
                            .display(),
                        imported,
                        pkg.full_name()
                    ))
                }
                if !self.packages.contains_key(&imported) {
                    errors.push(format!(
                        "{}: cannot import `{}` in `{}`, no such package",
                        self.source_dir
                            .join(pkg.rel.fs_full_name())
                            .join(MOON_PKG_JSON)
                            .display(),
                        imported,
                        pkg.full_name(),
                    ));
                }
            }
        }
        if !errors.is_empty() {
            bail!("{}", errors.join("\n"));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ModuleDBJSON {
    pub source_dir: String,
    pub name: String,
    pub packages: Vec<PackageJSON>,
    pub deps: Vec<String>,
    pub backend: String,
    pub source: Option<String>,
}

pub fn convert_mdb_to_json(module: &ModuleDB) -> ModuleDBJSON {
    let mut pkgs = vec![];
    for (_, pkg) in &module.packages {
        let files = pkg.files.iter().map(|f| f.display().to_string()).collect();
        let wbtest_files = pkg
            .wbtest_files
            .iter()
            .map(|f| f.display().to_string())
            .collect();
        let test_files = pkg
            .test_files
            .iter()
            .map(|f| f.display().to_string())
            .collect();
        let mut deps = vec![];
        for dep in &pkg.imports {
            let alias = match &dep.alias {
                None => {
                    let alias = dep.path.rel_path.components.last();
                    match alias {
                        None => dep.path.module_name.split('/').last().unwrap().to_string(),
                        Some(x) => x.to_string(),
                    }
                }
                Some(x) => x.to_string(),
            };
            deps.push(AliasJSON {
                path: dep.path.make_full_path(),
                alias,
            });
        }

        let mut wbtest_deps = vec![];
        for dep in &pkg.wbtest_imports {
            let alias = match &dep.alias {
                None => {
                    let alias = dep.path.rel_path.components.last();
                    match alias {
                        None => dep.path.module_name.split('/').last().unwrap().to_string(),
                        Some(x) => x.to_string(),
                    }
                }
                Some(x) => x.to_string(),
            };
            wbtest_deps.push(AliasJSON {
                path: dep.path.make_full_path(),
                alias,
            });
        }

        let mut test_deps = vec![];
        for dep in &pkg.test_imports {
            let alias = match &dep.alias {
                None => {
                    let alias = dep.path.rel_path.components.last();
                    match alias {
                        None => dep.path.module_name.split('/').last().unwrap().to_string(),
                        Some(x) => x.to_string(),
                    }
                }
                Some(x) => x.to_string(),
            };
            test_deps.push(AliasJSON {
                path: dep.path.make_full_path(),
                alias,
            });
        }

        pkgs.push(PackageJSON {
            is_main: pkg.is_main,
            is_third_party: pkg.is_third_party,
            root: pkg.root.full_name(),
            rel: pkg.rel.full_name(),
            files,
            wbtest_files,
            test_files,
            deps,
            wbtest_deps,
            test_deps,
            artifact: pkg
                .artifact
                .with_extension("mi")
                .to_str()
                .unwrap()
                .to_string(),
        })
    }
    let mut deps = vec![];
    for dep in &module.deps {
        deps.push(dep.clone());
    }
    ModuleDBJSON {
        source_dir: module.source_dir.display().to_string(),
        name: module.name.clone(),
        packages: pkgs,
        deps,
        backend: module.backend.clone(),
        source: module.source.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoonMod {
    pub name: String,
    pub version: Option<Version>,
    pub deps: IndexMap<String, DependencyInfo>,
    pub readme: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub description: Option<String>,

    pub compile_flags: Option<Vec<String>>,
    pub link_flags: Option<Vec<String>>,
    pub checksum: Option<String>,
    pub source: Option<String>,

    /// Fields not covered by the info above, which should be left as-is.
    #[serde(flatten)]
    pub ext: serde_json_lenient::Value,

    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(
    title = "JSON schema for MoonBit moon.mod.json files",
    description = "A module of MoonBit lang"
)]
pub struct MoonModJSON {
    /// name of the module
    pub name: String,

    /// version of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub version: Option<Version>,

    /// third-party dependencies of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashMap<String, String>>")]
    pub deps: Option<IndexMap<String, DependencyInfoJson>>,

    /// path to module's README file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,

    /// url to module's repository
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// license of this module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// keywords of this module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,

    /// description of this module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// custom compile flags
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub compile_flags: Option<Vec<String>>,

    /// custom link flags
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub link_flags: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub checksum: Option<String>,

    /// source code directory of this module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "root-dir")]
    pub source: Option<String>,

    /// Fields not covered by the info above, which should be left as-is.
    #[serde(flatten)]
    #[schemars(skip)]
    pub ext: serde_json_lenient::Value,

    /// Warn list setting of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn_list: Option<String>,

    /// Alert list setting of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_list: Option<String>,
}

impl From<MoonModJSON> for MoonMod {
    fn from(j: MoonModJSON) -> Self {
        let deps = match j.deps {
            None => IndexMap::new(),
            Some(d) => d.into_iter().map(|(k, v)| (k, v.into())).collect(),
        };

        MoonMod {
            name: j.name,
            version: j.version,
            deps,
            readme: j.readme,
            repository: j.repository,
            license: j.license,
            keywords: j.keywords,
            description: j.description,

            compile_flags: j.compile_flags,
            link_flags: j.link_flags,
            checksum: j.checksum,
            source: j.source,
            ext: j.ext,

            alert_list: j.alert_list,
            warn_list: j.warn_list,
        }
    }
}

pub fn convert_module_to_mod_json(m: MoonMod) -> MoonModJSON {
    MoonModJSON {
        name: m.name,
        version: m.version,
        deps: Some(m.deps.into_iter().map(|(k, v)| (k, v.into())).collect()),
        readme: m.readme,
        repository: m.repository,
        license: m.license,
        keywords: m.keywords,
        description: m.description,

        compile_flags: m.compile_flags,
        link_flags: m.link_flags,
        checksum: m.checksum,
        source: m.source,
        ext: m.ext,

        alert_list: m.alert_list,
        warn_list: m.warn_list,
    }
}

impl From<MoonMod> for MoonModJSON {
    fn from(val: MoonMod) -> Self {
        convert_module_to_mod_json(val)
    }
}

#[test]
fn validate_mod_json_schema() {
    let schema = schemars::schema_for!(MoonModJSON);
    let actual = &serde_json_lenient::to_string_pretty(&schema).unwrap();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../moonbuild/template/mod.schema.json"
    );
    expect_test::expect_file![path].assert_eq(actual);
}
