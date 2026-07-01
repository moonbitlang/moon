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

use crate::model::{DirectNativeMode, NativeBackendMode, RunBackend, TccRunConfig};

#[derive(Clone, Debug)]
pub(crate) enum SelectedBackend {
    Wasm { use_wat: bool },
    WasmGc { use_wat: bool },
    Js,
    C(CBackend),
    Llvm,
}

#[derive(Clone, Debug)]
pub(crate) enum CBackend {
    GeneratedC,
    TccRun(TccRunConfig),
    DirectObject(DirectNativeMode),
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
        native_mode: &NativeBackendMode,
        output_wat: bool,
    ) -> Self {
        debug_assert!(
            !native_mode.is_tcc_run() || target_backend == RunBackend::Native,
            "tcc-run is only valid for the C backend"
        );
        debug_assert!(
            native_mode.direct_target().is_none() || target_backend == RunBackend::Native,
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
            RunBackend::Native => Self::C(CBackend::new(native_mode)),
            RunBackend::Llvm => Self::Llvm,
        }
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

    pub(crate) fn direct_native_mode(&self) -> Option<&DirectNativeMode> {
        match self {
            Self::C(backend) => backend.direct_native_mode(),
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js | Self::Llvm => None,
        }
    }

    pub(crate) fn is_windows_msvc_direct(&self) -> bool {
        matches!(
            self.direct_native_mode(),
            Some(DirectNativeMode::WindowsMsvc(_))
        )
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

impl CBackend {
    fn new(native_mode: &NativeBackendMode) -> Self {
        match native_mode {
            NativeBackendMode::GeneratedC => Self::GeneratedC,
            NativeBackendMode::TccRun(config) => Self::TccRun(config.clone()),
            NativeBackendMode::DirectObject(mode) => Self::DirectObject(mode.clone()),
        }
    }

    pub(crate) fn executable_realization(&self) -> CExecutableRealization {
        match self {
            Self::GeneratedC => CExecutableRealization::CompileAndLinkGeneratedC,
            Self::TccRun(_) => CExecutableRealization::WriteTccRunResponseFile,
            Self::DirectObject(_) => CExecutableRealization::LinkDirectObject,
        }
    }

    fn runtime_realization(&self) -> CRuntimeRealization {
        match self {
            Self::TccRun(_) => CRuntimeRealization::SharedLibraryForTccRun,
            Self::GeneratedC | Self::DirectObject(_) => CRuntimeRealization::StaticObject,
        }
    }

    pub(crate) fn c_stub_library_realization(&self) -> CStubLibraryRealization {
        match self {
            Self::TccRun(_) => CStubLibraryRealization::SharedLibraryForTccRun,
            Self::GeneratedC | Self::DirectObject(_) => CStubLibraryRealization::StaticArchive,
        }
    }

    pub(crate) fn tcc_run(&self) -> Option<&TccRunConfig> {
        match self {
            Self::TccRun(config) => Some(config),
            Self::GeneratedC | Self::DirectObject(_) => None,
        }
    }

    pub(crate) fn direct_native_mode(&self) -> Option<&DirectNativeMode> {
        match self {
            Self::DirectObject(mode) => Some(mode),
            Self::GeneratedC | Self::TccRun(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use moonutil::compiler_flags::{ARKind, CC, CCKind};

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
        let backend = SelectedBackend::new(RunBackend::Wasm, &NativeBackendMode::GeneratedC, true);

        assert!(matches!(backend, SelectedBackend::Wasm { use_wat: true }));
    }

    #[test]
    fn c_tcc_run_realizes_shared_runtime_and_response_file() {
        let native_mode = NativeBackendMode::TccRun(fake_tcc_run());
        let backend = SelectedBackend::new(RunBackend::Native, &native_mode, false);

        let SelectedBackend::C(ref c_backend) = backend else {
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
        let native_mode = NativeBackendMode::DirectObject(DirectNativeMode::generic(
            crate::model::NativeTarget::Aarch64AppleDarwin,
        ));
        let backend = SelectedBackend::new(RunBackend::Native, &native_mode, false);

        let SelectedBackend::C(ref c_backend) = backend else {
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
        let backend = SelectedBackend::new(RunBackend::Llvm, &NativeBackendMode::GeneratedC, false);

        assert!(matches!(backend, SelectedBackend::Llvm));
        assert_eq!(
            backend.c_stub_library_realization(),
            CStubLibraryRealization::StaticArchive
        );
        assert!(!backend.uses_shared_runtime());
    }
}
