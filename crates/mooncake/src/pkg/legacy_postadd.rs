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

//! Compatibility warning for the retired `scripts.postadd` hook.

use moonutil::{manifest::MoonMod, user_log::UserLog};

/// Accept legacy declarations without executing package-controlled hooks.
// TODO: Remove this compatibility warning when `scripts.postadd` is rejected.
pub fn warn(module: &MoonMod, user_log: &UserLog) {
    if module
        .scripts
        .as_ref()
        .is_some_and(|scripts| scripts.contains_key("postadd"))
    {
        user_log.warn(format!(
            "`scripts.postadd` in module `{}` is deprecated and ignored. Please open an issue at https://github.com/moonbitlang/moon/issues if you need toolchain setup hooks.",
            module.name
        ));
    }
}
