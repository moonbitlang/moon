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

//! Target backend pipeline decisions used by backend lowering.
//!
//! This module keeps the user-visible target backend separate from native
//! pipeline details. In particular, `tcc -run` still consumes generated C from
//! link-core; it only changes how the final runnable artifact is realized.

use crate::model::{NativeTarget, OperatingSystem, RunBackend};

use super::artifact::{ExecutableArtifact, LinkedCoreArtifact};

#[derive(Clone, Copy, Debug)]
pub(crate) enum BackendPipeline {
    Wasm { use_wat: bool },
    WasmGC { use_wat: bool },
    Js,
    Native(NativePipeline),
}

impl BackendPipeline {
    pub(crate) fn from_config(
        target_backend: RunBackend,
        native_target: Option<NativeTarget>,
        use_tcc_run: bool,
        output_wat: bool,
        os: impl FnOnce() -> OperatingSystem,
    ) -> Self {
        debug_assert!(!use_tcc_run || target_backend == RunBackend::Native);
        debug_assert!(!use_tcc_run || native_target.is_none());
        debug_assert!(target_backend == RunBackend::Native || native_target.is_none());

        match target_backend {
            RunBackend::Wasm => Self::Wasm {
                use_wat: output_wat,
            },
            RunBackend::WasmGC => Self::WasmGC {
                use_wat: output_wat,
            },
            RunBackend::Js => Self::Js,
            RunBackend::Native if use_tcc_run => Self::Native(NativePipeline::tcc_run(os())),
            RunBackend::Native if native_target.is_some() => {
                Self::Native(NativePipeline::direct_object(os()))
            }
            RunBackend::Native => Self::Native(NativePipeline::generated_c_executable(os())),
            RunBackend::Llvm => Self::Native(NativePipeline::llvm_object(os())),
        }
    }

    pub(crate) fn uses_tcc_run(self) -> bool {
        matches!(self, Self::Native(native) if native.uses_tcc_run())
    }

    pub(crate) fn native_executable_realization(self) -> NativeExecutableRealization {
        match self {
            Self::Native(native) => native.executable,
            Self::Wasm { .. } | Self::WasmGC { .. } | Self::Js => {
                unreachable!(
                    "native executable realization is not defined for non-native pipelines"
                )
            }
        }
    }

    pub(crate) fn executable_artifact(self) -> ExecutableArtifact {
        match self {
            Self::Wasm { use_wat } => ExecutableArtifact::Wasm { use_wat },
            Self::WasmGC { use_wat } => ExecutableArtifact::WasmGC { use_wat },
            Self::Js => ExecutableArtifact::Js,
            Self::Native(native) => native.executable_artifact(),
        }
    }

    pub(crate) fn linked_core_artifact(self) -> LinkedCoreArtifact {
        match self {
            Self::Wasm { use_wat } => LinkedCoreArtifact::Wasm { use_wat },
            Self::WasmGC { use_wat } => LinkedCoreArtifact::WasmGC { use_wat },
            Self::Js => LinkedCoreArtifact::Js,
            Self::Native(native) => native.linked_core_artifact(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NativePipeline {
    os: OperatingSystem,
    linked_core: NativeLinkedCore,
    executable: NativeExecutableRealization,
}

impl NativePipeline {
    fn generated_c_executable(os: OperatingSystem) -> Self {
        Self {
            os,
            linked_core: NativeLinkedCore::GeneratedC,
            executable: NativeExecutableRealization::CompileAndLinkGeneratedC,
        }
    }

    fn tcc_run(os: OperatingSystem) -> Self {
        Self {
            os,
            linked_core: NativeLinkedCore::GeneratedC,
            executable: NativeExecutableRealization::WriteTccRunResponseFile,
        }
    }

    fn direct_object(os: OperatingSystem) -> Self {
        Self {
            os,
            linked_core: NativeLinkedCore::DirectObject,
            executable: NativeExecutableRealization::LinkNativeObject,
        }
    }

    fn llvm_object(os: OperatingSystem) -> Self {
        Self {
            os,
            linked_core: NativeLinkedCore::LlvmObject,
            executable: NativeExecutableRealization::LinkLlvmObject,
        }
    }

    fn uses_tcc_run(self) -> bool {
        matches!(
            self.executable,
            NativeExecutableRealization::WriteTccRunResponseFile
        )
    }

    fn linked_core_artifact(self) -> LinkedCoreArtifact {
        match self.linked_core {
            NativeLinkedCore::GeneratedC => LinkedCoreArtifact::NativeC,
            NativeLinkedCore::DirectObject => LinkedCoreArtifact::NativeObject { os: self.os },
            NativeLinkedCore::LlvmObject => LinkedCoreArtifact::LlvmObject { os: self.os },
        }
    }

    fn executable_artifact(self) -> ExecutableArtifact {
        match self.executable {
            NativeExecutableRealization::CompileAndLinkGeneratedC
            | NativeExecutableRealization::LinkNativeObject => ExecutableArtifact::NativeExecutable,
            NativeExecutableRealization::WriteTccRunResponseFile => {
                ExecutableArtifact::TccRunResponseFile
            }
            NativeExecutableRealization::LinkLlvmObject => ExecutableArtifact::LlvmExecutable,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum NativeLinkedCore {
    GeneratedC,
    DirectObject,
    LlvmObject,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum NativeExecutableRealization {
    CompileAndLinkGeneratedC,
    LinkNativeObject,
    WriteTccRunResponseFile,
    LinkLlvmObject,
}

#[cfg(test)]
mod tests {
    use crate::model::{NativeTarget, OperatingSystem, RunBackend};

    use super::*;

    #[test]
    fn tcc_run_keeps_link_core_as_generated_c() {
        let pipeline = BackendPipeline::from_config(RunBackend::Native, None, true, false, || {
            OperatingSystem::Linux
        });

        assert!(pipeline.uses_tcc_run());
        assert!(matches!(
            pipeline.linked_core_artifact(),
            LinkedCoreArtifact::NativeC
        ));
        assert!(matches!(
            pipeline.executable_artifact(),
            ExecutableArtifact::TccRunResponseFile
        ));
    }

    #[test]
    fn direct_native_uses_object_link_core_and_native_executable() {
        let pipeline = BackendPipeline::from_config(
            RunBackend::Native,
            Some(NativeTarget::Aarch64AppleDarwin),
            false,
            false,
            || OperatingSystem::MacOS,
        );

        assert!(!pipeline.uses_tcc_run());
        assert!(matches!(
            pipeline.linked_core_artifact(),
            LinkedCoreArtifact::NativeObject {
                os: OperatingSystem::MacOS
            }
        ));
        assert!(matches!(
            pipeline.executable_artifact(),
            ExecutableArtifact::NativeExecutable
        ));
    }

    #[test]
    fn llvm_uses_llvm_object_and_llvm_executable() {
        let pipeline = BackendPipeline::from_config(RunBackend::Llvm, None, false, false, || {
            OperatingSystem::Windows
        });

        assert!(matches!(
            pipeline.linked_core_artifact(),
            LinkedCoreArtifact::LlvmObject {
                os: OperatingSystem::Windows
            }
        ));
        assert!(matches!(
            pipeline.executable_artifact(),
            ExecutableArtifact::LlvmExecutable
        ));
    }

    #[test]
    fn non_native_backends_preserve_linked_core_and_executable_shape() {
        let wasm = BackendPipeline::from_config(RunBackend::Wasm, None, false, true, || {
            OperatingSystem::None
        });
        let wasm_gc = BackendPipeline::from_config(RunBackend::WasmGC, None, false, false, || {
            OperatingSystem::None
        });
        let js = BackendPipeline::from_config(RunBackend::Js, None, false, false, || {
            OperatingSystem::None
        });

        assert!(matches!(
            wasm.linked_core_artifact(),
            LinkedCoreArtifact::Wasm { use_wat: true }
        ));
        assert!(matches!(
            wasm.executable_artifact(),
            ExecutableArtifact::Wasm { use_wat: true }
        ));
        assert!(matches!(
            wasm_gc.linked_core_artifact(),
            LinkedCoreArtifact::WasmGC { use_wat: false }
        ));
        assert!(matches!(
            wasm_gc.executable_artifact(),
            ExecutableArtifact::WasmGC { use_wat: false }
        ));
        assert!(matches!(js.linked_core_artifact(), LinkedCoreArtifact::Js));
        assert!(matches!(js.executable_artifact(), ExecutableArtifact::Js));
    }

    #[test]
    fn non_native_backends_do_not_need_operating_system() {
        let pipeline = BackendPipeline::from_config(RunBackend::Js, None, false, false, || {
            panic!("non-native pipeline should not resolve operating system")
        });

        assert!(matches!(
            pipeline.linked_core_artifact(),
            LinkedCoreArtifact::Js
        ));
    }
}
