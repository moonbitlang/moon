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

use crate::model::{NativeBackendMode, NativeTarget, RunBackend};

#[derive(Clone, Debug)]
pub(crate) enum SelectedBackend {
    Wasm { use_wat: bool },
    WasmGc { use_wat: bool },
    Js,
    C(NativeBackendMode),
    Llvm,
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
        native_mode: Option<&NativeBackendMode>,
        output_wat: bool,
    ) -> Self {
        match (target_backend, native_mode) {
            (RunBackend::Wasm, None) => Self::Wasm {
                use_wat: output_wat,
            },
            (RunBackend::WasmGC, None) => Self::WasmGc {
                use_wat: output_wat,
            },
            (RunBackend::Js, None) => Self::Js,
            (RunBackend::Native, Some(native_mode)) => Self::C(native_mode.clone()),
            (RunBackend::Llvm, None) => Self::Llvm,
            (RunBackend::Native, None) => {
                panic!("native backend requires a native implementation mode")
            }
            (_, Some(_)) => panic!("native implementation mode requires the native backend"),
        }
    }

    pub(crate) fn direct_target(&self) -> Option<NativeTarget> {
        match self {
            Self::C(native_mode) => native_mode.direct_target(),
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js | Self::Llvm => None,
        }
    }

    pub(crate) fn is_tcc_run(&self) -> bool {
        matches!(self, Self::C(native_mode) if native_mode.is_tcc_run())
    }

    pub(crate) fn c_stub_library_realization(&self) -> CStubLibraryRealization {
        match self {
            Self::C(backend) => backend.c_stub_library_realization(),
            Self::Llvm => CStubLibraryRealization::StaticArchive,
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js => {
                unreachable!("C stubs are only realized for C or LLVM backends")
            }
        }
    }

    pub(crate) fn use_wat(&self) -> bool {
        match self {
            Self::Wasm { use_wat } | Self::WasmGc { use_wat } => *use_wat,
            Self::Js | Self::C(_) | Self::Llvm => false,
        }
    }

    pub(crate) fn uses_shared_runtime(&self) -> bool {
        match self {
            Self::C(backend) => {
                backend.runtime_realization() == CRuntimeRealization::SharedLibraryForTccRun
            }
            Self::Llvm => false,
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js => {
                unreachable!("runtime products are only realized for C or LLVM backends")
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
    use moonutil::compiler_flags::{ARKind, CC, CCKind};

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
        let backend = SelectedBackend::new(RunBackend::Wasm, None, true);

        assert!(matches!(backend, SelectedBackend::Wasm { use_wat: true }));
    }

    #[test]
    fn c_tcc_run_realizes_shared_runtime_and_response_file() {
        let native_mode = NativeBackendMode::TccRun(fake_tcc_run());
        let backend = SelectedBackend::new(RunBackend::Native, Some(&native_mode), false);

        let SelectedBackend::C(ref native_mode) = backend else {
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
        let native_mode = NativeBackendMode::DirectObject(DirectNativeMode::Target(
            crate::model::NativeTarget::Aarch64AppleDarwin,
        ));
        let backend = SelectedBackend::new(RunBackend::Native, Some(&native_mode), false);

        let SelectedBackend::C(ref native_mode) = backend else {
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
        let backend = SelectedBackend::new(RunBackend::Llvm, None, false);

        assert!(matches!(backend, SelectedBackend::Llvm));
        assert_eq!(
            backend.c_stub_library_realization(),
            CStubLibraryRealization::StaticArchive
        );
        assert!(!backend.uses_shared_runtime());
    }
}
