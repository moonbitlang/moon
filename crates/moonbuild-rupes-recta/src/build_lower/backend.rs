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

//! Backend-specific lowering realization.
//!
//! Build planning deals in logical actions and artifacts. Lowering selects one
//! backend branch for command shape and runtime/linking behavior. Concrete
//! artifact paths are resolved by `target_layout`.

use crate::model::NativeBackendMode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CExecutableRealization {
    CompileAndLinkGeneratedC,
    LinkDirectObject,
}

impl NativeBackendMode {
    pub(crate) fn executable_realization(&self) -> CExecutableRealization {
        match self {
            NativeBackendMode::GeneratedC => CExecutableRealization::CompileAndLinkGeneratedC,
            NativeBackendMode::DirectObject(_) => CExecutableRealization::LinkDirectObject,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::DirectNativeMode;

    use super::*;

    #[test]
    fn c_direct_object_realizes_linker_executable() {
        let native_mode = NativeBackendMode::DirectObject(DirectNativeMode::Target(
            crate::model::NativeTarget::Aarch64AppleDarwin,
        ));
        assert_eq!(
            native_mode.executable_realization(),
            CExecutableRealization::LinkDirectObject
        );
    }
}
