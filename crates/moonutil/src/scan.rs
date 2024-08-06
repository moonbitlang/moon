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

use crate::module::ModuleDB;
use crate::mooncakes::result::ResolvedEnv;
use crate::mooncakes::DirSyncResult;
use crate::package::{Import, Package};
use crate::path::{ImportComponent, ImportPath, PathComponent};
use anyhow::{bail, Context};
use indexmap::map::IndexMap;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use walkdir::WalkDir;

use crate::common::{
    read_module_desc_file_in_dir, MoonbuildOpt, DEP_PATH, IGNORE_DIRS, MOONBITLANG_CORE,
    MOON_MOD_JSON, MOON_PKG_JSON,
};

/// Matches an import string to scan paths.
///
/// Since the separation between module name and package name is unclear in import,
/// we currently do a greedy search to match the longest prefix present in the module names.
fn match_import_to_path(
    env: &ScanPaths,
    curr_module: &str,
    import: &str,
) -> anyhow::Result<ImportPath> {
    let mut len = import.len();
    let mut module_name = import;
    loop {
        if env.contains_key(module_name) {
            let suffix = import[len..].trim_start_matches('/');
            let is_3rd = module_name != curr_module;
            return Ok(ImportPath {
                module_name: module_name.to_owned(),
                rel_path: PathComponent::from_str(suffix).unwrap(),
                is_3rd,
            });
        } else {
            match module_name.rfind('/') {
                Some(d) => {
                    len = d;
                    module_name = &module_name[..len]
                }
                None => anyhow::bail!("No matching module was found for {}", import),
            }
        }
    }
}

/// (*.mbt[exclude the following], *_wbtest.mbt, *_test.mbt)
pub fn get_mbt_and_test_file_paths(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let mut mbt_files = vec![];
    let mut mbt_wbtest_files = vec![];
    let mut mbt_test_files = vec![];
    let entries = std::fs::read_dir(dir).unwrap();
    for entry in entries.flatten() {
        if let Ok(t) = entry.file_type() {
            if (t.is_file() || t.is_symlink())
                && entry.path().extension().is_some()
                && entry.path().extension().unwrap() == "mbt"
            {
                let p = entry.path();
                let stem = p.file_stem().unwrap().to_str().unwrap();

                let dot = stem.rfind('.');
                match dot {
                    None => {
                        if stem.ends_with("_wbtest") {
                            mbt_wbtest_files.push(p);
                        } else if stem.ends_with("_test") {
                            mbt_test_files.push(p);
                        } else {
                            mbt_files.push(p);
                        }
                    }
                    Some(idx) => {
                        let (filename, _dot_backend_ext) = stem.split_at(idx);
                        if filename.ends_with("_wbtest") {
                            mbt_wbtest_files.push(p);
                        } else if filename.ends_with("_test") {
                            mbt_test_files.push(p);
                        } else {
                            mbt_files.push(p);
                        }
                    }
                }
            }
        }
    }
    (mbt_files, mbt_wbtest_files, mbt_test_files)
}

/// This is to support coverage testing for builtin packages.
/// This function adds files in `../coverage` directory alongside files normally found in the
/// package, in order to bring coverage testing into scope.
///
/// This function is a temporary workaround requested by Hongbo.
/// It should be removed once proper package build disambiguation is implemented.
fn workaround_builtin_get_coverage_mbt_file_paths(dir: &Path, paths: &mut Vec<PathBuf>) {
    let coverage_dir = dir.parent().unwrap().join("coverage");
    if coverage_dir.exists() {
        let entries = std::fs::read_dir(coverage_dir).unwrap();
        for entry in entries.flatten() {
            if let Ok(t) = entry.file_type() {
                if (t.is_file() || t.is_symlink())
                    && entry.path().extension().is_some()
                    && entry.path().extension().unwrap() == "mbt"
                {
                    paths.push(entry.path());
                }
            }
        }
    }
}

fn scan_module_packages(
    env: &ScanPaths,
    is_third_party: bool,
    doc_mode: bool,
    moonbuild_opt: &crate::common::MoonbuildOpt,
    moonc_opt: &crate::common::MooncOpt,
) -> anyhow::Result<IndexMap<String, Package>> {
    let (module_source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);

    let mod_desc = read_module_desc_file_in_dir(module_source_dir)?;
    let module_source_dir = match &mod_desc.source {
        None => module_source_dir.to_path_buf(),
        Some(p) => module_source_dir.join(p),
    };

    let mut packages: IndexMap<String, Package> = IndexMap::new();

    // scan local packages
    let mut walker = WalkDir::new(&module_source_dir)
        .into_iter()
        .filter_entry(|e| {
            !IGNORE_DIRS.contains(&e.file_name().to_str().unwrap()) && e.file_type().is_dir()
        });
    while let Some(entry) = walker.next() {
        // manual iter since we want to skip subdirectories containing moon module json

        // We only care about 2 kinds of files here:
        // 1. `moon.mod.json`. It signals the presence of a new module in this dir,
        //    and if that dir isn't root, we wouldn't want to recurse into it.
        // 2. `moon.pkg.json`. This is what we are willing to find.

        let entry = entry.context("failed to read entry")?;
        let path = entry.path();
        let dir_contents = std::fs::read_dir(path)?;
        let mut has_moon_mod = false;
        let mut has_moon_pkg = false;
        for it in dir_contents {
            let filename = it?.file_name();
            if filename == MOON_MOD_JSON && path != module_source_dir {
                has_moon_mod = true;
            } else if filename == MOON_PKG_JSON {
                has_moon_pkg = true;
            }
        }

        if has_moon_mod {
            // This is a module located within the current module. Don't recurse into it any more.
            walker.skip_current_dir();
        } else if has_moon_pkg {
            // Go on scanning the package
            let cur_pkg = scan_one_package(
                env,
                path,
                &module_source_dir,
                &mod_desc,
                moonbuild_opt,
                moonc_opt,
                target_dir,
                is_third_party,
                doc_mode,
            )?;

            packages.insert(cur_pkg.full_name(), cur_pkg);
        }
    }
    Ok(packages)
}

#[allow(clippy::too_many_arguments)] // FIXME
fn scan_one_package(
    env: &ScanPaths,
    pkg_path: &Path,
    module_source_dir: &PathBuf,
    mod_desc: &crate::module::MoonMod,
    moonbuild_opt: &MoonbuildOpt,
    moonc_opt: &crate::common::MooncOpt,
    target_dir: &PathBuf,
    is_third_party: bool,
    doc_mode: bool,
) -> Result<Package, anyhow::Error> {
    let get_imports = |source: Vec<Import>| -> anyhow::Result<Vec<ImportComponent>> {
        let mut imports: Vec<ImportComponent> = vec![];
        for im in source {
            let x: anyhow::Result<ImportComponent> = match im {
                crate::package::Import::Simple(path) => {
                    let ic =
                        match_import_to_path(env, &mod_desc.name, &path).with_context(|| {
                            format!(
                                "failed to read import path in \"{}\"",
                                pkg_path.join(MOON_PKG_JSON).display()
                            )
                        })?;
                    let alias = Path::new(&path)
                        .file_stem()
                        .context(format!("failed to get alias of `{}`", path))?
                        .to_str()
                        .unwrap()
                        .to_string();
                    Ok(ImportComponent {
                        path: ic,
                        alias: Some(alias),
                    })
                }
                crate::package::Import::Alias { path, alias } => {
                    let ic = match_import_to_path(env, &mod_desc.name, &path)?;
                    Ok(ImportComponent {
                        path: ic,
                        alias: Some(alias),
                    })
                }
            };
            let x = x?;
            imports.push(x);
        }
        Ok(imports)
    };

    let pkg = crate::common::read_package_desc_file_in_dir(pkg_path)?;
    let rel = pkg_path.strip_prefix(module_source_dir)?;
    let rel_path = PathComponent::from_path(rel)?;

    let imports = get_imports(pkg.imports)?;
    let wbtest_imports = get_imports(pkg.wbtest_imports)?;
    let test_imports = get_imports(pkg.test_imports)?;

    let (mut mbt_files, mut wbtest_mbt_files, mut test_mbt_files) =
        get_mbt_and_test_file_paths(pkg_path);

    // workaround for builtin package testing
    if moonc_opt.build_opt.enable_coverage
        && mod_desc.name == MOONBITLANG_CORE
        && rel_path.components == ["builtin"]
    {
        workaround_builtin_get_coverage_mbt_file_paths(pkg_path, &mut mbt_files);
    }

    let sort_input = moonbuild_opt.sort_input;
    if sort_input {
        mbt_files.sort();
        wbtest_mbt_files.sort();
        test_mbt_files.sort();
    }
    let artifact: PathBuf = target_dir.into();
    let mut cur_pkg = Package {
        is_main: pkg.is_main,
        need_link: pkg.need_link,
        is_third_party,
        root_path: pkg_path.to_owned(),
        root: PathComponent::from_str(&mod_desc.name)?,
        files: mbt_files,
        files_contain_test_block: vec![],
        wbtest_files: wbtest_mbt_files,
        test_files: test_mbt_files,
        imports,
        wbtest_imports,
        test_imports,
        generated_test_drivers: vec![],
        artifact,
        rel: rel_path,
        link: pkg.link,
        warn_list: pkg.warn_list,
        alert_list: pkg.alert_list,
    };
    if doc_mode {
        // -o <folder>
        cur_pkg.artifact = cur_pkg
            .artifact
            .join(cur_pkg.root.full_name())
            .join(cur_pkg.rel.fs_full_name())
            .join(cur_pkg.last_name())
            .with_extension("?");
    } else {
        cur_pkg.artifact = if is_third_party {
            cur_pkg
                .artifact
                .join(DEP_PATH)
                .join(cur_pkg.root.full_name())
                .join(cur_pkg.rel.fs_full_name())
                .join(cur_pkg.last_name())
                .with_extension("?")
        } else {
            cur_pkg
                .artifact
                .join(cur_pkg.rel.fs_full_name())
                .join(cur_pkg.last_name())
                .with_extension("?")
        };
    }
    Ok(cur_pkg)
}

type ScanPaths = HashMap<String, PathBuf>;

/// Adapts the module data from [`ResolvedEnv`] into plain module names and their paths.
/// Workaround before [`scan`] supports multiple modules/packages with the same name.
fn adapt_modules_into_scan_paths(
    resolved_modules: &ResolvedEnv,
    module_paths: &DirSyncResult,
) -> ScanPaths {
    let mut result = HashMap::new();
    for (id, module) in resolved_modules.all_packages_and_id() {
        let path = module_paths
            .get(&id)
            .expect("All modules should be resolved");
        let module_name = module.name.to_string();
        result.insert(module_name, path.clone());
    }
    result
}

pub fn scan(
    doc_mode: bool,
    resolved_modules: &ResolvedEnv,
    module_paths: &DirSyncResult,
    moonc_opt: &crate::common::MooncOpt,
    moonbuild_opt: &crate::common::MoonbuildOpt,
) -> anyhow::Result<ModuleDB> {
    let source_dir = &moonbuild_opt.source_dir;

    let mod_desc = read_module_desc_file_in_dir(source_dir)?;
    let deps: Vec<String> = mod_desc.deps.iter().map(|(name, _)| name.clone()).collect();

    let module_scan_paths = adapt_modules_into_scan_paths(resolved_modules, module_paths);

    let mut packages = scan_module_packages(
        &module_scan_paths,
        false,
        doc_mode,
        moonbuild_opt,
        moonc_opt,
    )?;

    if moonbuild_opt.run_mode == crate::common::RunMode::Test {
        if let Some(crate::common::TestOpt {
            filter_package: Some(ref filter_package),
            ..
        }) = moonbuild_opt.test_opt
        {
            let pkgs = packages
                .iter()
                .filter(|(k, _)| filter_package.contains(Path::new(k)))
                .map(|(_, v)| v);
            let mut pkg_and_its_deps = HashSet::new();
            for pkg in pkgs {
                pkg_and_its_deps.extend(get_pkg_and_its_deps(pkg, &packages));
            }
            // filter out other packages
            packages.retain(|k, _| pkg_and_its_deps.contains(k));
        }
    }

    // scan third party packages in DEP_PATH according to deps field
    for (module_id, _) in resolved_modules.all_packages_and_id() {
        if resolved_modules.module_info(module_id).name == mod_desc.name {
            continue; // skip self
        }

        let dir = module_paths.get(&module_id).unwrap();

        let moonbuild_opt = &MoonbuildOpt {
            source_dir: dir.clone(),
            ..moonbuild_opt.clone()
        };

        let third_packages =
            scan_module_packages(&module_scan_paths, true, doc_mode, moonbuild_opt, moonc_opt)?;
        packages.extend(third_packages);
    }

    let sort_input = moonbuild_opt.sort_input;
    if sort_input {
        let mut xs: Vec<(String, Package)> = packages.into_iter().collect();
        xs.sort_by(|a, b| a.0.cmp(&b.0));
        packages = xs.into_iter().collect();
    }

    let mut graph = DiGraph::<String, usize>::new();
    let mut name_to_idx: IndexMap<String, NodeIndex> = IndexMap::new();
    let mut idx_to_name = IndexMap::new();

    let mut entries = Vec::new();
    for (i, (from_node, pkg)) in packages.iter().enumerate() {
        if !name_to_idx.contains_key(from_node) {
            let from_idx = graph.add_node(from_node.clone());
            name_to_idx.insert(from_node.clone(), from_idx);
            idx_to_name.insert(from_idx, from_node.clone());
        }
        if pkg.is_main {
            entries.push(i);
        }

        let from_idx = name_to_idx[from_node];

        for dep in pkg.imports.iter() {
            let to_node = dep.path.make_full_path();
            if !name_to_idx.contains_key(&to_node) {
                let to_idx = graph.add_node(to_node.clone());
                name_to_idx.insert(to_node.clone(), to_idx);
                idx_to_name.insert(to_idx, to_node.clone());
            }
            let to_idx = name_to_idx[&to_node];
            graph.add_edge(from_idx, to_idx, 0);
        }
    }

    match petgraph::algo::toposort(&graph, None) {
        Ok(_) => {}
        Err(cycle) => {
            let cycle = crate::graph::get_example_cycle(&graph, cycle.node_id());
            let cycle = cycle
                .into_iter()
                .map(|n| idx_to_name[&n].clone())
                .collect::<Vec<_>>();
            bail!("cyclic dependency detected: {:?}", cycle);
        }
    }

    entries.sort();

    let module = ModuleDB {
        source_dir: dunce::canonicalize(source_dir).unwrap(),
        name: mod_desc.name.to_string(),
        packages,
        entries,
        deps,
        graph,
        backend: moonc_opt.link_opt.target_backend.to_backend_ext().into(),
        source: mod_desc.source,
    };

    module.validate()?;

    // log::debug!("{:#?}", module);
    // log::debug!(
    //     "{:?}",
    //     petgraph::dot::Dot::with_config(&module.graph, &[petgraph::dot::Config::EdgeNoLabel])
    // );
    Ok(module)
}

fn get_pkg_and_its_deps(pkg: &Package, packages: &IndexMap<String, Package>) -> HashSet<String> {
    let mut resolved = HashSet::new();
    resolved.insert(pkg.full_name().clone());
    resolve_deps_of_pkg(&pkg.full_name(), packages, &mut resolved);
    resolved
}

// resolve deps of the given pkg in dfs way
fn resolve_deps_of_pkg(
    pkg_name: &String,
    packages: &IndexMap<String, Package>,
    res: &mut HashSet<String>,
) {
    let pkg = packages.get(pkg_name);
    if let Some(pkg) = pkg {
        for dep in pkg
            .imports
            .iter()
            .chain(pkg.wbtest_imports.iter())
            .chain(pkg.test_imports.iter())
        {
            let dep = &dep.path.make_full_path();
            if !res.contains(dep) {
                res.insert(dep.clone());
                resolve_deps_of_pkg(dep, packages, res);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use expect_test::expect;

    use super::match_import_to_path;

    #[test]
    fn test_match_import() {
        let scan_paths = [("foo/bar", ""), ("foo/baz", ""), ("foo/bar/baz", "")]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        // foo/bar = package(foo/bar) / rel()
        expect![[r#"
            (foo/bar)
        "#]]
        .assert_debug_eq(&match_import_to_path(&scan_paths, "foo/bar", "foo/bar").unwrap());

        // foo/bar/qux = package(foo/bar) / rel(qux)
        expect![[r#"
            (foo/bar)qux
        "#]]
        .assert_debug_eq(&match_import_to_path(&scan_paths, "foo/bar", "foo/bar/qux").unwrap());

        // foo/bar/baz = package(foo/bar/baz) / rel()
        expect![[r#"
            *(foo/bar/baz)
        "#]]
        .assert_debug_eq(&match_import_to_path(&scan_paths, "foo/bar", "foo/bar/baz").unwrap());

        // foo/bar/baz/qux = package(foo/bar/baz) / rel(qux)
        expect![[r#"
            *(foo/bar/baz)qux
        "#]]
        .assert_debug_eq(&match_import_to_path(&scan_paths, "foo/bar", "foo/bar/baz/qux").unwrap());

        // foo/baz/qux = package(foo/baz) / rel(qux)
        expect![[r#"
            *(foo/baz)qux
        "#]]
        .assert_debug_eq(&match_import_to_path(&scan_paths, "foo/bar", "foo/baz/qux").unwrap());
    }
}
