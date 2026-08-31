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
    use moonutil::compiler_flags::NativeAllocator;

    use crate::model::{BackendConfig, DirectNativeMode};

    use super::*;

    #[test]
    fn wasm_backend_carries_wat_setting() {
        let backend = BackendConfig::Wasm {
            use_wat: true,
            wasi_link: false,
        };

        assert!(matches!(backend, BackendConfig::Wasm { use_wat: true, .. }));
    }

    #[test]
    fn c_direct_object_realizes_linker_executable() {
        let backend = BackendConfig::Native {
            mode: NativeBackendMode::DirectObject(DirectNativeMode::Target(
                crate::model::NativeTarget::Aarch64AppleDarwin,
            )),
            allocator: NativeAllocator::Default,
        };

        let BackendConfig::Native {
            mode: ref native_mode,
            ..
        } = backend
        else {
            panic!("native backend should select C lowering")
        };
        assert_eq!(
            native_mode.executable_realization(),
            CExecutableRealization::LinkDirectObject
        );
    }

    #[test]
    fn llvm_backend_is_not_c_realization() {
        let backend = BackendConfig::Llvm {
            allocator: NativeAllocator::Default,
        };

        assert!(matches!(backend, BackendConfig::Llvm { .. }));
    }
}
