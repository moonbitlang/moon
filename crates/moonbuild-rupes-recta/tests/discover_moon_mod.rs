// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::path::PathBuf;

use moonbuild_rupes_recta::discover::discover_packages;
use moonutil::manifest::read_module_desc_file_in_dir;
use moonutil::resolution::{DirSyncResult, ModuleSource, ResolvedEnv};
use moonutil::user_log::UserLog;

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

    let discovered = discover_packages(&resolved, &dirs, &UserLog::new(log::LevelFilter::Error))
        .expect("discover packages");
    let mut packages = discovered
        .all_packages(false)
        .map(|(_, pkg)| pkg.fqn.to_string())
        .collect::<Vec<_>>();
    packages.sort();
    let actual = packages.join("\n");

    assert_eq!(actual, "example/root\nexample/root/lib");
}
