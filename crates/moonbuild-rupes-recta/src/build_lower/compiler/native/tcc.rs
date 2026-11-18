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

//! TCC-specific compiler flags.

use colored::Colorize;
use moonutil::compiler_flags::CC;

use super::CCConfig;

pub(super) fn add_cc_specific_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if !cc.is_tcc() {
        return;
    }

    if config.no_sys_header {
        buf.push("-DMOONBIT_NATIVE_NO_SYS_HEADER".to_string());
    } else {
        eprintln!(
            "{}: Use tcc without set MOONBIT_NATIVE_NO_SYS_HEADER.",
            "Warning".yellow().bold(),
        );
    }
}
