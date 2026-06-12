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
//! product paths are resolved by `target_layout`.

use crate::model::{NativeTarget, RunBackend};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SelectedBackend {
    Wasm { use_wat: bool },
    WasmGc { use_wat: bool },
    Js,
    C(CBackend),
    Llvm,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CBackend {
    executable: CExecutableRealization,
    runtime: CRuntimeRealization,
    c_stubs: CStubLibraryRealization,
}

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

impl SelectedBackend {
    pub(crate) fn new(
        target_backend: RunBackend,
        native_target: Option<NativeTarget>,
        use_tcc_run: bool,
        output_wat: bool,
    ) -> Self {
        debug_assert!(
            !use_tcc_run || target_backend == RunBackend::Native,
            "tcc-run is only valid for the C backend"
        );
        debug_assert!(
            native_target.is_none() || target_backend == RunBackend::Native,
            "direct native object lowering is only valid for the C backend"
        );

        match target_backend {
            RunBackend::Wasm => Self::Wasm {
                use_wat: output_wat,
            },
            RunBackend::WasmGC => Self::WasmGc {
                use_wat: output_wat,
            },
            RunBackend::Js => Self::Js,
            RunBackend::Native => Self::C(CBackend::new(native_target, use_tcc_run)),
            RunBackend::Llvm => Self::Llvm,
        }
    }

    pub(crate) fn c_stub_library_realization(self) -> CStubLibraryRealization {
        match self {
            Self::C(backend) => backend.c_stubs,
            Self::Llvm => CStubLibraryRealization::StaticArchive,
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js => {
                unreachable!("C stubs are only realized for C or LLVM backends")
            }
        }
    }

    pub(crate) fn uses_shared_runtime(self) -> bool {
        match self {
            Self::C(backend) => backend.runtime == CRuntimeRealization::SharedLibraryForTccRun,
            Self::Llvm => false,
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js => {
                unreachable!("runtime products are only realized for C or LLVM backends")
            }
        }
    }
}

impl CBackend {
    fn new(native_target: Option<NativeTarget>, use_tcc_run: bool) -> Self {
        if native_target.is_some() && use_tcc_run {
            unreachable!("direct native object lowering and tcc-run are mutually exclusive")
        }

        let direct_object = native_target.is_some();
        Self {
            executable: if use_tcc_run {
                CExecutableRealization::WriteTccRunResponseFile
            } else if direct_object {
                CExecutableRealization::LinkDirectObject
            } else {
                CExecutableRealization::CompileAndLinkGeneratedC
            },
            runtime: if use_tcc_run {
                CRuntimeRealization::SharedLibraryForTccRun
            } else {
                CRuntimeRealization::StaticObject
            },
            c_stubs: if use_tcc_run {
                CStubLibraryRealization::SharedLibraryForTccRun
            } else {
                CStubLibraryRealization::StaticArchive
            },
        }
    }

    pub(crate) fn executable_realization(self) -> CExecutableRealization {
        self.executable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_backend_carries_wat_setting() {
        let backend = SelectedBackend::new(RunBackend::Wasm, None, false, true);

        assert!(matches!(backend, SelectedBackend::Wasm { use_wat: true }));
    }

    #[test]
    fn c_tcc_run_realizes_shared_runtime_and_response_file() {
        let backend = SelectedBackend::new(RunBackend::Native, None, true, false);

        let SelectedBackend::C(c_backend) = backend else {
            panic!("native backend should select C lowering")
        };
        assert_eq!(
            c_backend.executable_realization(),
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
        let backend = SelectedBackend::new(
            RunBackend::Native,
            Some(NativeTarget::Aarch64AppleDarwin),
            false,
            false,
        );

        let SelectedBackend::C(c_backend) = backend else {
            panic!("native backend should select C lowering")
        };
        assert_eq!(
            c_backend.executable_realization(),
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
        let backend = SelectedBackend::new(RunBackend::Llvm, None, false, false);

        assert!(matches!(backend, SelectedBackend::Llvm));
        assert_eq!(
            backend.c_stub_library_realization(),
            CStubLibraryRealization::StaticArchive
        );
        assert!(!backend.uses_shared_runtime());
    }
}
