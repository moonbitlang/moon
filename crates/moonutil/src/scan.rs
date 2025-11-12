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

use crate::cond_expr::{self, CompileCondition, CondExpr};
use crate::module::{ModuleDB, MoonMod};
use crate::mooncakes::DirSyncResult;
use crate::mooncakes::result::ResolvedEnv;
use crate::package::{Import, MoonPkgGenerate, Package, SubPackageInPackage};
use crate::path::{ImportComponent, ImportPath, PathComponent};
use anyhow::{Context, bail};
use colored::Colorize;
use indexmap::{IndexSet, map::IndexMap};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use walkdir::WalkDir;

use crate::common::{
    DEP_PATH, DOT_MBL, DOT_MBT_DOT_MD, DOT_MBY, IGNORE_DIRS, MBTI_USER_WRITTEN, MOON_MOD_JSON,
    MOON_PKG_JSON, MOONBITLANG_ABORT, MoonbuildOpt, SUB_PKG_POSTFIX, TargetBackend,
    read_module_desc_file_in_dir,
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

/// (*.mbt[exclude the following], *_wbtest.mbt, *_test.mbt, *.mbt.md, *.mbt.x)
#[allow(clippy::type_complexity)]
pub fn get_mbt_and_test_file_paths(
    dir: &Path,
) -> (
    Vec<PathBuf>,
    Vec<PathBuf>,
    Vec<PathBuf>,
    Vec<PathBuf>,
    Vec<PathBuf>,
    Vec<PathBuf>,
) {
    let mut mbt_files = vec![];
    let mut mbt_wbtest_files = vec![];
    let mut mbt_test_files = vec![];
    let mut mbt_md_files = vec![];
    let mut mbl_files = vec![];
    let mut mby_files: Vec<PathBuf> = vec![];
    let entries = std::fs::read_dir(dir).unwrap();
    for entry in entries.flatten() {
        if let Ok(t) = entry.file_type()
            && (t.is_file() || t.is_symlink())
            && entry.path().extension().is_some()
            && (entry.path().extension().unwrap() == "mbt"
                || entry.path().extension().unwrap() == "md"
                || entry.path().extension().unwrap() == "mbl"
                || entry.path().extension().unwrap() == "mby")
        {
            let p = entry.path();

            let p_str = p.to_str().unwrap();
            if p_str.ends_with("md") {
                if p_str.ends_with(DOT_MBT_DOT_MD) {
                    mbt_md_files.push(p.clone());
                }
            } else if p_str.ends_with(DOT_MBL) {
                mbl_files.push(p.clone());
            } else if p_str.ends_with(DOT_MBY) {
                mby_files.push(p.clone())
            } else {
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
    (
        mbt_files,
        mbt_wbtest_files,
        mbt_test_files,
        mbt_md_files,
        mbl_files,
        mby_files,
    )
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
            if let Ok(t) = entry.file_type()
                && (t.is_file() || t.is_symlink())
                && entry.path().extension().is_some()
                && entry.path().extension().unwrap() == "mbt"
            {
                paths.push(entry.path());
            }
        }
    }
}

fn scan_module_packages(
    packages: &mut IndexMap<String, Package>,
    env: &ScanPaths,
    is_third_party: bool,
    doc_mode: bool,
    moonbuild_opt: &crate::common::MoonbuildOpt,
    moonc_opt: &crate::common::MooncOpt,
) -> anyhow::Result<()> {
    let (module_source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);
    let module_source_arc: Arc<_> = module_source_dir.as_path().into();

    let mod_desc = read_module_desc_file_in_dir(module_source_dir)?;
    let module_source_dir = match &mod_desc.source {
        None => module_source_dir.to_path_buf(),
        Some(p) => {
            let src_dir = module_source_dir.join(p);
            dunce::canonicalize(src_dir.clone()).with_context(|| {
                format!(
                    "failed to canonicalize source directory: {}",
                    src_dir.display()
                )
            })?
        }
    };

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
                &module_source_arc,
                &mod_desc,
                moonbuild_opt,
                moonc_opt,
                target_dir,
                is_third_party,
                doc_mode,
            )?;

            if let Some(sub_package) = cur_pkg.with_sub_package.as_ref() {
                let mut components = cur_pkg.rel.clone().components;
                if let Some(last) = components.last_mut() {
                    *last = format!("{last}{SUB_PKG_POSTFIX}");
                }
                let rel = PathComponent { components };

                let artifact = cur_pkg.artifact.parent().unwrap().join(format!(
                    "{}.?",
                    if rel.components.is_empty() {
                        cur_pkg.root.components.last().unwrap()
                    } else {
                        rel.components.last().unwrap()
                    }
                ));

                let sub_pkg = Package {
                    rel,
                    files: sub_package.files.clone(),
                    with_sub_package: None,
                    is_sub_package: true,
                    imports: sub_package.import.clone(),
                    artifact: artifact.clone(),

                    wbtest_files: IndexMap::new(),
                    test_files: IndexMap::new(),
                    mbt_md_files: IndexMap::new(),
                    files_contain_test_block: vec![],
                    wbtest_imports: vec![],
                    test_imports: vec![],
                    generated_test_drivers: vec![],
                    patch_file: None,
                    no_mi: false,
                    install_path: None,
                    bin_name: None,

                    ..cur_pkg.clone()
                };

                packages.insert(sub_pkg.full_name(), sub_pkg);
            }

            match packages.entry(cur_pkg.full_name()) {
                indexmap::map::Entry::Occupied(occupied_entry) => {
                    let existing = occupied_entry.get();
                    anyhow::bail!(
                        "Ambiguous package name: {}\nCandidates:\n  {} in {} ({})\n  {} in {} ({})",
                        cur_pkg.full_name(),
                        cur_pkg.rel.full_name(),
                        cur_pkg.root.full_name(),
                        cur_pkg.root_path.display(),
                        existing.rel.full_name(),
                        existing.root.full_name(),
                        existing.root_path.display()
                    )
                }
                indexmap::map::Entry::Vacant(vacant_entry) => {
                    vacant_entry.insert(cur_pkg);
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // FIXME
fn scan_one_package(
    env: &ScanPaths,
    pkg_path: &Path,
    module_source_path: &Path,
    module_source_arc: &Arc<Path>,
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
                        .context(format!("failed to get alias of `{path}`"))?
                        .to_str()
                        .unwrap()
                        .to_string();
                    Ok(ImportComponent {
                        path: ic,
                        alias: Some(alias),
                        sub_package: false,
                    })
                }
                crate::package::Import::Alias {
                    path,
                    alias,
                    sub_package,
                } => {
                    let ic = match_import_to_path(env, &mod_desc.name, &path)?;
                    Ok(ImportComponent {
                        path: ic,
                        alias,
                        sub_package,
                    })
                }
            };
            let x = x?;
            imports.push(x);
        }
        Ok(imports)
    };

    let pkg = crate::common::read_package_desc_file_in_dir(pkg_path)?;
    let rel = pkg_path.strip_prefix(module_source_path)?;
    let rel_path = PathComponent::from_path(rel)?;

    // FIXME: This is merely a workaround for the whole thing to work for now
    alias_dedup(&pkg.imports, &pkg.wbtest_imports, &pkg.test_imports).with_context(|| {
        format!(
            "Duplicated alias found when scanning package at {}",
            pkg_path.display()
        )
    })?;

    let imports = get_imports(pkg.imports)?;
    let wbtest_imports = get_imports(pkg.wbtest_imports)?;
    let mut test_imports = get_imports(pkg.test_imports)?;
    // add prelude to test-import for core packages, unless we're scanning the prelude itself
    if mod_desc.name == crate::common::MOONBITLANG_CORE {
        let is_prelude_pkg = rel_path.components.len() == 1 && rel_path.components[0] == "prelude";
        let has_prelude_import = test_imports.iter().any(|import| {
            import.path.module_name == mod_desc.name
                && import.path.rel_path.components.len() == 1
                && import.path.rel_path.components[0] == "prelude"
        });

        if !is_prelude_pkg && !has_prelude_import {
            test_imports.push(ImportComponent {
                path: ImportPath {
                    module_name: mod_desc.name.clone(),
                    rel_path: PathComponent {
                        components: vec!["prelude".to_string()],
                    },
                    is_3rd: false,
                },
                alias: Some("prelude".to_string()),
                sub_package: false,
            });
        }
    }

    let (
        mut mbt_files,
        mut wbtest_mbt_files,
        mut test_mbt_files,
        mut mbt_md_files,
        mut mbl_files,
        mut mby_files,
    ) = get_mbt_and_test_file_paths(pkg_path);

    // workaround for builtin package testing
    if moonc_opt.build_opt.enable_coverage
        && mod_desc.name == crate::common::MOONBITLANG_CORE
        && rel_path.components == ["builtin"]
    {
        workaround_builtin_get_coverage_mbt_file_paths(pkg_path, &mut mbt_files);
    }

    let sort_input = moonbuild_opt.sort_input;
    if sort_input {
        mbt_files.sort();
        wbtest_mbt_files.sort();
        test_mbt_files.sort();
        mbt_md_files.sort();
        mbl_files.sort();
        mby_files.sort();
    }

    // append warn_list & alert_list in current moon.pkg.json into the one in moon.mod.json
    let warn_list = mod_desc
        .warn_list
        .as_ref()
        .map_or(pkg.warn_list.clone(), |x| {
            Some(x.clone() + &pkg.warn_list.unwrap_or_default())
        })
        .map_or(moonc_opt.build_opt.warn_list.clone(), |x| {
            Some(x.clone() + &moonc_opt.build_opt.warn_list.clone().unwrap_or_default())
        })
        .filter(|s| !s.is_empty());
    let alert_list = mod_desc
        .alert_list
        .as_ref()
        .map_or(pkg.alert_list.clone(), |x| {
            Some(x.clone() + &pkg.alert_list.unwrap_or_default())
        })
        .map_or(moonc_opt.build_opt.alert_list.clone(), |x| {
            Some(x.clone() + &moonc_opt.build_opt.alert_list.clone().unwrap_or_default())
        })
        .filter(|s| !s.is_empty());

    let artifact: PathBuf = target_dir.into();

    let cond_targets = {
        let mut x = pkg.targets.unwrap_or(IndexMap::new());

        for file in mbt_files
            .iter()
            .chain(wbtest_mbt_files.iter())
            .chain(test_mbt_files.iter())
        {
            let filename = file.file_name().unwrap().to_str().unwrap().to_string();
            if !x.contains_key(&filename) {
                let stem = file.file_stem().unwrap().to_str().unwrap();
                let dot = stem.rfind('.');
                match dot {
                    None => {}
                    Some(idx) => {
                        let (_, backend_ext) = stem.split_at(idx + 1);
                        if let Ok(target) = TargetBackend::str_to_backend(backend_ext) {
                            eprintln!(
                                "{}: use backend extension in filename(`{}`) is deprecated. Please use `targets` field in moon.pkg.json instead.",
                                "Warning".yellow(),
                                file.display()
                            );
                            x.insert(filename, CondExpr::Atom(cond_expr::Atom::Target(target)));
                        }
                    }
                };
            }
        }
        Some(x)
    };

    let file_cond_map = |files: Vec<PathBuf>| -> IndexMap<PathBuf, CompileCondition> {
        IndexMap::from_iter(files.into_iter().map(|p| {
            (
                p.clone(),
                cond_targets
                    .as_ref()
                    .and_then(|it| it.get(p.file_name().unwrap().to_str().unwrap()))
                    .map(|f| f.to_compile_condition())
                    .unwrap_or_default(),
            )
        }))
    };

    let sub_package = pkg.sub_package.and_then(|s| {
        let imports = get_imports(s.import).ok()?;
        Some(SubPackageInPackage {
            files: file_cond_map(s.files.iter().map(|p| pkg_path.join(p)).collect()),
            import: imports,
        })
    });

    let formatter_ignore: IndexSet<String> = pkg
        .formatter
        .ignore
        .iter()
        .filter_map(|entry| {
            Path::new(entry)
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
        })
        .collect();

    macro_rules! stringify_bin {
        ($field:ident) => {{
            static CELL: OnceLock<Option<&str>> = OnceLock::new();
            || {
                CELL.get_or_init(|| crate::BINARIES.$field.to_str())
                    .with_context(|| {
                        format!(
                            "cannot decode {} path: {:?}",
                            stringify!($field),
                            crate::BINARIES.$field
                        )
                    })
            }
        }};
    }
    let moonrun = stringify_bin!(moonrun);
    let pkg_prebuild_is_none = pkg.pre_build.is_none();
    let mut prebuild = pkg.pre_build.unwrap_or(vec![]);
    for mbl_file in mbl_files {
        let mbt_file = mbl_file.with_extension("mbt");
        let generate = MoonPkgGenerate {
            input: crate::package::StringOrArray::String(mbl_file.display().to_string()),
            output: crate::package::StringOrArray::String(mbt_file.display().to_string()),
            command: format!(
                "{} {} -- $input -o $output",
                moonrun()?,
                stringify_bin!(moonlex)()?,
            ),
        };
        prebuild.push(generate);
    }
    for mby_file in mby_files {
        let mbt_file = mby_file.with_extension("mbt");
        let generate = MoonPkgGenerate {
            input: crate::package::StringOrArray::String(mby_file.display().to_string()),
            output: crate::package::StringOrArray::String(mbt_file.display().to_string()),
            command: format!(
                "{} {} -- $input -o $output",
                moonrun()?,
                stringify_bin!(moonyacc)()?,
            ),
        };
        prebuild.push(generate);
    }

    let mut cur_pkg = Package {
        is_main: pkg.is_main,
        force_link: pkg.force_link,
        is_third_party,
        root_path: pkg_path.to_owned(),
        module_root: Arc::clone(module_source_arc),
        root: PathComponent::from_str(&mod_desc.name)?,
        files: file_cond_map(mbt_files),
        files_contain_test_block: vec![],
        wbtest_files: file_cond_map(wbtest_mbt_files),
        test_files: file_cond_map(test_mbt_files),
        mbt_md_files: file_cond_map(mbt_md_files),
        formatter_ignore,
        with_sub_package: sub_package,
        is_sub_package: false,
        imports,
        wbtest_imports,
        test_imports,
        generated_test_drivers: vec![],
        artifact,
        rel: rel_path,
        link: pkg.link,
        warn_list,
        alert_list,
        targets: cond_targets,
        pre_build: if pkg_prebuild_is_none && prebuild.is_empty() {
            None
        } else {
            Some(prebuild)
        },
        patch_file: None,
        no_mi: false,
        install_path: moonbuild_opt
            .build_opt
            .as_ref()
            .and_then(|it| it.install_path.clone())
            .filter(|_| pkg.is_main && !is_third_party),
        bin_name: pkg.bin_name,
        bin_target: pkg.bin_target,
        enable_value_tracing: false,
        supported_targets: pkg.supported_targets,
        stub_lib: pkg
            .native_stub
            .and_then(|x| if x.is_empty() { None } else { Some(x) }),

        virtual_mbti_file: if pkg.virtual_pkg.is_some() {
            // Currently we accept both `pkg.mbti` and `<pkg_short_name>.mbti`,
            // preferring the former if both are available.
            let new_virtual_mbti_file = pkg_path.join(MBTI_USER_WRITTEN);
            let has_new_mbti = new_virtual_mbti_file.exists();

            let legacy_virtual_mbti_file = pkg_path.join(format!(
                "{}.mbti",
                rel.file_name()
                    .map(|x| x.to_string_lossy())
                    .unwrap_or_else(|| mod_desc
                        .name
                        .rsplit('/')
                        .next()
                        .expect("Empty module name")
                        .into())
            ));
            let has_legacy_mbti = legacy_virtual_mbti_file.exists();

            if has_new_mbti {
                Some(new_virtual_mbti_file)
            } else if has_legacy_mbti {
                eprintln!(
                    "{}: Using package name in MBTI file `{}` is deprecated. Please rename it to `{}`.",
                    "Warning".yellow(),
                    legacy_virtual_mbti_file.display(),
                    MBTI_USER_WRITTEN
                );
                Some(legacy_virtual_mbti_file)
            } else {
                anyhow::bail!(
                    "virtual mbti file `{}` not found",
                    legacy_virtual_mbti_file.display()
                );
            }
        } else {
            None
        },
        virtual_pkg: pkg.virtual_pkg,
        implement: pkg.implement,
        overrides: pkg.overrides,
        link_libs: vec![],
        link_search_paths: vec![],
        link_flags: None,
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

/// Check if import aliases within the package imports have any duplicates
///
/// This piece of code originally lived in
/// [`crate::package::convert_pkg_json_to_package`], but moved here since alias
/// is now optional.
fn alias_dedup(
    imports: &[Import],
    wbtest_imports: &[Import],
    test_imports: &[Import],
) -> anyhow::Result<()> {
    use std::collections::HashSet;
    // TODO: check on the fly
    let mut alias_dedup = HashSet::new();
    for item in imports.iter() {
        let alias = alias_from_import_item(item);
        if alias_dedup.contains(&alias) {
            bail!("Duplicate alias `{}`", alias);
        } else {
            alias_dedup.insert(alias);
        }
    }

    // TODO: check on the fly
    let mut alias_dedup = HashSet::new();
    for item in wbtest_imports.iter() {
        let alias = alias_from_import_item(item);
        if alias_dedup.contains(&alias) {
            bail!("Duplicate alias `{}`", alias);
        } else {
            alias_dedup.insert(alias);
        }
    }

    // TODO: check on the fly
    let mut alias_dedup = HashSet::new();
    for item in test_imports.iter() {
        let alias = alias_from_import_item(item);
        if alias_dedup.contains(&alias) {
            bail!("Duplicate alias `{}`", alias);
        } else {
            alias_dedup.insert(alias);
        }
    }

    Ok(())
}

fn alias_from_import_item(item: &Import) -> &str {
    match item {
        Import::Simple(p) => alias_from_package_name(p),
        Import::Alias {
            path,
            alias,
            sub_package: _,
        } => alias
            .as_deref()
            .unwrap_or_else(|| alias_from_package_name(path)),
    }
}

pub fn alias_from_package_name(package: &str) -> &str {
    package.rsplit_once('/').map(|x| x.1).unwrap_or(package)
}

type ScanPaths = HashMap<String, PathBuf>;

/// Adapts the module data from [`ResolvedEnv`] into plain module names and their paths.
/// Workaround before [`scan`] supports multiple modules/packages with the same name.
fn adapt_modules_into_scan_paths(
    resolved_modules: &ResolvedEnv,
    module_paths: &DirSyncResult,
) -> ScanPaths {
    let mut result = HashMap::new();
    for (id, module) in resolved_modules.all_modules_and_id() {
        let path = module_paths
            .get(id)
            .expect("All modules should be resolved");
        let module_name = module.name().to_string();
        result.insert(module_name, path.clone());
    }
    result
}

pub fn scan(
    doc_mode: bool,
    moon_mod_for_single_file_test: Option<MoonMod>,
    resolved_modules: &ResolvedEnv,
    module_paths: &DirSyncResult,
    moonc_opt: &crate::common::MooncOpt,
    moonbuild_opt: &crate::common::MoonbuildOpt,
) -> anyhow::Result<ModuleDB> {
    let source_dir = &moonbuild_opt.source_dir;

    let module_scan_paths = adapt_modules_into_scan_paths(resolved_modules, module_paths);
    let mut packages = IndexMap::new();
    if moon_mod_for_single_file_test.is_none() {
        scan_module_packages(
            &mut packages,
            &module_scan_paths,
            false,
            doc_mode,
            moonbuild_opt,
            moonc_opt,
        )?;
    }

    let mod_desc = if let Some(moon_mod) = moon_mod_for_single_file_test {
        moon_mod
    } else {
        read_module_desc_file_in_dir(source_dir)?
    };
    let deps: Vec<String> = mod_desc.deps.iter().map(|(name, _)| name.clone()).collect();

    // scan third party packages in DEP_PATH according to deps field
    for (module_id, module) in resolved_modules.all_modules_and_id() {
        if resolved_modules.module_info(module_id).name == mod_desc.name {
            continue; // skip self
        }

        // Don't scan the injected standard library
        if matches!(
            module.source(),
            crate::mooncakes::ModuleSourceKind::Stdlib(_)
        ) {
            continue;
        }

        let dir = module_paths.get(module_id).unwrap();

        let moonbuild_opt = &MoonbuildOpt {
            source_dir: dir.clone(),
            ..moonbuild_opt.clone()
        };

        scan_module_packages(
            &mut packages,
            &module_scan_paths,
            true,
            doc_mode,
            moonbuild_opt,
            moonc_opt,
        )?;
    }

    if !moonc_opt.nostd && mod_desc.name != crate::common::MOONBITLANG_CORE {
        packages.insert(
            MOONBITLANG_ABORT.to_string(),
            crate::common::gen_moonbitlang_abort_pkg(moonc_opt),
        );
    }

    let sort_input = moonbuild_opt.sort_input;
    if sort_input {
        packages.sort_unstable_keys();
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
            let to_node = dep.make_full_path();
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
            let cycle_str = cycle.join(" -> ");
            bail!("cyclic dependency detected: {}", cycle_str);
        }
    }

    entries.sort();

    let module = ModuleDB::new(
        dunce::canonicalize(source_dir).unwrap(),
        mod_desc.name.to_string(),
        packages,
        entries,
        deps,
        graph.clone(),
        moonc_opt.link_opt.target_backend.to_backend_ext().into(),
        if moonc_opt.build_opt.debug_flag {
            "debug".to_string()
        } else {
            "release".to_string()
        },
        mod_desc.source,
    );

    module.validate()?;

    module.validate_virtual_pkg()?;

    // todo: if there are only one backend and target backend is not specified by user, set it as the default backend?
    // MAINTAINERS: removed because the feature is very incomplete and slow as hell
    // let _ = module.get_project_supported_targets(moonc_opt.build_opt.target_backend)?;

    // log::debug!("{:#?}", module);
    // log::debug!(
    //     "{:?}",
    //     petgraph::dot::Dot::with_config(&module.graph, &[petgraph::dot::Config::EdgeNoLabel])
    // );
    Ok(module)
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
