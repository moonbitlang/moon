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

use std::path::PathBuf;

use moonbuild_rupes_recta::discover::discover_packages;
use moonutil::common::read_module_desc_file_in_dir;
use moonutil::mooncakes::{DirSyncResult, ModuleSource, result::ResolvedEnv};

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
#[allow(clippy::disallowed_methods)]
fn discover_skips_nested_module_with_moon_mod() {
    let root = fixture_dir("module_dsl_skip_nested");
    let module = read_module_desc_file_in_dir(&root).expect("read module");
    let source = ModuleSource::from_local_module(&module, &root);
    let (resolved, module_id) = ResolvedEnv::only_one_module(source, module);

    let mut dirs = DirSyncResult::new();
    dirs.insert(module_id, root.clone());

    let discovered = discover_packages(&resolved, &dirs).expect("discover packages");
    let mut packages = discovered
        .all_packages(false)
        .map(|(_, pkg)| pkg.fqn.to_string())
        .collect::<Vec<_>>();
    packages.sort();
    let actual = packages.join("\n");

    assert_eq!(actual, "example/root\nexample/root/lib");
}
