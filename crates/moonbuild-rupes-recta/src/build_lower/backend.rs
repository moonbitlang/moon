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
