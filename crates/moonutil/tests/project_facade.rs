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

use moonutil::project::{ModuleRef, ProjectContext};

#[test]
fn project_facade_exposes_selected_module_ref() {
    let selected_module = ModuleRef {
        root: PathBuf::from("module"),
        manifest_path: PathBuf::from("module/moon.mod.json"),
    };
    let project = ProjectContext::Workspace {
        root: PathBuf::from("."),
        manifest_path: PathBuf::from("moon.work"),
        selected_module: Some(selected_module.clone()),
    };

    let returned: Option<ModuleRef> = project.selected_module();
    assert_eq!(returned, Some(selected_module));
}
