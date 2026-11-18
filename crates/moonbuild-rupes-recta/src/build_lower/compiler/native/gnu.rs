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

//! GNU-style compiler-driver arguments for GCC, Clang, and system cc.
//!
//! Driver syntax is independent of the target ABI: plain Clang belongs here
//! even when targeting MSVC, while clang-cl uses the MSVC command module.

use std::path::Path;

use moonutil::compiler_flags::{CC, CCKind};

use super::{CCConfig, LinkerConfig, OutputType};

pub(super) fn add_cc_specific_flags(cc: &CC, buf: &mut Vec<String>) {
    // The flags are required by the generated C runtime. See:
    // https://github.com/moonbitlang/core/issues/1594#issuecomment-2649652455
    if cc.is_full_featured_gcc_like() {
        buf.push("-fwrapv".to_string());
        buf.push("-fno-strict-aliasing".to_string());
        // Fusing operations changes rounding and can disagree with other backends.
        buf.push("-ffp-contract=off".to_string());
        // Apple clang is usually detected as SystemCC on macOS.
        if matches!(cc.cc_kind, CCKind::Clang)
            || (cfg!(target_os = "macos") && matches!(cc.cc_kind, CCKind::SystemCC))
        {
            buf.push("-Wno-unused-value".to_string());
        }
    }
}

pub(super) fn add_linker_common_libraries<P: AsRef<Path>>(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<P>,
) {
    if cc.is_gcc_like() {
        if cc.should_link_libm() {
            buf.push("-lm".to_string());
        }
        if let Some(dyn_lib_path) = config.link_shared_runtime.as_ref() {
            buf.push("-lruntime".to_string());
            buf.push(format!("-Wl,-rpath,{}", dyn_lib_path.as_ref().display()));
        }
    }
}

pub(super) fn add_cc_common_libraries(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if cc.should_link_libm() && config.output_ty != OutputType::Object {
        buf.push("-lm".to_string());
    }
}
