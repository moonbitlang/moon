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

use std::{borrow::Cow, path::Path};

use crate::build_lower::compiler::CmdlineAbstraction;

/// `moon tool archive`: recreate an archive with an archiver that would
/// otherwise update the existing one in place.
#[derive(Debug)]
pub(crate) struct MoonArchive<'a> {
    pub output: Cow<'a, Path>,
    /// The archiver command, starting with the archiver executable.
    pub archiver: &'a [String],
}

impl CmdlineAbstraction for MoonArchive<'_> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.extend([
            "tool".to_string(),
            "archive".to_string(),
            "--output".to_string(),
            self.output.display().to_string(),
            "--".to_string(),
        ]);
        args.extend(self.archiver.iter().cloned());
    }
}
