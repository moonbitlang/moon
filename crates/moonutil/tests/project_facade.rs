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

use moonutil::project::{ModuleRef, ProjectContext};

#[test]
fn project_facade_exposes_selected_module_ref() {
    let selected_module = ModuleRef {
        root: PathBuf::from("module"),
        manifest_path: PathBuf::from("module/moon.mod.json"),
    };
    let project = ProjectContext::Workspace {
        root: PathBuf::from("."),
        manifest_path: PathBuf::from("moon.work.json"),
        selected_module: Some(selected_module.clone()),
    };

    let returned: Option<ModuleRef> = project.selected_module();
    assert_eq!(returned, Some(selected_module));
}
