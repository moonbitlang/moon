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

use std::{borrow::Cow, path::Path};

use crate::build_lower::compiler::CmdlineAbstraction;

#[derive(Debug)]
pub(crate) struct MoonGenerateNodeTestPackageConfig<'a> {
    pub output: Cow<'a, Path>,
}

impl CmdlineAbstraction for MoonGenerateNodeTestPackageConfig<'_> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.extend([
            "tool".to_string(),
            "generate-node-test-package-config".to_string(),
            "--output".to_string(),
            self.output.display().to_string(),
        ]);
    }
}
