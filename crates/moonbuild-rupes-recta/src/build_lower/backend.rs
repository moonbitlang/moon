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

use crate::model::{BackendConfig, NativeBackendMode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CExecutableRealization {
    CompileAndLinkGeneratedC,
    LinkDirectObject,
    WriteTccRunResponseFile,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CRuntimeRealization {
    StaticObject,
    SharedLibraryForTccRun,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CStubLibraryRealization {
    StaticArchive,
    SharedLibraryForTccRun,
}

impl BackendConfig {
    pub(crate) fn c_stub_library_realization(&self) -> CStubLibraryRealization {
        match self {
            Self::Native { mode, .. } => mode.c_stub_library_realization(),
            Self::Llvm { .. } => CStubLibraryRealization::StaticArchive,
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js => {
                unreachable!("C stubs are only realized for C or LLVM backends")
            }
        }
    }

    pub(crate) fn uses_shared_runtime(&self) -> bool {
        match self {
            Self::Native { mode, .. } => {
                mode.runtime_realization() == CRuntimeRealization::SharedLibraryForTccRun
            }
            Self::Llvm { .. } => false,
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js => {
                unreachable!("runtime artifacts are only realized for C or LLVM backends")
            }
        }
    }
}

impl NativeBackendMode {
    pub(crate) fn executable_realization(&self) -> CExecutableRealization {
        match self {
            NativeBackendMode::GeneratedC => CExecutableRealization::CompileAndLinkGeneratedC,
            NativeBackendMode::TccRun(_) => CExecutableRealization::WriteTccRunResponseFile,
            NativeBackendMode::DirectObject(_) => CExecutableRealization::LinkDirectObject,
        }
    }

    fn runtime_realization(&self) -> CRuntimeRealization {
        match self {
            NativeBackendMode::TccRun(_) => CRuntimeRealization::SharedLibraryForTccRun,
            NativeBackendMode::GeneratedC | NativeBackendMode::DirectObject(_) => {
                CRuntimeRealization::StaticObject
            }
        }
    }

    pub(crate) fn c_stub_library_realization(&self) -> CStubLibraryRealization {
        match self {
            NativeBackendMode::TccRun(_) => CStubLibraryRealization::SharedLibraryForTccRun,
            NativeBackendMode::GeneratedC | NativeBackendMode::DirectObject(_) => {
                CStubLibraryRealization::StaticArchive
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use moonutil::compiler_flags::{ARKind, CC, CCKind, NativeAllocator};

    use crate::model::{DirectNativeMode, TccRunConfig};

    use super::*;

    fn fake_tcc_run() -> TccRunConfig {
        TccRunConfig::new(CC {
            cc_kind: CCKind::Tcc,
            cc_path: "tcc".to_string(),
            ar_kind: ARKind::TccAr,
            ar_path: "tcc".to_string(),
            target_triple: None,
            is_env_override: false,
        })
    }

    #[test]
    fn wasm_backend_carries_wat_setting() {
        let backend = BackendConfig::Wasm {
            use_wat: true,
            wasi_link: false,
        };

        assert!(matches!(backend, BackendConfig::Wasm { use_wat: true, .. }));
    }

    #[test]
    fn c_tcc_run_realizes_shared_runtime_and_response_file() {
        let backend = BackendConfig::Native {
            mode: NativeBackendMode::TccRun(fake_tcc_run()),
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
            CExecutableRealization::WriteTccRunResponseFile
        );
        assert_eq!(
            backend.c_stub_library_realization(),
            CStubLibraryRealization::SharedLibraryForTccRun
        );
        assert!(backend.uses_shared_runtime());
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
        assert_eq!(
            backend.c_stub_library_realization(),
            CStubLibraryRealization::StaticArchive
        );
        assert!(!backend.uses_shared_runtime());
    }

    #[test]
    fn llvm_backend_is_not_c_realization() {
        let backend = BackendConfig::Llvm {
            allocator: NativeAllocator::Default,
        };

        assert!(matches!(backend, BackendConfig::Llvm { .. }));
        assert_eq!(
            backend.c_stub_library_realization(),
            CStubLibraryRealization::StaticArchive
        );
        assert!(!backend.uses_shared_runtime());
    }
}
