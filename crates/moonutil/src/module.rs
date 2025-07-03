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

use crate::common::{
    MoonModJSONFormatErrorKind, MooncOpt, NameError, TargetBackend, MOON_PKG_JSON,
};
use crate::dependency::{
    BinaryDependencyInfo, BinaryDependencyInfoJson, SourceDependencyInfo, SourceDependencyInfoJson,
};
use crate::package::{AliasJSON, Package, PackageJSON};
use crate::path::ImportPath;
use anyhow::bail;
use indexmap::map::IndexMap;
use petgraph::graph::DiGraph;
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ModuleDB {
    pub source_dir: PathBuf,
    pub name: String,
    packages: IndexMap<String, Package>,
    pub entries: Vec<usize>, // index of entry packages
    pub deps: Vec<String>,
    pub graph: DiGraph<String, usize>,
    pub backend: String,
    pub opt_level: String,
    pub source: Option<String>,
}

impl ModuleDB {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_dir: PathBuf,
        name: String,
        package: IndexMap<String, Package>,
        entries: Vec<usize>,
        deps: Vec<String>,
        graph: DiGraph<String, usize>,
        backend: String,
        opt_level: String,
        source: Option<String>,
    ) -> Self {
        ModuleDB {
            source_dir,
            name,
            packages: package,
            entries,
            deps,
            graph,
            backend,
            opt_level,
            source,
        }
    }

    pub fn get_all_packages(&self) -> &IndexMap<String, Package> {
        &self.packages
    }

    pub fn get_all_packages_mut(&mut self) -> &mut IndexMap<String, Package> {
        &mut self.packages
    }

    #[track_caller]
    pub fn get_package_by_name(&self, name: &str) -> &Package {
        self.packages.get(name).unwrap()
    }

    pub fn get_package_by_name_safe(&self, name: &str) -> Option<&Package> {
        self.packages.get(name)
    }

    pub fn get_package_by_name_mut_safe(&mut self, name: &str) -> Option<&mut Package> {
        self.packages.get_mut(name)
    }

    pub fn get_package_by_path(&self, path: &Path) -> Option<&Package> {
        self.packages.values().find(|it| it.root_path == path)
    }

    fn get_entry_pkgs(&self) -> Vec<&Package> {
        let mut dependent_pkgs = HashSet::<String>::new();

        for (_, pkg) in self.packages.iter() {
            let mut deps = HashSet::new();
            self.resolve_deps_of_pkg(pkg, &mut deps);
            for dep in deps.iter() {
                dependent_pkgs.insert(dep.clone());
            }
        }

        self.packages
            .iter()
            .filter(|(_, pkg)| !dependent_pkgs.contains(&pkg.full_name()))
            .map(|(_, pkg)| pkg)
            .collect()
    }

    fn backtrace_deps_chain(&self, pkg: &Package) -> Vec<Vec<String>> {
        let mut cache = HashMap::new();
        let mut visited = HashSet::new();

        fn dfs(
            mdb: &ModuleDB,
            pkg: &Package,
            cache: &mut HashMap<String, Vec<Vec<String>>>,
            visited: &mut HashSet<String>,
        ) -> Vec<Vec<String>> {
            let pkg_name = pkg.full_name();

            if let Some(cached) = cache.get(&pkg_name) {
                return cached.clone();
            }

            if visited.contains(&pkg_name) {
                return vec![];
            }
            visited.insert(pkg_name.clone());

            let all_deps = pkg
                .imports
                .iter()
                .chain(pkg.wbtest_imports.iter())
                .chain(pkg.test_imports.iter());

            let mut paths = Vec::new();
            let has_deps = all_deps.clone().count() > 0;

            for dep in all_deps {
                let dep_name = dep.path.make_full_path();
                let dep_pkg = mdb.get_package_by_name(&dep_name);

                for mut subpath in dfs(mdb, dep_pkg, cache, visited) {
                    let mut new_path = vec![pkg_name.clone()];
                    new_path.append(&mut subpath);
                    paths.push(new_path);
                }
            }

            if !has_deps || paths.is_empty() {
                paths.push(vec![pkg_name.clone()]);
            }

            visited.remove(&pkg_name);
            cache.insert(pkg_name, paths.clone());
            paths
        }

        let mut result = dfs(self, pkg, &mut cache, &mut visited);

        result.retain(|path| path.len() > 1);
        result
    }

    pub fn get_project_supported_targets(
        &self,
        _cur_target_backend: TargetBackend,
    ) -> anyhow::Result<HashSet<TargetBackend>> {
        let mut project_supported_targets = HashSet::from_iter(vec![
            TargetBackend::WasmGC,
            TargetBackend::Wasm,
            TargetBackend::Native,
            TargetBackend::Js,
        ]);

        for entry_pkg in self.get_entry_pkgs() {
            let deps_chain = self.backtrace_deps_chain(entry_pkg);
            for chain in deps_chain.iter() {
                let mut cur_deps_chain_supported_targets = HashSet::from_iter(vec![
                    TargetBackend::WasmGC,
                    TargetBackend::Wasm,
                    TargetBackend::Native,
                    TargetBackend::Js,
                    TargetBackend::LLVM,
                ]);
                for (i, dpe_pkg_name) in chain.iter().enumerate() {
                    let dep_pkg = self.get_package_by_name(dpe_pkg_name);
                    cur_deps_chain_supported_targets = cur_deps_chain_supported_targets
                        .intersection(&dep_pkg.supported_targets)
                        .cloned()
                        .collect();
                    if cur_deps_chain_supported_targets.is_empty() {
                        bail!(
                            "cannot find a common supported backend for the deps chain: {:?}",
                            chain[0..=i]
                                .iter()
                                .map(|s| format!(
                                    "{}: {}",
                                    s,
                                    TargetBackend::hashset_to_string(
                                        &self.get_package_by_name(s).supported_targets
                                    )
                                ))
                                .collect::<Vec<_>>()
                                .join(" -> ")
                        );
                    }
                    // disable this check for now, since it cause moon fmt | moon publish to fail
                    // if !cur_deps_chain_supported_targets.contains(&cur_target_backend) {
                    //     bail!(
                    //         "deps chain: {:?} supports backends `{}`, while the current target backend is {}",
                    //         chain[0..=i].iter().map(|s| format!("{}: {}", s, TargetBackend::hashset_to_string(&self.get_package_by_name(s).supported_targets))).collect::<Vec<_>>().join(" -> "), TargetBackend::hashset_to_string(&cur_deps_chain_supported_targets), cur_target_backend
                    //     );
                    // }
                }
                project_supported_targets = project_supported_targets
                    .intersection(&cur_deps_chain_supported_targets)
                    .cloned()
                    .collect();
            }
        }

        Ok(project_supported_targets)
    }

    pub fn get_topo_pkgs(&self) -> anyhow::Result<Vec<&Package>> {
        use petgraph::graph::NodeIndex;

        let mut graph = DiGraph::<String, usize>::new();
        let mut name_to_idx: IndexMap<String, NodeIndex> = IndexMap::new();
        let mut idx_to_name = IndexMap::new();

        for (to_node, pkg) in self.packages.iter() {
            if !name_to_idx.contains_key(to_node) {
                let to_idx = graph.add_node(to_node.clone());
                name_to_idx.insert(to_node.clone(), to_idx);
                idx_to_name.insert(to_idx, to_node.clone());
            }

            let to_idx = name_to_idx[to_node];

            for dep in pkg.imports.iter() {
                let from_node = dep.make_full_path();
                if !name_to_idx.contains_key(&from_node) {
                    let to_idx = graph.add_node(from_node.clone());
                    name_to_idx.insert(from_node.clone(), to_idx);
                    idx_to_name.insert(to_idx, from_node.clone());
                }
                let from_idx = name_to_idx[&from_node];
                graph.add_edge(from_idx, to_idx, 0);
            }
        }

        let topo_pkgs = match petgraph::algo::toposort(&graph, None) {
            Ok(res) => res
                .into_iter()
                .map(|idx| self.get_package_by_name(idx_to_name[&idx].as_str()))
                .collect::<Vec<_>>(),
            Err(cycle) => {
                bail!("cyclic dependency detected: {:?}", cycle);
            }
        };
        Ok(topo_pkgs)
    }

    pub fn get_package_by_path_mut(&mut self, path: &Path) -> Option<&mut Package> {
        self.packages
            .iter_mut()
            .map(|(_, pkg)| pkg)
            .find(|it| it.root_path == path)
    }

    pub fn get_package_by_index(&self, index: usize) -> &Package {
        &self.packages[self.packages.keys().nth(index).unwrap()]
    }

    pub fn contains_package(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    pub fn get_filtered_packages(
        &self,
        maybe_filter: Option<impl Fn(&Package) -> bool>,
    ) -> impl Iterator<Item = (&String, &Package)> {
        self.packages.iter().filter(move |(_, pkg)| {
            if let Some(filter) = &maybe_filter {
                filter(pkg)
            } else {
                true
            }
        })
    }

    pub fn get_filtered_packages_and_its_deps_by_pkgpath(
        &self,
        pkg_path: &Path,
    ) -> IndexMap<String, Package> {
        let pkg = self.get_package_by_path(pkg_path);
        match pkg {
            Some(pkg) => {
                let mut resolved = HashSet::new();
                resolved.insert(pkg.full_name().clone());
                self.resolve_deps_of_pkg(pkg, &mut resolved);
                let it = resolved
                    .iter()
                    .map(|pkg_name| (pkg_name.clone(), self.get_package_by_name(pkg_name).clone()));
                IndexMap::from_iter(it)
            }
            None => IndexMap::new(),
        }
    }

    pub fn get_filtered_packages_and_its_deps_by_pkgname(
        &self,
        pkgname: &str,
    ) -> anyhow::Result<IndexMap<String, Package>> {
        match self.packages.get(pkgname) {
            None => bail!("no such package: {}", pkgname),
            Some(pkg) => {
                let mut resolved = HashSet::new();
                resolved.insert(pkg.full_name().clone());
                self.resolve_deps_of_pkg(pkg, &mut resolved);
                let it = resolved
                    .iter()
                    .map(|pkg_name| (pkg_name.clone(), self.get_package_by_name(pkg_name).clone()));
                Ok(IndexMap::from_iter(it))
            }
        }
    }

    // resolve deps of the given pkg in dfs way
    fn resolve_deps_of_pkg(&self, pkg: &Package, res: &mut HashSet<String>) {
        for dep in pkg
            .imports
            .iter()
            .chain(pkg.wbtest_imports.iter())
            .chain(pkg.test_imports.iter())
        {
            let dep = &dep.path.make_full_path();
            if !res.contains(dep) {
                res.insert(dep.clone());
                self.resolve_deps_of_pkg(self.get_package_by_name(dep), res);
            }
        }
    }

    pub fn get_filtered_packages_mut(
        &mut self,
        maybe_filter: Option<impl Fn(&Package) -> bool>,
    ) -> impl Iterator<Item = (&String, &mut Package)> {
        self.packages.iter_mut().filter(move |(_, pkg)| {
            if let Some(filter) = &maybe_filter {
                filter(pkg)
            } else {
                true
            }
        })
    }
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
                        pkg.root_path.join(MOON_PKG_JSON).display(),
                        imported,
                        pkg.full_name()
                    ))
                }
                if !self.packages.contains_key(&imported) {
                    errors.push(format!(
                        "{}: cannot import `{}` in `{}`, no such package",
                        pkg.root_path.join(MOON_PKG_JSON).display(),
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

    pub fn contain_pre_build(&self) -> bool {
        for (_, pkg) in &self.packages {
            if pkg.pre_build.is_some() {
                return true;
            }
        }
        false
    }

    // some rules for virtual pkg
    pub fn validate_virtual_pkg(&self) -> anyhow::Result<()> {
        for (_, pkg) in &self.packages {
            // should we ignore third party packages?
            if pkg.is_third_party {
                continue;
            }

            let pkg_json = self
                .source_dir
                .join(pkg.rel.fs_full_name())
                .join(MOON_PKG_JSON);

            // virtual pkg can't implement other packages
            if pkg.virtual_pkg.is_some() && pkg.implement.is_some() {
                bail!(
                    "{}: virtual package `{}` cannot implement other packages",
                    pkg_json.display(),
                    pkg.full_name()
                );
            }

            if let Some(pkg_to_impl) = &pkg.implement {
                match self.get_package_by_name_safe(pkg_to_impl) {
                    // pkg_to_impl must be existed
                    None => bail!(
                        "{}: could not found the package `{}` to implemented, make sure the package name is correct, e.g. 'moonbitlang/core/double'",
                        pkg_json.display(),
                        pkg_to_impl
                    ),
                    // pkg_to_impl must be a virtual pkg
                    Some(pkg) if pkg.virtual_pkg.is_none() => {
                        bail!(
                            "{}: `{}` to implement must be a virtual package",
                            pkg_json.display(),
                            pkg_to_impl
                        )
                    },
                    _ => {}
                }

                // cannot implement and import at the same time
                if pkg
                    .imports
                    .iter()
                    .any(|i| i.path.make_full_path() == *pkg_to_impl)
                {
                    bail!(
                        "{}: cannot implement and import `{}` at the same time",
                        pkg_json.display(),
                        pkg_to_impl
                    );
                }
            }

            if let Some(overrides) = &pkg.overrides {
                let mut seen = std::collections::HashMap::new();

                for over_ride in overrides {
                    let override_impl = self.get_package_by_name_safe(over_ride);

                    match override_impl {
                        Some(impl_pkg) => {
                            match impl_pkg.implement.as_ref() {
                                Some(virtual_pkgname) => {
                                    // one virtual pkg can only have one implementation when link-core
                                    #[allow(clippy::map_entry)]
                                    if seen.contains_key(&virtual_pkgname) {
                                        bail!(
                                            "{}: duplicate implementation found for virtual package `{}`, both `{}` and `{}` implement it",
                                            pkg_json.display(),
                                            virtual_pkgname,
                                            seen[&virtual_pkgname],
                                            over_ride
                                        );
                                    } else {
                                        seen.insert(virtual_pkgname, over_ride.clone());
                                    }
                                }
                                None => {
                                    bail!(
                                        "{}: package `{}` doesn't implement any virtual package",
                                        pkg_json.display(),
                                        over_ride
                                    )
                                }
                            }
                        }
                        None => {
                            // override_impl must exist
                            bail!(
                                "{}: could not found the package `{}`, make sure the package name is correct, e.g. 'moonbitlang/core/double'",
                                pkg_json.display(),
                                over_ride
                            )
                        }
                    }
                }
            }
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
    pub opt_level: String,
    pub source: Option<String>,
}

pub fn convert_mdb_to_json(module: &ModuleDB) -> ModuleDBJSON {
    let mut pkgs = vec![];
    for (_, pkg) in &module.packages {
        // skip virtual moonbitlang/core/abort (gen_moonbitlang_abort_pkg)
        if pkg.full_name().starts_with(crate::common::MOONBITLANG_CORE) && pkg.is_third_party {
            continue;
        }
        let files = pkg.files.clone();
        let wbtest_files = pkg.wbtest_files.clone();
        let test_files = pkg.test_files.clone();
        let mbt_md_files = pkg.mbt_md_files.clone();
        let mut deps = vec![];
        for dep in &pkg.imports {
            let alias = match &dep.alias {
                None => {
                    let alias = dep.path.rel_path.components.last();
                    match alias {
                        None => dep
                            .path
                            .module_name
                            .split('/')
                            .next_back()
                            .unwrap()
                            .to_string(),
                        Some(x) => x.to_string(),
                    }
                }
                Some(x) => x.to_string(),
            };
            deps.push(AliasJSON {
                path: dep.path.make_full_path(),
                alias,
                fspath: module
                    .get_package_by_name(&dep.path.make_full_path())
                    .root_path
                    .display()
                    .to_string(),
            });
        }

        let mut wbtest_deps = vec![];
        for dep in &pkg.wbtest_imports {
            let alias = match &dep.alias {
                None => {
                    let alias = dep.path.rel_path.components.last();
                    match alias {
                        None => dep
                            .path
                            .module_name
                            .split('/')
                            .next_back()
                            .unwrap()
                            .to_string(),
                        Some(x) => x.to_string(),
                    }
                }
                Some(x) => x.to_string(),
            };
            wbtest_deps.push(AliasJSON {
                path: dep.path.make_full_path(),
                alias,
                fspath: module
                    .get_package_by_name(&dep.path.make_full_path())
                    .root_path
                    .display()
                    .to_string(),
            });
        }

        let mut test_deps = vec![];
        for dep in &pkg.test_imports {
            let alias = match &dep.alias {
                None => {
                    let alias = dep.path.rel_path.components.last();
                    match alias {
                        None => dep
                            .path
                            .module_name
                            .split('/')
                            .next_back()
                            .unwrap()
                            .to_string(),
                        Some(x) => x.to_string(),
                    }
                }
                Some(x) => x.to_string(),
            };
            test_deps.push(AliasJSON {
                path: dep.path.make_full_path(),
                alias,
                fspath: module
                    .get_package_by_name(&dep.path.make_full_path())
                    .root_path
                    .display()
                    .to_string(),
            });
        }

        pkgs.push(PackageJSON {
            is_main: pkg.is_main,
            is_third_party: pkg.is_third_party,
            root_path: pkg.root_path.display().to_string(),
            root: pkg.root.full_name(),
            rel: pkg.rel.full_name(),
            files,
            wbtest_files,
            test_files,
            deps,
            wbtest_deps,
            test_deps,
            mbt_md_files,
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
        opt_level: module.opt_level.clone(),
        source: module.source.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoonMod {
    pub name: String,
    pub version: Option<Version>,
    pub deps: IndexMap<String, SourceDependencyInfo>,
    pub bin_deps: Option<IndexMap<String, BinaryDependencyInfo>>,
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

    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,

    pub preferred_target: Option<TargetBackend>,

    pub scripts: Option<IndexMap<String, String>>,
    pub __moonbit_unstable_prebuild: Option<String>,
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
    pub version: Option<String>,

    /// third-party dependencies of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashMap<String, SourceDependencyInfoJson>>")]
    pub deps: Option<IndexMap<String, SourceDependencyInfoJson>>,

    /// third-party binary dependencies of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashMap<String, BinaryDependencyInfoJson>>")]
    pub bin_deps: Option<IndexMap<String, BinaryDependencyInfoJson>>,

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

    /// Files to include when publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,

    /// Files to exclude when publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,

    /// Scripts related to the current module.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashMap<String, String>>")]
    pub scripts: Option<IndexMap<String, String>>,

    /// The preferred target backend of this module.
    ///
    /// Toolchains are recommended to use this target as the default target
    /// when the user is not specifying or overriding in any other ways.
    /// However, this is merely a recommendation, and tools may deviate from
    /// this value at any time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_target: Option<String>,

    /// **Experimental:** A relative path to the pre-build configuration script.
    ///
    /// The script should be a **JavaScript or Python** file that is able to be
    /// executed with vanilla Node.JS or Python interpreter. Since this is
    /// experimental, the API may change at any time without warning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub __moonbit_unstable_prebuild: Option<String>,
}

impl TryFrom<MoonModJSON> for MoonMod {
    type Error = MoonModJSONFormatErrorKind;
    fn try_from(j: MoonModJSON) -> Result<Self, Self::Error> {
        if j.name.is_empty() {
            return Err(MoonModJSONFormatErrorKind::Name(NameError::EmptyName));
        }

        let version = match &j.version {
            None => None,
            Some(v) => {
                Some(Version::parse(v.as_str()).map_err(MoonModJSONFormatErrorKind::Version)?)
            }
        };

        let deps = match j.deps {
            None => IndexMap::new(),
            Some(d) => d.into_iter().map(|(k, v)| (k, v.into())).collect(),
        };

        let bin_deps = j
            .bin_deps
            .map(|d| d.into_iter().map(|(k, v)| (k, v.into())).collect());

        let source = j.source.map(|s| if s.is_empty() { ".".into() } else { s });
        let preferred_target = j
            .preferred_target
            .map(|x| TargetBackend::str_to_backend(&x))
            .transpose()
            .map_err(MoonModJSONFormatErrorKind::PreferredBackend)?;

        Ok(MoonMod {
            name: j.name,
            version,
            deps,
            bin_deps,
            readme: j.readme,
            repository: j.repository,
            license: j.license,
            keywords: j.keywords,
            description: j.description,

            compile_flags: j.compile_flags,
            link_flags: j.link_flags,
            checksum: j.checksum,
            source,
            ext: j.ext,

            alert_list: j.alert_list,
            warn_list: j.warn_list,

            include: j.include,
            exclude: j.exclude,

            scripts: j.scripts,
            preferred_target,

            __moonbit_unstable_prebuild: j.__moonbit_unstable_prebuild,
        })
    }
}

pub fn convert_module_to_mod_json(m: MoonMod) -> MoonModJSON {
    MoonModJSON {
        name: m.name,
        version: m.version.map(|v| v.to_string()),
        deps: Some(m.deps.into_iter().map(|(k, v)| (k, v.into())).collect()),
        bin_deps: m
            .bin_deps
            .map(|d| d.into_iter().map(|(k, v)| (k, v.into())).collect()),
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

        include: m.include,
        exclude: m.exclude,

        scripts: m.scripts,

        preferred_target: m.preferred_target.map(|x| x.to_flag().to_owned()),

        __moonbit_unstable_prebuild: m.__moonbit_unstable_prebuild,
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

    let html_template_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../moonbuild/template/mod_json_schema.html"
    );
    let html_template = std::fs::read_to_string(html_template_path).unwrap();
    let content = html_template.replace("const schema = {}", &format!("const schema = {}", actual));
    let html_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/manual/src/source/mod_json_schema.html"
    );
    std::fs::write(html_path, &content).unwrap();

    let html_path_zh = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/manual-zh/src/source/mod_json_schema.html"
    );
    std::fs::write(html_path_zh, content).unwrap();
}
