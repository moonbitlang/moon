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

use std::sync::Arc;

use mooncake::{
    MoonMod, UserLog,
    pkg::sync::{
        CacheRoot, ModuleSource, ResolvedModule, ResolvedRootModules, SyncOptions, sync_roots,
    },
};
use semver::Version;

#[test]
fn sync_uses_supplied_root_declarations_without_manifests() {
    // A library client can supply several roots from memory, including edits
    // not yet persisted. The roots have source directories but no manifests.
    let temp = tempfile::tempdir().unwrap();
    let app_dir = temp.path().join("app");
    let lib_dir = temp.path().join("lib");
    std::fs::create_dir(&app_dir).unwrap();
    std::fs::create_dir(&lib_dir).unwrap();
    let mut app = MoonMod {
        name: "example/app".into(),
        version: Some(Version::new(0, 1, 0)),
        ..Default::default()
    };
    app.deps
        .insert("example/lib".into(), "0.1.0".parse().unwrap());
    let lib = MoonMod {
        name: "example/lib".into(),
        version: Some(Version::new(0, 1, 0)),
        ..Default::default()
    };
    let mut roots = ResolvedRootModules::with_key();
    let app_id = roots.insert(ResolvedModule::new(
        ModuleSource::from_local_module(&app, &app_dir).unwrap(),
        Arc::new(app),
    ));
    let lib_id = roots.insert(ResolvedModule::new(
        ModuleSource::from_local_module(&lib, &lib_dir).unwrap(),
        Arc::new(lib),
    ));

    let (graph, paths) = sync_roots(
        roots,
        &temp.path().join(".mooncakes"),
        &CacheRoot::Disabled,
        &SyncOptions {
            frozen: true,
            jobs: Some(1),
            ..Default::default()
        },
        &UserLog::new(log::LevelFilter::Off),
    )
    .unwrap();

    assert_eq!(graph.input_module_ids(), &[app_id, lib_id]);
    assert!(graph.deps(app_id).any(|id| id == lib_id));
    assert_eq!(paths[app_id], app_dir);
    assert_eq!(paths[lib_id], lib_dir);
}
