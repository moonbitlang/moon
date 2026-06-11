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

//! Backend-specific product realization.
//!
//! Build planning deals in logical actions and artifacts. Lowering selects one
//! backend branch, lets that branch decide concrete product paths, then hands
//! those paths to command lowering and n2 graph construction.

use std::path::PathBuf;

use moonutil::common::TargetBackend;

use crate::{
    build_lower::artifact::{ExecutableArtifact, LegacyLayout, LinkedCoreArtifact},
    discover::DiscoverResult,
    model::{BuildTarget, NativeTarget, OperatingSystem, PackageId, RunBackend},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SelectedBackend {
    Wasm(WasmBackend),
    WasmGc(WasmGcBackend),
    Js(JsBackend),
    C(CBackend),
    Llvm(LlvmBackend),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WasmBackend {
    use_wat: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WasmGcBackend {
    use_wat: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct JsBackend;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CBackend {
    os: OperatingSystem,
    linked_core: CLinkedCoreRealization,
    executable: CExecutableRealization,
    runtime: CRuntimeRealization,
    c_stubs: CStubLibraryRealization,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LlvmBackend {
    os: OperatingSystem,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CLinkedCoreRealization {
    GeneratedC,
    DirectObject,
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
        os: impl FnOnce() -> OperatingSystem,
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
            RunBackend::Wasm => Self::Wasm(WasmBackend {
                use_wat: output_wat,
            }),
            RunBackend::WasmGC => Self::WasmGc(WasmGcBackend {
                use_wat: output_wat,
            }),
            RunBackend::Js => Self::Js(JsBackend),
            RunBackend::Native => Self::C(CBackend::new(os(), native_target, use_tcc_run)),
            RunBackend::Llvm => Self::Llvm(LlvmBackend { os: os() }),
        }
    }

    pub(crate) fn linked_core_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        match self {
            Self::Wasm(backend) => backend.linked_core_path(layout, packages, target),
            Self::WasmGc(backend) => backend.linked_core_path(layout, packages, target),
            Self::Js(backend) => backend.linked_core_path(layout, packages, target),
            Self::C(backend) => backend.linked_core_path(layout, packages, target),
            Self::Llvm(backend) => backend.linked_core_path(layout, packages, target),
        }
    }

    pub(crate) fn executable_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        match self {
            Self::Wasm(backend) => backend.executable_path(layout, packages, target),
            Self::WasmGc(backend) => backend.executable_path(layout, packages, target),
            Self::Js(backend) => backend.executable_path(layout, packages, target),
            Self::C(backend) => backend.executable_path(layout, packages, target),
            Self::Llvm(backend) => backend.executable_path(layout, packages, target),
        }
    }

    pub(crate) fn runtime_path(self, layout: &LegacyLayout) -> PathBuf {
        match self {
            Self::C(backend) => backend.runtime_path(layout),
            Self::Llvm(backend) => backend.runtime_path(layout),
            Self::Wasm(_) | Self::WasmGc(_) | Self::Js(_) => {
                unreachable!("runtime products are only realized for C or LLVM backends")
            }
        }
    }

    pub(crate) fn c_stub_library_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        package: PackageId,
        target_backend: TargetBackend,
    ) -> PathBuf {
        match self {
            Self::C(backend) => {
                backend.c_stub_library_path(layout, packages, package, target_backend)
            }
            Self::Llvm(backend) => {
                backend.c_stub_library_path(layout, packages, package, target_backend)
            }
            Self::Wasm(_) | Self::WasmGc(_) | Self::Js(_) => {
                unreachable!("C stubs are only realized for C or LLVM backends")
            }
        }
    }

    pub(crate) fn c_stub_library_realization(self) -> CStubLibraryRealization {
        match self {
            Self::C(backend) => backend.c_stub_library_realization(),
            Self::Llvm(_) => CStubLibraryRealization::StaticArchive,
            Self::Wasm(_) | Self::WasmGc(_) | Self::Js(_) => {
                unreachable!("C stubs are only realized for C or LLVM backends")
            }
        }
    }

    pub(crate) fn uses_shared_runtime(self) -> bool {
        match self {
            Self::C(backend) => backend.uses_shared_runtime(),
            Self::Llvm(_) => false,
            Self::Wasm(_) | Self::WasmGc(_) | Self::Js(_) => {
                unreachable!("runtime products are only realized for C or LLVM backends")
            }
        }
    }
}

impl WasmBackend {
    fn linked_core_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        layout.linked_core_of_build_target(
            packages,
            target,
            LinkedCoreArtifact::Wasm {
                use_wat: self.use_wat,
            },
        )
    }

    fn executable_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        layout.executable_of_build_target(
            packages,
            target,
            ExecutableArtifact::Wasm {
                use_wat: self.use_wat,
            },
        )
    }
}

impl WasmGcBackend {
    fn linked_core_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        layout.linked_core_of_build_target(
            packages,
            target,
            LinkedCoreArtifact::WasmGC {
                use_wat: self.use_wat,
            },
        )
    }

    fn executable_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        layout.executable_of_build_target(
            packages,
            target,
            ExecutableArtifact::WasmGC {
                use_wat: self.use_wat,
            },
        )
    }
}

impl JsBackend {
    fn linked_core_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        layout.linked_core_of_build_target(packages, target, LinkedCoreArtifact::Js)
    }

    fn executable_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        layout.executable_of_build_target(packages, target, ExecutableArtifact::Js)
    }
}

impl CBackend {
    fn new(os: OperatingSystem, native_target: Option<NativeTarget>, use_tcc_run: bool) -> Self {
        if native_target.is_some() && use_tcc_run {
            unreachable!("direct native object lowering and tcc-run are mutually exclusive")
        }

        let direct_object = native_target.is_some();
        Self {
            os,
            linked_core: if direct_object {
                CLinkedCoreRealization::DirectObject
            } else {
                CLinkedCoreRealization::GeneratedC
            },
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

    fn linked_core_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        let linked_core = match self.linked_core {
            CLinkedCoreRealization::GeneratedC => LinkedCoreArtifact::NativeC,
            CLinkedCoreRealization::DirectObject => {
                LinkedCoreArtifact::NativeObject { os: self.os }
            }
        };
        layout.linked_core_of_build_target(packages, target, linked_core)
    }

    fn executable_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        let executable = match self.executable {
            CExecutableRealization::CompileAndLinkGeneratedC
            | CExecutableRealization::LinkDirectObject => ExecutableArtifact::NativeExecutable,
            CExecutableRealization::WriteTccRunResponseFile => {
                ExecutableArtifact::TccRunResponseFile
            }
        };
        layout.executable_of_build_target(packages, target, executable)
    }

    pub(crate) fn executable_realization(self) -> CExecutableRealization {
        self.executable
    }

    fn runtime_path(self, layout: &LegacyLayout) -> PathBuf {
        layout.runtime_output_path(
            RunBackend::Native,
            self.runtime == CRuntimeRealization::SharedLibraryForTccRun,
            self.os,
        )
    }

    fn c_stub_library_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        package: PackageId,
        target_backend: TargetBackend,
    ) -> PathBuf {
        match self.c_stubs {
            CStubLibraryRealization::StaticArchive => {
                layout.c_stub_archive_path(packages, package, target_backend, self.os)
            }
            CStubLibraryRealization::SharedLibraryForTccRun => {
                layout.c_stub_link_dylib_path(packages, package, target_backend, self.os)
            }
        }
    }

    fn c_stub_library_realization(self) -> CStubLibraryRealization {
        self.c_stubs
    }

    pub(crate) fn uses_shared_runtime(self) -> bool {
        self.runtime == CRuntimeRealization::SharedLibraryForTccRun
    }
}

impl LlvmBackend {
    fn linked_core_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        layout.linked_core_of_build_target(
            packages,
            target,
            LinkedCoreArtifact::LlvmObject { os: self.os },
        )
    }

    fn executable_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        target: &BuildTarget,
    ) -> PathBuf {
        layout.executable_of_build_target(packages, target, ExecutableArtifact::LlvmExecutable)
    }

    fn runtime_path(self, layout: &LegacyLayout) -> PathBuf {
        layout.runtime_output_path(RunBackend::Llvm, false, self.os)
    }

    fn c_stub_library_path(
        self,
        layout: &LegacyLayout,
        packages: &DiscoverResult,
        package: PackageId,
        target_backend: TargetBackend,
    ) -> PathBuf {
        layout.c_stub_archive_path(packages, package, target_backend, self.os)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_native_backends_do_not_resolve_operating_system() {
        let backend = SelectedBackend::new(RunBackend::Wasm, None, false, true, || {
            panic!("non-native backend should not resolve the host OS")
        });

        assert!(matches!(
            backend,
            SelectedBackend::Wasm(WasmBackend { use_wat: true })
        ));
    }

    #[test]
    fn c_tcc_run_realizes_shared_runtime_and_response_file() {
        let backend = SelectedBackend::new(RunBackend::Native, None, true, false, || {
            OperatingSystem::Linux
        });

        let SelectedBackend::C(c_backend) = backend else {
            panic!("native backend should select C lowering")
        };
        assert_eq!(
            c_backend.executable_realization(),
            CExecutableRealization::WriteTccRunResponseFile
        );
        assert_eq!(
            c_backend.c_stub_library_realization(),
            CStubLibraryRealization::SharedLibraryForTccRun
        );
        assert!(c_backend.uses_shared_runtime());
    }

    #[test]
    fn c_direct_object_realizes_linker_executable() {
        let backend = SelectedBackend::new(
            RunBackend::Native,
            Some(NativeTarget::Aarch64AppleDarwin),
            false,
            false,
            || OperatingSystem::MacOS,
        );

        let SelectedBackend::C(c_backend) = backend else {
            panic!("native backend should select C lowering")
        };
        assert_eq!(
            c_backend.executable_realization(),
            CExecutableRealization::LinkDirectObject
        );
        assert_eq!(
            c_backend.c_stub_library_realization(),
            CStubLibraryRealization::StaticArchive
        );
        assert!(!c_backend.uses_shared_runtime());
    }

    #[test]
    fn llvm_backend_is_not_c_realization() {
        let backend = SelectedBackend::new(RunBackend::Llvm, None, false, false, || {
            OperatingSystem::Windows
        });

        assert!(matches!(backend, SelectedBackend::Llvm(_)));
        assert_eq!(
            backend.c_stub_library_realization(),
            CStubLibraryRealization::StaticArchive
        );
        assert!(!backend.uses_shared_runtime());
    }
}
