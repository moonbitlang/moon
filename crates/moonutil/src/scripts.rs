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

pub enum PrePostBuild {
    PreBuild,
}

impl PrePostBuild {
    pub fn name(&self) -> String {
        match self {
            PrePostBuild::PreBuild => "pre-build".into(),
        }
    }

    pub fn dbname(&self) -> String {
        format!("{}.db", self.name())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IgnoredMoonScript {
    Prebuild,
}

impl IgnoredMoonScript {
    pub fn env_var(self) -> &'static str {
        match self {
            IgnoredMoonScript::Prebuild => "MOON_IGNORE_PREBUILD",
        }
    }
}

pub fn is_moon_script_ignored(script: IgnoredMoonScript) -> bool {
    std::env::var_os(script.env_var()).is_some()
}
