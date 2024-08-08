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

//! Utilities to handle the local package directory, `.mooncakes`.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use moonutil::{
    common::{DEP_PATH, MOONBITLANG_CORE},
    moon_dir,
    mooncakes::{result::ResolvedEnv, DirSyncResult, ModuleSource, ModuleSourceKind},
};
use semver::Version;

use crate::registry::RegistryList;

fn dep_dir_of(source_dir: &Path) -> PathBuf {
    source_dir.join(DEP_PATH)
}

type DepDirState = HashMap<String, HashMap<String, Option<Version>>>;
type NewDepDirState<'a> = HashMap<String, HashMap<String, &'a ModuleSource>>;

/// The dependencies directory
///
/// # Note about folder structures
///
/// A MoonBit module name contains two parts: `username` and `pkgname`. The two parts
/// are separated by slash `/`, and the latter part can contain any number of slashes
/// itself. However, we don't want to have an arbitrary depth of directories in our
/// dependencies directory for ease of scanning. Instead, we replace all slashes
/// in the `pkgname` part with plus `+` sign. For example, a package
/// `foo/bar/baz` will be stored in the directory `foo/bar+baz`.
pub struct DepDir {
    path: PathBuf,
}

impl DepDir {
    pub fn of_source(source_dir: &Path) -> Self {
        DepDir {
            path: dep_dir_of(source_dir),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns a list of currently installed packages.
    pub fn get_current_state(&self) -> std::io::Result<DepDirState> {
        let it = self.path().read_dir()?;
        let mut user_list = HashMap::new();
        for entry in it {
            let entry = entry?;
            // Ignore all non-directory entries.
            if !entry.file_type()?.is_dir() {
                continue;
            }
            // These are user names. For each user name, we need to find all
            // package names within them.

            let pkgs = entry.path().read_dir()?;
            let mut pkg_list = HashMap::new();
            for pkg in pkgs {
                let pkg = pkg?;
                // Ignore all non-directory entries.
                if !pkg.file_type()?.is_dir() {
                    continue;
                }
                let pkg_name = pkg.file_name().to_string_lossy().replace('+', "/");
                let module = moonutil::common::read_module_desc_file_in_dir(&pkg.path());
                let version = module.map(|m| m.version).ok().flatten();
                pkg_list.insert(pkg_name, version);
            }
            user_list.insert(entry.file_name().to_string_lossy().into(), pkg_list);
        }

        Ok(user_list)
    }
}

fn pkg_list_to_dep_dir_state<'a>(
    pkg_list: impl Iterator<Item = &'a ModuleSource>,
) -> NewDepDirState<'a> {
    let mut user_list = HashMap::new();
    for pkg in pkg_list {
        match &pkg.source {
            ModuleSourceKind::Registry(_) => {}
            ModuleSourceKind::Local(_) => continue,
            ModuleSourceKind::Git(_) => continue, // TODO: git registries are resolved differently
        }
        let user = &pkg.name.username;
        let pkg_name = &pkg.name.pkgname;
        let pkg_list: &mut HashMap<String, _> = user_list.entry(user.to_owned()).or_default();
        pkg_list.insert(pkg_name.to_owned(), pkg);
    }
    user_list
}

struct DepDirStateDiff<'a> {
    add_user: HashSet<String>,
    remove_user: HashSet<String>,
    add_pkg: HashMap<String, HashMap<String, &'a ModuleSource>>,
    remove_pkg: HashMap<String, HashSet<String>>,
}

fn diff_dep_dir_state<'a>(
    current: &'a DepDirState,
    target: &'a NewDepDirState<'a>,
) -> DepDirStateDiff<'a> {
    // First, we need to find out which users are added and removed.
    let mut add_user = HashSet::new();
    let mut remove_user = HashSet::new();
    for user in target.keys() {
        if !current.contains_key(user) {
            add_user.insert(user.clone());
        }
    }
    for user in current.keys() {
        if !target.contains_key(user) {
            remove_user.insert(user.clone());
        }
    }

    // Then, we need to find out which packages are added and removed.
    let mut add_pkg = HashMap::new();
    let mut remove_pkg = HashMap::new();
    for (user, tgt_pkgs) in target {
        let _empty = HashMap::new(); // Empty package list, does not allocate
        let current_pkgs = current.get(user).unwrap_or(&_empty);
        let mut add_pkg_list = HashMap::new();
        let mut remove_pkg_list = HashSet::new();
        for (pkg, new_source) in tgt_pkgs {
            match current_pkgs.get(pkg) {
                None => {
                    // If the package is not in the current state, it's added.
                    add_pkg_list.insert(pkg.clone(), *new_source);
                }
                Some(current_version) => {
                    if current_version.is_none()
                        || current_version.as_ref().unwrap() != &new_source.version
                    {
                        // On version mismatch, we remove and re-add the package.
                        remove_pkg_list.insert(pkg.clone());
                        add_pkg_list.insert(pkg.clone(), *new_source);
                    }
                    // otherwise, the package is already installed and we don't need to do anything.
                }
            }
        }
        // Remove all packages that are not in the target list.
        for pkg in current_pkgs.keys() {
            if !tgt_pkgs.contains_key(pkg) {
                remove_pkg_list.insert(pkg.clone());
            }
        }

        if !add_pkg_list.is_empty() {
            add_pkg.insert(user.clone(), add_pkg_list.into_iter().collect());
        }
        if !remove_pkg_list.is_empty() && !remove_user.contains(user) {
            remove_pkg.insert(user.clone(), remove_pkg_list);
        }
    }

    DepDirStateDiff {
        add_user,
        remove_user,
        add_pkg,
        remove_pkg,
    }
}

/// Sync the dependencies directory with the target package list.
pub fn sync_deps(
    dep_dir: &DepDir,
    registries: &RegistryList,
    pkg_list: &ResolvedEnv,
    quiet: bool,
) -> anyhow::Result<()> {
    // Ensure the directory exists.
    std::fs::create_dir_all(dep_dir.path())?;

    let target_dep_dir = pkg_list_to_dep_dir_state(pkg_list.all_packages());
    let current_dep_dir = dep_dir.get_current_state()?;

    let diff = diff_dep_dir_state(&current_dep_dir, &target_dep_dir);

    // First, remove all packages that are no longer needed.
    for (user, pkgs) in diff.remove_pkg {
        for pkg in pkgs {
            let pkg_path = dep_dir.path().join(&user).join(&pkg);
            log::info!("Removing package {} at {}", &pkg, pkg_path.display());
            std::fs::remove_dir_all(pkg_path)?; // TODO: use async
        }
    }
    // Remove users that are no longer needed.
    for user in diff.remove_user {
        let user_path = dep_dir.path().join(&user);
        log::info!("Removing user {} at {}", &user, user_path.display());
        std::fs::remove_dir_all(user_path)?;
    }

    // Then, create all users that are needed.
    for user in diff.add_user {
        let user_path = dep_dir.path().join(&user);
        log::info!("Creating user {} at {}", &user, user_path.display());
        std::fs::create_dir_all(user_path)?;
    }
    // Finally, Download and install packages that are needed.
    for (user, pkgs) in diff.add_pkg {
        for (pkg, version) in pkgs {
            let pkg_path = pkg_to_dir(dep_dir, &user, &pkg);
            log::info!(
                "Installing package {:?} at {}",
                &version,
                pkg_path.display()
            );
            let ModuleSourceKind::Registry(registry) = &version.source else {
                unreachable!()
            };
            registries
                .get_registry(registry.as_deref())
                .expect("Registry not found")
                .install_to(&version.name, &version.version, &pkg_path, quiet)?;
            // TODO: parallelize this
        }
    }

    Ok(())
}

fn pkg_to_dir(dep_dir: &DepDir, username: &str, pkgname: &str) -> PathBuf {
    // Special case: core library locates in ~/.moon
    if format!("{username}/{pkgname}") == MOONBITLANG_CORE {
        return moon_dir::core();
    }
    let pkg_dir_name = pkgname.replace('/', "+");
    dep_dir.path().join(username).join(pkg_dir_name)
}

/// The result of a directory sync.

fn map_source_to_dir(dep_dir: &DepDir, module: &ModuleSource) -> PathBuf {
    match &module.source {
        ModuleSourceKind::Registry(_) => {
            pkg_to_dir(dep_dir, &module.name.username, &module.name.pkgname)
        }
        ModuleSourceKind::Local(path) => path.clone(),
        ModuleSourceKind::Git(url) => crate::resolver::git::resolve(url).unwrap(),
    }
}

/// Resolve the directory for each module in this build.
///
/// Assumes [`sync_deps`] is already called. Otherwise, modules might point to
/// directories that don't exist yet because they are not synced yet.
pub fn resolve_dep_dirs(dep_dir: &DepDir, pkg_list: &ResolvedEnv) -> DirSyncResult {
    let mut res = DirSyncResult::default();
    for (id, module) in pkg_list.all_packages_and_id() {
        res.insert(id, map_source_to_dir(dep_dir, module));
    }
    res
}

#[cfg(test)]
mod test {
    use std::collections::{HashMap, HashSet};

    use moonutil::mooncakes::{ModuleName, ModuleSource, ModuleSourceKind};

    fn to_state(s: &str) -> super::DepDirState {
        let mut state = super::DepDirState::new();
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split(':');
            let user = parts.next().unwrap().to_owned();
            let pkgs = parts.next().unwrap();
            let mut pkg_set = HashMap::new();
            for pkg in pkgs.split(',') {
                let (pkg_name, version) = pkg.split_once('@').unwrap();
                let pkg_name = pkg_name.to_owned();
                let version = version.parse().unwrap();
                pkg_set.insert(pkg_name, Some(version));
            }
            state.insert(user, pkg_set);
        }
        state
    }

    fn to_new_state(s: &str) -> super::NewDepDirState {
        let state = to_state(s);
        state
            .into_iter()
            .map(|(user, v)| {
                let new_v = v
                    .into_iter()
                    .map(|(module, v)| {
                        let version = v.unwrap();
                        let res = ModuleSource {
                            name: ModuleName {
                                username: user.clone(),
                                pkgname: module.clone(),
                            },
                            version,
                            source: ModuleSourceKind::Registry(None),
                        };
                        let leaked = Box::leak(Box::new(res));
                        (module, &*leaked)
                    })
                    .collect();
                (user, new_v)
            })
            .collect()
    }

    fn to_add_pkg(
        input: Vec<(&str, Vec<(&str, &str)>)>,
    ) -> HashMap<String, HashMap<String, &'static ModuleSource>> {
        input
            .into_iter()
            .map(|(user, v)| {
                let new_v = v
                    .into_iter()
                    .map(|(pkg, ver)| {
                        let version = ver.parse().unwrap();
                        let res = ModuleSource {
                            name: ModuleName {
                                username: user.to_owned(),
                                pkgname: pkg.to_owned(),
                            },
                            version,
                            source: ModuleSourceKind::Registry(None),
                        };
                        let leaked = Box::leak(Box::new(res));
                        (pkg.to_owned(), &*leaked)
                    })
                    .collect();
                (user.to_owned(), new_v)
            })
            .collect()
    }

    fn to_remove_pkg(input: Vec<(&str, Vec<&str>)>) -> HashMap<String, HashSet<String>> {
        input
            .into_iter()
            .map(|(user, pkgs)| {
                let pkgs_set = pkgs.into_iter().map(|pkg| pkg.to_owned()).collect();
                (user.to_owned(), pkgs_set)
            })
            .collect()
    }

    #[test]
    fn test_diff_state() {
        let original = to_state(
            r#"
            user1:pkg1@1.0.0,pkg2@2.0.0
            user2:pkg3@3.0.0
            "#,
        );
        let target = to_new_state(
            r#"
            user1:pkg1@1.0.0,pkg2@2.0.0,pkg4@4.0.0
            user3:pkg5@5.0.0
            "#,
        );
        let diff = super::diff_dep_dir_state(&original, &target);
        assert_eq!(
            diff.add_user,
            vec!["user3"].into_iter().map(|x| x.to_owned()).collect(),
            "add user"
        );
        assert_eq!(
            diff.remove_user,
            vec!["user2"].into_iter().map(|x| x.to_owned()).collect(),
            "remove user"
        );
        assert_eq!(
            diff.add_pkg,
            to_add_pkg(vec![
                ("user1", vec![("pkg4", "4.0.0"),]),
                ("user3", vec![("pkg5", "5.0.0"),])
            ]),
            "add pkg"
        );
        assert!(diff.remove_pkg.is_empty(), "remove pkg");
    }

    #[test]
    fn test_diff_state_2() {
        let original = to_state(
            r"
        user1:foo@1.0.0,bar@1.2.0
        user2:foo@2.0.1,bar@2.2.0
        ",
        );
        let target = to_new_state(
            r"
        user1:foo@1.2.0,bar@1.4.0
        user2:foo@2.2.0
        ",
        );
        let diff = super::diff_dep_dir_state(&original, &target);
        assert!(diff.add_user.is_empty(), "add user");
        assert!(diff.remove_user.is_empty(), "remove user");
        assert_eq!(
            diff.add_pkg,
            to_add_pkg(vec![
                ("user1", vec![("foo", "1.2.0"), ("bar", "1.4.0"),]),
                ("user2", vec![("foo", "2.2.0"),])
            ]),
            "add pkg"
        );
        assert_eq!(
            diff.remove_pkg,
            to_remove_pkg(vec![
                ("user1", vec!["foo", "bar"]),
                ("user2", vec!["foo", "bar"])
            ]),
            "remove pkg"
        );
    }
}
