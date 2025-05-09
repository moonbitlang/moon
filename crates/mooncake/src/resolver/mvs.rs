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

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
};

use anyhow::anyhow;
use moonutil::{
    dependency::SourceDependencyInfo,
    module::MoonMod,
    mooncakes::{GitSource, ModuleName, ModuleSource, ModuleSourceKind},
    version::as_caret_comparator,
};
use semver::Version;

use super::{env::ResolverEnv, Resolver, ResolverError};

/// A dependency solver that follows the MVS (minimal version selection) algorithm,
/// which is the same as that Go uses.
/// See https://research.swtch.com/vgo-mvs for more information.
pub struct MvsSolver;

impl Resolver for MvsSolver {
    fn resolve(
        &mut self,
        env: &mut ResolverEnv,
        root: &[(ModuleSource, Rc<MoonMod>)],
    ) -> Option<super::result::ResolvedEnv> {
        mvs_resolve(env, root)
    }
}

fn select_min_version_satisfying<'a>(
    name: &ModuleName,
    req: &SourceDependencyInfo,
    versions: impl Iterator<Item = &'a Version> + 'a,
) -> Result<Version, ResolverError> {
    // We only support caret version requirements, per mvs algorithm, so
    // do a preliminary scan before matching.
    for it in &req.version.comparators {
        if it.op != semver::Op::Caret {
            return Err(ResolverError::Other(anyhow!(
                "Only caret version requirements are supported; got: {}",
                req.version
            )));
        }
    }

    // From lowest to highest version, find the first version that satisfies the requirement.
    for version in versions {
        if req.version.matches(version) {
            return Ok(version.clone());
        }
    }

    Err(ResolverError::NoSatisfiedVersion(
        name.clone(),
        req.version.clone(),
    ))
}

fn select_min_version_satisfying_in_env(
    env: &mut ResolverEnv,
    name: &ModuleName,
    req: &SourceDependencyInfo,
) -> Result<(Version, Rc<MoonMod>), ResolverError> {
    let all_versions = env
        .all_versions_of(name, None) // todo: registry
        .ok_or_else(|| ResolverError::ModuleMissing(name.clone()))?;

    let min_version_satisfying = select_min_version_satisfying(name, req, all_versions.keys());
    match min_version_satisfying {
        Ok(version) => {
            let module = Rc::clone(&all_versions[&version]);
            Ok((version, module))
        }
        Err(err) => Err(err),
    }
}

/// Checks whether resolving local dependency is allowed for the dependant package
fn local_dep_allowed(dependant: &ModuleSource) -> bool {
    match &dependant.source {
        ModuleSourceKind::Registry(_) => false,
        ModuleSourceKind::Local(_) => true,
        ModuleSourceKind::Git(_) => true,
    }
}

/// Checks whether resolving git dependency is allowed for the dependant package
fn git_dep_allowed(dependant: &ModuleSource) -> bool {
    match &dependant.source {
        ModuleSourceKind::Registry(_) => false,
        ModuleSourceKind::Local(_) => true,
        ModuleSourceKind::Git(_) => true,
    }
}

/// Returns the root path of the dependant, to be used with local dependencies.
/// Panics if [`local_dep_allowed(dependant)`] is false.
fn root_path_of(dependant: &ModuleSource) -> PathBuf {
    match &dependant.source {
        ModuleSourceKind::Registry(_) => {
            panic!("Registry dependencies don't have a local root path!")
        }
        ModuleSourceKind::Local(path) => path.clone(),
        ModuleSourceKind::Git(repo) => {
            todo!("Resolve local downloaded path for git repo: {}", repo)
        }
    }
}

/// A wrapper to generate the correct ordering expected in the MVS solver.
#[repr(transparent)]
#[derive(PartialEq, Eq)]
struct ModuleSourceOrdWrapper(ModuleSource);

impl Ord for ModuleSourceOrdWrapper {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let version_cmp = self.0.version.cmp(&other.0.version);
        if !version_cmp.is_eq() {
            return version_cmp;
        }
        self.0.source.cmp(&other.0.source)
    }
}

impl PartialOrd for ModuleSourceOrdWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.0.cmp(&other.0))
    }
}

impl From<ModuleSource> for ModuleSourceOrdWrapper {
    fn from(value: ModuleSource) -> Self {
        Self(value)
    }
}

impl From<ModuleSourceOrdWrapper> for ModuleSource {
    fn from(value: ModuleSourceOrdWrapper) -> Self {
        value.0
    }
}

fn warn_about_skipped_local_or_git_dep(ms: &ModuleSource) {
    match &ms.source {
        ModuleSourceKind::Local(_) => {
            log::warn!(
                "A git dependency was skipped during version resolution: {}",
                ms
            );
        }
        ModuleSourceKind::Git(_) => {
            log::warn!(
                "A git dependency was skipped during version selection: {}",
                ms
            )
        }
        _ => (),
    }
}

fn mvs_resolve(
    env: &mut ResolverEnv,
    root: &[(ModuleSource, Rc<MoonMod>)],
) -> Option<super::result::ResolvedEnv> {
    // Ordered set used to ensure they are iterated in order later.
    let mut gathered_versions = HashMap::<ModuleName, BTreeSet<ModuleSourceOrdWrapper>>::new();

    // Collect all version constraints for each dependency.
    let mut working_list = vec![];
    let mut visited = HashSet::new();

    log::debug!("Begin MVS solving");

    working_list.extend_from_slice(root);
    if log::log_enabled!(log::Level::Debug) {
        for (source, _) in root {
            log::debug!("MVS root item: {}", source);
            visited.insert(source.clone());
        }
    }

    // Do a DFS in the graph
    while let Some((source, module)) = working_list.pop() {
        log::debug!("-- Solving for {}", source);
        let mut all_deps = module.deps.clone();
        all_deps.extend(
            module
                .bin_deps
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, v.into())),
        );
        for (name, req) in &all_deps {
            let pkg_name = match name.parse() {
                Ok(v) => v,
                Err(_) => {
                    env.report_error(ResolverError::MalformedModuleName(
                        module.name.parse().unwrap(),
                        name.clone(),
                    ));
                    continue;
                }
            };

            let (ms, module) = match resolve_pkg(req, &source, env, &pkg_name) {
                Ok(value) => value,
                Err(e) => {
                    env.report_error(e);
                    continue;
                }
            };

            // Add module to working list
            if visited.insert(ms.clone()) {
                working_list.push((ms.clone(), module));
            }

            // Add to gathered versions
            gathered_versions
                .entry(pkg_name)
                .or_default()
                .insert(ms.into());
        }
    }

    if env.any_errors() {
        log::warn!("Errors in MVS dependency solving, bailing out.");
        return None;
    }

    log::debug!("Selecting minimal version for each set of compatible versions");

    // Select the minimal version of each compatible version set.
    let mut settled_versions = HashMap::<ModuleName, BTreeSet<ModuleSourceOrdWrapper>>::new();
    for (name, versions) in gathered_versions {
        log::debug!("-- Module {}", name);

        let mut versions = versions.into_iter().map(|x| x.0);
        let mut curr = versions.next().unwrap();
        log::debug!("---- seen {}", curr);
        for v in versions {
            let caret_curr = as_caret_comparator(curr.version.clone());
            if caret_curr.matches(&v.version) {
                // v >= curr, as implied by btreeset
                // Emit a warning if the skipped dep is local or git, as they are manually specified
                warn_about_skipped_local_or_git_dep(&curr);
                curr = v;
            } else {
                log::debug!("---- selected {}", curr);
                settled_versions
                    .entry(name.clone())
                    .or_default()
                    .insert(curr.into());
                // This starts a new incompatible set.
                curr = v;
            }
            log::debug!("---- seen {}", curr);
        }
        // There's one last version left to be inserted
        log::debug!("---- selected {}", curr);
        settled_versions
            .entry(name)
            .or_default()
            .insert(curr.into());
    }

    log::debug!("Building result dependency graph");

    // And finally, build the dependency graph
    let mut builder = super::result::ResolvedEnv::builder();
    let mut working_list = vec![];
    // id is inserted on first see;
    // may contain items still in working list instead of fully resolved
    let mut visited = HashMap::new();

    log::debug!("-- Inserting root modules");
    // Insert ID for root modules
    for (ms, module) in root {
        let id = builder.add_module(ms.clone(), Rc::clone(module));
        log::debug!("---- {} -> {:?}", ms, id);
        working_list.push((Rc::clone(module), ms.clone()));
        visited.insert(ms, id);
    }

    log::debug!("-- Inserting dependencies");
    while let Some((module, module_source)) = working_list.pop() {
        let pkg = module_source;

        let curr_id = *visited.get(&pkg).unwrap();

        let mut all_deps = module.deps.clone();
        all_deps.extend(
            module
                .bin_deps
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, v.into())),
        );
        for (dep_name, req) in &all_deps {
            let dep_name = dep_name.parse().unwrap();
            // If any malformed name, it should be reported in the previous round

            let dep_versions = &settled_versions[&dep_name];
            let resolved = dep_versions
                .iter()
                .find(|v| req.version.matches(&v.0.version))
                .expect("There should be at least one version available, otherwise previous steps will fail");
            let resolved = &resolved.0;

            let id = if let Some(id) = visited.get(&resolved) {
                *id
            } else {
                let dep_module = env.get(resolved).unwrap();
                let id = builder.add_module(resolved.clone(), Rc::clone(&dep_module));
                log::debug!("---- {} -> {:?}", resolved, id);
                visited.insert(resolved, id);
                working_list.push((dep_module, resolved.clone()));
                id
            };
            log::debug!("---- {}.deps[{}] = {}", pkg, dep_name, resolved);

            // Add dependency
            builder.add_dependency(curr_id, id, &dep_name);
        }
    }

    log::debug!("Finished MVS solving");

    Some(builder.build())
}

fn resolve_pkg(
    req: &SourceDependencyInfo,
    dependant: &ModuleSource,
    env: &mut ResolverEnv,
    pkg_name: &ModuleName,
) -> Result<(ModuleSource, Rc<MoonMod>), ResolverError> {
    if let Some(path) = &req.path {
        if local_dep_allowed(dependant) {
            return resolve_pkg_local(dependant, path, env, pkg_name, req);
        }
    }
    if req.git.is_some() && git_dep_allowed(dependant) {
        return resolve_pkg_git(req, env, pkg_name);
    }
    // If neither git nor local dependencies can be resolved (either because the user
    // didn't specify it at all, or because the repo comes from a registry), we fallback
    // to resolving from a registry.
    let (version, module) = select_min_version_satisfying_in_env(env, pkg_name, req)?;
    log::debug!(
        "---- Dependency {}, required {:?}, selected {}",
        pkg_name,
        req,
        version
    );
    let ms = ModuleSource {
        name: pkg_name.clone(),
        version,
        source: ModuleSourceKind::Registry(None),
    };
    Ok((ms, module))
}

fn resolve_pkg_local(
    dependant: &ModuleSource,
    path: &String,
    env: &mut ResolverEnv,
    pkg_name: &ModuleName,
    req: &SourceDependencyInfo,
) -> Result<(ModuleSource, Rc<MoonMod>), ResolverError> {
    // Try resolving using local dependency
    let root = root_path_of(dependant);
    assert!(
        root.is_absolute(),
        "Root path of {} is not absolute! Got: {}",
        dependant,
        root.display()
    );
    let dep_path = root.join(path);
    let dep_path = dunce::canonicalize(dep_path).map_err(|err| ResolverError::Other(err.into()))?;
    let res = env.resolve_local_module(&dep_path)?;
    let ms = ModuleSource {
        name: pkg_name.clone(),
        version: res.version.clone().expect("Expected version in module"),
        source: ModuleSourceKind::Local(dep_path),
    };
    // Assert version matches
    if let Some(v) = &res.version {
        if !req.version.matches(v) {
            return Err(ResolverError::LocalDepVersionMismatch(
                Box::new(ms),
                req.version.clone(),
            ));
        }
    }

    Ok((ms, res))
}

fn resolve_pkg_git(
    info: &SourceDependencyInfo,
    env: &mut ResolverEnv,
    pkg_name: &ModuleName,
) -> Result<(ModuleSource, Rc<MoonMod>), ResolverError> {
    let git_info = GitSource {
        url: info.git.clone().unwrap(),
        branch: info.git_branch.clone(),
        revision: info.git_revision.clone(),
    };
    let res = env.resolve_git_module(&git_info, pkg_name)?;
    let ms = ModuleSource {
        name: pkg_name.clone(),
        version: res.version.clone().expect("Expected version in module"),
        source: ModuleSourceKind::Git(git_info),
    };
    Ok((ms, res))
}

#[cfg(test)]
mod test {
    use expect_test::expect;
    use petgraph::dot::{Config, Dot};
    use test_log::test;

    use super::*;
    use crate::registry::mock::{create_mock_module, MockRegistry};
    use crate::registry::RegistryList;
    use crate::resolver::env::ResolverEnv;
    use crate::resolver::result::{DependencyKey, ResolvedEnv};
    use crate::resolver::ResolverErrors;

    fn create_mock_registry() -> RegistryList {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("dep/one", "0.1.1", [])
            .add_module_full("dep/one", "0.1.2", [])
            .add_module_full("dep/one", "0.1.3", [])
            .add_module_full("dep/one", "0.2.0", [])
            .add_module_full("dep/one", "0.2.1", []);
        registry
            .add_module_full("dep/two", "0.1.0", [("dep/one", "0.1.2")])
            .add_module_full("dep/two", "0.1.1", [("dep/one", "0.1.3")])
            .add_module_full("dep/two", "0.2.0", [("dep/one", "0.2.0")]);
        registry
            .add_module_full("dep/three", "0.1.0", [("dep/two", "0.1.0")])
            .add_module_full(
                "dep/three",
                "0.2.0",
                [("dep/one", "0.2.0"), ("dep/two", "0.2.0")],
            );
        let reg = Box::new(registry);
        RegistryList::with_registry(reg)
    }

    #[test]
    fn api_walkthrough() {
        let registry = create_mock_registry();

        let mut resolver = MvsSolver;
        let module_name: ModuleName = "dep/three".parse().unwrap();
        let version: Version = "0.1.0".parse().unwrap();
        let root_ms = ModuleSource::from_version(module_name.clone(), version.clone());
        let root = registry
            .get_registry(None)
            .unwrap()
            .get_module_version(&module_name, &version)
            .unwrap();
        let roots = vec![(root_ms.clone(), root)];
        let mut env = ResolverEnv::new(&registry);
        let result = resolver.resolve(&mut env, &roots).expect("Resolve failed");

        let id = result.id_from_mod_name(&root_ms).unwrap();
        expect!["ModuleId(0)"].assert_eq(&format!("{:?}", &id));
        let mt = result.mod_name_from_id(id);
        expect!["dep/three@0.1.0"].assert_eq(&format!("{:?}", mt));

        let module_info = result.module_info(id);
        expect![[r#"
            MoonMod {
                name: "dep/three",
                version: Some(
                    Version {
                        major: 0,
                        minor: 1,
                        patch: 0,
                    },
                ),
                deps: {
                    "dep/two": ^0.1.0,
                },
                bin_deps: None,
                readme: None,
                repository: None,
                license: None,
                keywords: None,
                description: None,
                compile_flags: None,
                link_flags: None,
                checksum: None,
                source: None,
                ext: Null,
                warn_list: None,
                alert_list: None,
                include: None,
                exclude: None,
                scripts: None,
            }
        "#]]
        .assert_debug_eq(module_info);

        let deps = result.deps(id).collect::<Vec<_>>();
        expect![[r#"
            "[ModuleId(1)]"
        "#]]
        .assert_debug_eq(&format!("{:?}", &deps));

        let deps_keyed = result.deps_keyed(id).collect::<Vec<_>>();
        expect![[r#"
            "[(ModuleId(1), dep/two)]"
        "#]]
        .assert_debug_eq(&format!("{:?}", &deps_keyed));

        let key1 = "dep/two".parse::<DependencyKey>().unwrap();
        let key2 = "dep/three".parse::<DependencyKey>().unwrap();
        let x1 = result.dep_with_key(id, &key1);
        let x2 = result.dep_with_key(id, &key2);
        expect!["(Some(ModuleId(1)), None)"].assert_eq(&format!("{:?}", (x1, x2)));

        let dep_count = result.dep_count(id);
        expect!["1"].assert_eq(&dep_count.to_string());

        let all_packages = result.all_packages().collect::<Vec<_>>();
        expect![[r#"
            [
                dep/three@0.1.0,
                dep/two@0.1.0,
                dep/one@0.1.2,
            ]
        "#]]
        .assert_debug_eq(&all_packages);

        let all_packages_and_id = result.all_packages_and_id().collect::<Vec<_>>();
        expect![[r#"
            [
                (
                    ModuleId(
                        0,
                    ),
                    dep/three@0.1.0,
                ),
                (
                    ModuleId(
                        1,
                    ),
                    dep/two@0.1.0,
                ),
                (
                    ModuleId(
                        2,
                    ),
                    dep/one@0.1.2,
                ),
            ]
        "#]]
        .assert_debug_eq(&all_packages_and_id);

        let graph = result.graph();
        expect![[r#"
            digraph {
                0 [ label = "ModuleId(0)" ]
                1 [ label = "ModuleId(1)" ]
                2 [ label = "ModuleId(2)" ]
                0 -> 1 [ ]
                1 -> 2 [ ]
            }
        "#]]
        .assert_eq(&format!(
            "{:?}",
            &Dot::with_config(graph, &[Config::EdgeNoLabel])
        ));
    }

    fn assert_depends_on(result: &ResolvedEnv, pkg1: &str, pkg2: &str) {
        let pkg1 = pkg1.parse().expect("Invalid pkg1");
        let pkg2 = pkg2.parse().expect("Invalid pkg2");
        let id1 = result.id_from_mod_name(&pkg1).expect("pkg1 not found");
        let id2 = result.id_from_mod_name(&pkg2).expect("pkg2 not found");
        assert!(
            result.graph().contains_edge(id1, id2),
            "{} does not depend on {}",
            pkg1,
            pkg2
        );
    }

    fn assert_no_depends_on(result: &ResolvedEnv, pkg1: &str, pkg2: &str) {
        let pkg1 = pkg1.parse().expect("Invalid pkg1");
        let pkg2 = pkg2.parse().expect("Invalid pkg2");
        let id1 = result.id_from_mod_name(&pkg1);
        let id2 = result.id_from_mod_name(&pkg2);
        if let (Some(id1), Some(id2)) = (id1, id2) {
            assert!(
                !result.graph().contains_edge(id1, id2),
                "{} depends on {}",
                pkg1,
                pkg2
            );
        } else {
            // Ok, since at least one of the nodes don't exist at all
        }
    }

    fn create_mock_local_source(module: &MoonMod) -> ModuleSource {
        ModuleSource::from_version(
            module.name.parse().unwrap(),
            module.version.clone().unwrap(),
        )
    }

    fn create_mock_root(root: impl Into<Rc<MoonMod>>) -> Vec<(ModuleSource, Rc<MoonMod>)> {
        let root = root.into();
        let root_src = create_mock_local_source(&root);
        let roots = vec![(root_src, root)];
        roots
    }

    #[test]
    fn test_basic_resolve() {
        let registry = create_mock_registry();
        let mut env = ResolverEnv::new(&registry);
        let mut resolver = MvsSolver;
        let root = create_mock_module("root/module", "0.1.0", [("dep/one", "0.1.1")]);
        let roots = create_mock_root(root);
        let result = resolver.resolve(&mut env, &roots).expect("Resolve failed");
        assert_depends_on(&result, "root/module@0.1.0", "dep/one@0.1.1");
    }

    #[test]
    fn test_dependency_should_be_max_among_requested_version() {
        let registry = create_mock_registry();
        let mut env = ResolverEnv::new(&registry);
        let mut resolver = MvsSolver;
        let root = create_mock_module(
            "root/module",
            "0.1.0",
            [("dep/one", "0.1.1"), ("dep/two", "0.1.1")],
        );
        let roots = create_mock_root(root);
        let result = resolver.resolve(&mut env, &roots).expect("Resolve failed");

        // dep/two depend on dep/one@0.1.3, so the result
        // should be dep/one@0.1.3 instead of 0.1.1
        assert_depends_on(&result, "root/module@0.1.0", "dep/one@0.1.3");
        assert_depends_on(&result, "root/module@0.1.0", "dep/two@0.1.1");
        assert_no_depends_on(&result, "root/module@0.1.0", "dep/one@0.1.1")
    }

    #[test]
    fn test_incompatible_versions() {
        let registry = create_mock_registry();
        let mut env = ResolverEnv::new(&registry);
        let mut resolver = MvsSolver;
        let root = create_mock_module(
            "root/module",
            "0.1.0",
            [("dep/one", "0.2.1"), ("dep/two", "0.1.1")],
        );
        let roots = create_mock_root(root);
        let result = resolver.resolve(&mut env, &roots).expect("Resolve failed");

        // dep/one@0.2.1 is incompatible with dep/two@0.1.1, so there should be
        // two dep/one instances: one is 0.2.1 depended by root/module, and the other
        // is 0.1.3 depended by dep/two

        assert_depends_on(&result, "root/module@0.1.0", "dep/one@0.2.1");
        assert_depends_on(&result, "dep/two@0.1.1", "dep/one@0.1.3");
    }

    #[test]
    fn test_nonexistent_modules() {
        let registry = create_mock_registry();
        let mut env = ResolverEnv::new(&registry);
        let mut resolver = MvsSolver;
        let root = create_mock_module(
            "root/module",
            "0.1.0",
            [("dep/one", "0.1.1"), ("dep/nonexistant", "0.1.1")],
        );
        let roots = create_mock_root(root);
        let result = resolver.resolve(&mut env, &roots);
        assert!(result.is_none());
    }

    #[test]
    fn test_transitive_dependencies() {
        let registry = create_mock_registry();
        let mut env = ResolverEnv::new(&registry);
        let mut resolver = MvsSolver;
        let root = create_mock_module("root/module", "0.1.0", [("dep/three", "0.2.0")]);
        let roots = create_mock_root(root);
        let result = resolver.resolve(&mut env, &roots).expect("Resolve failed");

        assert_depends_on(&result, "root/module@0.1.0", "dep/three@0.2.0");
        assert_depends_on(&result, "dep/two@0.2.0", "dep/one@0.2.0");
        assert_no_depends_on(&result, "root/module@0.1.0", "dep/one@0.2.0");
        assert_no_depends_on(&result, "root/module@0.1.0", "dep/two@0.2.0");
    }

    fn resolve(registry: &RegistryList, root: Rc<MoonMod>) -> Vec<ModuleSource> {
        let mut resolver = MvsSolver;
        let mut env = ResolverEnv::new(registry);
        let roots = create_mock_root(root);
        let result = resolver.resolve(&mut env, &roots);
        match result {
            Some(result) => {
                let pkgs = result.all_packages().cloned().collect::<Vec<_>>();
                pkgs
            }
            None => {
                println!("Errors: {}", ResolverErrors(env.into_errors()));
                vec![]
            }
        }
    }

    fn check_resolve_result(reg: &RegistryList, root: Rc<MoonMod>, expected: expect_test::Expect) {
        expected.assert_debug_eq(&resolve(reg, root));
    }

    // the following tests port from https://github.com/golang/go/blob/22344e1/src/cmd/go/internal/mvs/mvs_test.go

    #[test]
    fn test_blog() {
        // name: blog
        // A: B1 C2
        // B1: D3
        // C1: D2
        // C2: D4
        // C3: D5
        // C4: G1
        // D2: E1
        // D3: E2
        // D4: E2 F1
        // D5: E2
        // G1: C4
        // A2: B1 C4 D4
        // build A:       A B1 C2 D4 E2 F1
        // upgrade* A:    A B1 C4 D5 E2 F1 G1
        // upgrade A C4:  A B1 C4 D4 E2 F1 G1
        // build A2:     A2 B1 C4 D4 E2 F1 G1
        // downgrade A2 D2: A2 C4 D2 E2 F1 G1
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 1), ("C", 2)],
            vec![("B", 1), ("D", 3)],
            vec![("C", 2), ("D", 2)],
            vec![("C", 2), ("D", 4)],
            vec![("C", 3), ("D", 5)],
            vec![("C", 4), ("G", 1)],
            vec![("D", 2), ("E", 1)],
            vec![("D", 3), ("E", 2)],
            vec![("D", 4), ("E", 2), ("F", 1)],
            vec![("D", 5), ("E", 2)],
            vec![("G", 1), ("C", 4)],
            vec![("A", 2), ("B", 1), ("C", 4), ("D", 4)],
        ]);

        let root1 = registry.get_module("t/A", "0.1.0");
        let root2 = registry.get_module("t/A", "0.1.2");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root1,
            expect_test::expect![[r#"
            [
                t/A@0.1.0,
                t/B@0.1.1,
                t/C@0.1.2,
                t/D@0.1.4,
                t/E@0.1.2,
                t/F@0.1.1,
            ]
        "#]],
        );

        check_resolve_result(
            &rl,
            root2,
            expect_test::expect![[r#"
            [
                t/A@0.1.2,
                t/B@0.1.1,
                t/C@0.1.4,
                t/D@0.1.4,
                t/E@0.1.2,
                t/F@0.1.1,
                t/G@0.1.1,
            ]
        "#]],
        );
    }

    #[test]
    fn test_trim() {
        // name: trim
        // A: B1 C2
        // B1: D3
        // C2: B2
        // B2:
        // build A: A B2 C2 D3
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 1), ("C", 2)],
            vec![("B", 1), ("D", 3)],
            vec![("C", 2), ("B", 2)],
            vec![("B", 2)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
            [
                t/A@0.1.0,
                t/B@0.1.2,
                t/C@0.1.2,
            ]
        "#]],
        );
    }

    #[test]
    fn test_cross1() {
        // name: cross1
        // A: B C
        // B: D1
        // C: D2
        // D1: E2
        // D2: E1
        // build A: A B C D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 0), ("C", 0)],
            vec![("B", 0), ("D", 1)],
            vec![("C", 0), ("D", 2)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.0,
                    t/C@0.1.0,
                    t/D@0.1.2,
                    t/E@0.1.2,
                ]
            "#]],
        );
    }

    #[test]
    fn test_cross1v() {
        // name: cross1V
        // A: B2 C D2 E1
        // B1:
        // B2: D1
        // C: D2
        // D1: E2
        // D2: E1
        // build A: A B2 C D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 2), ("C", 0), ("D", 2), ("E", 1)],
            vec![("B", 2), ("D", 1)],
            vec![("C", 0), ("D", 2)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
            [
                t/A@0.1.0,
                t/B@0.1.2,
                t/C@0.1.0,
                t/D@0.1.2,
                t/E@0.1.2,
            ]
        "#]],
        );
    }

    #[test]
    fn test_cross1u() {
        // name: cross1U
        // A: B1 C
        // B1:
        // B2: D1
        // C: D2
        // D1: E2
        // D2: E1
        // build A:      A B1 C D2 E1
        // upgrade A B2: A B2 C D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 1), ("C", 0)],
            vec![("B", 1)],
            vec![("B", 2), ("D", 1)],
            vec![("C", 0), ("D", 2)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
            [
                t/A@0.1.0,
                t/B@0.1.1,
                t/C@0.1.0,
                t/D@0.1.2,
                t/E@0.1.1,
            ]
        "#]],
        );
    }

    #[test]
    fn test_cross1r() {
        // name: cross1R
        // A: B C
        // B: D2
        // C: D1
        // D1: E2
        // D2: E1
        // build A: A B C D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 0), ("C", 0)],
            vec![("B", 0), ("D", 2)],
            vec![("C", 0), ("D", 1)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
            [
                t/A@0.1.0,
                t/B@0.1.0,
                t/C@0.1.0,
                t/D@0.1.2,
                t/E@0.1.2,
            ]
        "#]],
        );
    }
    #[test]
    fn test_cross1x() {
        // name: cross1X
        // A: B C
        // B: D1 E2
        // C: D2
        // D1: E2
        // D2: E1
        // build A: A B C D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 0), ("C", 0)],
            vec![("B", 0), ("D", 1), ("E", 2)],
            vec![("C", 0), ("D", 2)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
            [
                t/A@0.1.0,
                t/B@0.1.0,
                t/C@0.1.0,
                t/D@0.1.2,
                t/E@0.1.2,
            ]
        "#]],
        )
    }

    #[test]
    fn test_cross2() {
        // name: cross2
        // A: B D2
        // B: D1
        // D1: E2
        // D2: E1
        // build A: A B D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 0), ("D", 2)],
            vec![("B", 0), ("D", 1)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
            [
                t/A@0.1.0,
                t/B@0.1.0,
                t/D@0.1.2,
                t/E@0.1.2,
            ]
        "#]],
        );
    }

    #[test]
    fn test_cross2x() {
        // name: cross2X
        // A: B D2
        // B: D1 E2
        // C: D2
        // D1: E2
        // D2: E1
        // build A: A B D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 0), ("D", 2)],
            vec![("B", 0), ("D", 1), ("E", 2)],
            vec![("C", 0), ("D", 2)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
            [
                t/A@0.1.0,
                t/B@0.1.0,
                t/D@0.1.2,
                t/E@0.1.2,
            ]
        "#]],
        );
    }

    #[test]
    fn test_cross3() {
        // name: cross3
        // A: B D2 E1
        // B: D1
        // D1: E2
        // D2: E1
        // build A: A B D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 0), ("D", 2), ("E", 1)],
            vec![("B", 0), ("D", 1)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
            [
                t/A@0.1.0,
                t/B@0.1.0,
                t/D@0.1.2,
                t/E@0.1.2,
            ]
        "#]],
        );
    }

    #[test]
    fn test_cross4() {
        // name: cross4
        // A1: B1 D2
        // A2: B2 D2
        // B1: D1
        // B2: D2
        // D1: E2
        // D2: E1
        // build A1: A1 B1 D2 E2
        // build A2: A2 B2 D2 E1
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 1), ("B", 1), ("D", 2)],
            vec![("A", 2), ("B", 2), ("D", 2)],
            vec![("B", 1), ("D", 1)],
            vec![("B", 2), ("D", 2)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);

        let root = registry.get_module("t/A", "0.1.1");
        let root2 = registry.get_module("t/A", "0.1.2");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.1,
                    t/B@0.1.1,
                    t/D@0.1.2,
                    t/E@0.1.2,
                ]
            "#]],
        );

        check_resolve_result(
            &rl,
            root2,
            expect![[r#"
                [
                    t/A@0.1.2,
                    t/B@0.1.2,
                    t/D@0.1.2,
                    t/E@0.1.1,
                ]
            "#]],
        );
    }

    #[test]
    fn test_cross5() {
        // name: cross5
        // A: D1
        // D1: E2
        // D2: E1
        // build A:       A D1 E2
        // upgrade* A:    A D2 E2
        // upgrade A D2:  A D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("D", 1)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);

        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
            [
                t/A@0.1.0,
                t/D@0.1.1,
                t/E@0.1.2,
            ]
        "#]],
        );
    }

    #[test]
    fn test_cross6() {
        // name: cross6
        // A: D2
        // D1: E2
        // D2: E1
        // build A:      A D2 E1
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("D", 2)],
            vec![("D", 1), ("E", 2)],
            vec![("D", 2), ("E", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
        [
            t/A@0.1.0,
            t/D@0.1.2,
            t/E@0.1.1,
        ]
    "#]],
        );
    }

    #[test]
    fn test_cross7() {
        // name: cross7
        // A: B C
        // B: D1
        // C: E1
        // D1: E2
        // E1: D2
        // build A: A B C D2 E2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 0), ("C", 0)],
            vec![("B", 0), ("D", 1)],
            vec![("C", 0), ("E", 1)],
            vec![("D", 1), ("E", 2)],
            vec![("E", 1), ("D", 2)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.0,
                    t/C@0.1.0,
                    t/E@0.1.2,
                    t/D@0.1.2,
                ]
            "#]],
        );
    }

    #[test]
    fn test_cross8() {
        // name: cross8
        // M: A1 B1
        // A1: X1
        // B1: X2
        // X1: I1
        // X2:
        // build M: M A1 B1 I1 X2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("M", 0), ("A", 1), ("B", 1)],
            vec![("A", 1), ("X", 1)],
            vec![("B", 1), ("X", 2)],
            vec![("X", 1), ("I", 1)],
            vec![("X", 2)],
        ]);
        let root = registry.get_module("t/M", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/M@0.1.0,
                    t/A@0.1.1,
                    t/B@0.1.1,
                    t/X@0.1.2,
                ]
            "#]],
        );
    }

    #[test]
    fn test_drop() {
        // name: drop
        // A: B1 C1
        // B1: D1
        // B2:
        // C2:
        // D2:
        // build A:    A B1 C1 D1
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 1), ("C", 1)],
            vec![("B", 1), ("D", 1)],
            vec![("B", 2)],
            vec![("C", 2)],
            vec![("D", 2)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.1,
                    t/C@0.1.1,
                    t/D@0.1.1,
                ]
            "#]],
        );
    }

    #[test]
    fn test_simplify() {
        // name: simplify
        // A: B1 C1
        // B1: C2
        // C1: D1
        // C2:
        // build A: A B1 C2 D1
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 1), ("C", 1)],
            vec![("B", 1), ("C", 2)],
            vec![("C", 1), ("D", 1)],
            vec![("C", 2)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.1,
                    t/C@0.1.2,
                ]
            "#]],
        );
    }

    #[test]
    #[ignore = "not support hidden"]
    fn test_up1() {
        // name: up1
        // A: B1 C1
        // B1:
        // B2:
        // B3:
        // B4:
        // B5.hidden:
        // C2:
        // C3:
        // build A:    A B1 C1
        // upgrade* A: A B4 C3
    }

    #[test]
    #[ignore = "not support hidden"]
    fn test_up2() {
        // name: up2
        // A: B5.hidden C1
        // B1:
        // B2:
        // B3:
        // B4:
        // B5.hidden:
        // C2:
        // C3:
        // build A:    A B5.hidden C1
        // upgrade* A: A B5.hidden C3
    }

    #[test]
    fn test_down1() {
        // name: down1
        // A: B2
        // B1: C1
        // B2: C2
        // build A:        A B2 C2
        // downgrade A C1: A B1 C1
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 2)],
            vec![("B", 1), ("C", 1)],
            vec![("B", 2), ("C", 2)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.2,
                    t/C@0.1.2,
                ]
            "#]],
        );
    }

    #[test]
    fn test_down2() {
        // name: down2
        // A: B2 E2
        // B1:
        // B2: C2 F2
        // C1:
        // D1:
        // C2: D2 E2
        // D2: B2
        // E2: D2
        // E1:
        // F1:
        // build A:        A B2 C2 D2 E2 F2
        // downgrade A F1: A B1 C1 D1 E1 F1
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 2), ("E", 2)],
            vec![("B", 1)],
            vec![("B", 2), ("C", 2), ("F", 2)],
            vec![("C", 1)],
            vec![("D", 1)],
            vec![("C", 2), ("D", 2), ("E", 2)],
            vec![("D", 2), ("B", 2)],
            vec![("E", 2), ("D", 2)],
            vec![("E", 1)],
            vec![("F", 1)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.2,
                    t/E@0.1.2,
                    t/D@0.1.2,
                    t/C@0.1.2,
                    t/F@0.1.2,
                ]
            "#]],
        );
    }

    #[test]
    fn test_downcross1() {
        // name: downcross1
        // A: B2 C1
        // B1: C2
        // B2: C1
        // C1: D2
        // C2:
        // D1:
        // D2:
        // build A:        A B2 C1 D2
        // downgrade A D1: A       D1
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 2), ("C", 1)],
            vec![("B", 1), ("C", 2)],
            vec![("B", 2), ("C", 1)],
            vec![("C", 1), ("D", 2)],
            vec![("D", 1)],
            vec![("D", 2)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.2,
                    t/C@0.1.1,
                    t/D@0.1.2,
                ]
            "#]],
        );
    }

    #[test]
    fn test_downcross2() {
        // name: downcross2
        // A: B2
        // B1: C1
        // B2: D2
        // C1:
        // D1:
        // D2:
        // build A:        A B2    D2
        // downgrade A D1: A B1 C1 D1
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 2)],
            vec![("B", 1), ("C", 1)],
            vec![("B", 2), ("D", 2)],
            vec![("D", 1)],
            vec![("D", 2)],
        ]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.2,
                    t/D@0.1.2,
                ]
            "#]],
        );
    }

    #[test]
    fn test_downcycle() {
        // name: downcycle
        // A: A B2
        // B2: A
        // B1:
        // build A:        A B2
        // downgrade A B1: A B1
        let mut registry = MockRegistry::new();
        registry.parse(vec![vec![("A", 0), ("B", 2)], vec![("B", 2), ("A", 0)]]);
        let root = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.2,
                ]
            "#]],
        );
    }

    #[test]
    #[ignore = "not support hidden"]
    fn test_downhiddenartifact() {
        // name: downhiddenartifact
        // A: B3 C2
        // A1: B3
        // B1: E1
        // B2.hidden:
        // B3: D2
        // C1: B2.hidden
        // C2: D2
        // D1:
        // D2:
        // build A1: A1 B3 D2
        // downgrade A1 D1: A1 B1 D1 E1
        // build A: A B3 C2 D2
        // downgrade A D1: A B2.hidden C1 D1
    }

    #[test]
    #[ignore = "not support hidden"]
    fn test_downhiddencross() {
        // name: downhiddencross
        // A: B3 C3
        // B1: C2.hidden
        // B2.hidden:
        // B3: D2
        // C1: B2.hidden
        // C2.hidden:
        // C3: D2
        // D1:
        // D2:
        // build A: A B3 C3 D2
        // downgrade A D1: A B2.hidden C2.hidden D1
    }

    #[test]
    #[ignore = "not support hidden"]
    fn test_noprev1() {
        // name: noprev1
        // A: B4 C2
        // B2.hidden:
        // C2:
        // build A:               A B4        C2
        // downgrade A B2.hidden: A B2.hidden C2
    }

    #[test]
    #[ignore = "not support hidden"]
    fn test_noprev2() {
        // name: noprev2
        // A: B4 C2
        // B2.hidden:
        // B1:
        // C2:
        // build A:               A B4        C2
        // downgrade A B2.hidden: A B2.hidden C2
    }

    #[test]
    #[ignore = "not support hidden"]
    fn test_noprev3() {
        // name: noprev3
        // A: B4 C2
        // B3:
        // B2.hidden:
        // C2:
        // build A:               A B4        C2
        // downgrade A B2.hidden: A B2.hidden C2
    }

    #[test]
    #[ignore = "Cyclic dependency detection is not supported yet"]
    fn test_cycle1() {
        // name: cycle1
        // A: B1
        // B1: A1
        // B2: A2
        // B3: A3
        // build A:      A B1
        // upgrade A B2: A B2
        // upgrade* A:   A B3
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 1)],
            vec![("B", 1), ("A", 1)],
            vec![("B", 2), ("A", 2)],
            vec![("B", 3), ("A", 3)],
        ]);
        let root_initial = registry.get_module("t/A", "0.1.0");
        let root_upgrade_b2 = registry.get_module("t/A", "0.1.0");
        let root_upgrade_star = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root_initial,
            expect![[r#"
                []
            "#]],
        );

        check_resolve_result(
            &rl,
            root_upgrade_b2,
            expect![[r#"
                []
            "#]],
        );

        check_resolve_result(
            &rl,
            root_upgrade_star,
            expect![[r#"
                []
            "#]],
        );
    }

    #[test]
    #[ignore = "Cyclic dependency detection is not supported yet"]
    fn test_cycle2() {
        // name: cycle2
        // A: B1
        // A1: C1
        // A2: D1
        // B1: A1
        // B2: A2
        // C1: A2
        // C2:
        // D2:
        // build A:    A B1 C1 D1
        // upgrade* A: A B2 C2 D2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("A", 0), ("B", 1)],
            vec![("A", 1), ("C", 1)],
            vec![("A", 2), ("D", 1)],
            vec![("B", 1), ("A", 1)],
            vec![("B", 2), ("A", 2)],
            vec![("C", 1), ("A", 2)],
        ]);
        let root_initial = registry.get_module("t/A", "0.1.0");
        let root_upgrade_star = registry.get_module("t/A", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root_initial,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.1,
                    t/A@0.1.2,
                    t/D@0.1.1,
                ]
            "#]],
        );

        check_resolve_result(
            &rl,
            root_upgrade_star,
            expect![[r#"
                [
                    t/A@0.1.0,
                    t/B@0.1.1,
                    t/A@0.1.2,
                    t/D@0.1.1,
                ]
            "#]],
        );
    }

    #[test]
    #[ignore = "Cyclic dependency detection is not supported yet"]
    fn test_cycle3() {
        // name: cycle3
        // M: A1 C2
        // A1: B1
        // B1: C1
        // B2: C2
        // C1:
        // C2: B2
        // build M: M A1 B2 C2
        // req M:     A1 B2
        // req M A:   A1 B2
        // req M C:   A1 C2
        let mut registry = MockRegistry::new();
        registry.parse(vec![
            vec![("M", 0), ("A", 1), ("C", 2)],
            vec![("A", 1), ("B", 1)],
            vec![("B", 1), ("C", 1)],
            vec![("B", 2), ("C", 2)],
            vec![("C", 1)],
            vec![("C", 2), ("B", 2)],
        ]);
        let root = registry.get_module("t/M", "0.1.0");
        let rl = RegistryList::with_registry(Box::new(registry));
        check_resolve_result(
            &rl,
            root,
            expect![[r#"
                [
                    t/M@0.1.0,
                    t/A@0.1.1,
                    t/C@0.1.2,
                    t/B@0.1.2,
                ]
            "#]],
        );
    }
}
