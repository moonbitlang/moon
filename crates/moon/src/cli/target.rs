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

//! Command-line target selections and their expansion into concrete backends.

use clap::ValueEnum;
use moonutil::target::TargetBackend;

#[derive(Debug, Copy, Clone, PartialEq, ValueEnum)]
pub(super) enum SurfaceTarget {
    Wasm,
    WasmGC,
    Js,
    Native,
    Llvm,
    // LLVM remains an explicit opt-in rather than part of `--target all`.
    All,
}

pub(super) fn lower_surface_targets(st: &[SurfaceTarget]) -> Vec<TargetBackend> {
    let mut result = std::collections::HashSet::new();
    for item in st {
        match item {
            SurfaceTarget::Wasm => {
                result.insert(TargetBackend::Wasm);
            }
            SurfaceTarget::WasmGC => {
                result.insert(TargetBackend::WasmGC);
            }
            SurfaceTarget::Js => {
                result.insert(TargetBackend::Js);
            }
            SurfaceTarget::Native => {
                result.insert(TargetBackend::Native);
            }
            SurfaceTarget::Llvm => {
                result.insert(TargetBackend::LLVM);
            }
            SurfaceTarget::All => {
                result.insert(TargetBackend::Wasm);
                result.insert(TargetBackend::WasmGC);
                result.insert(TargetBackend::Js);
                result.insert(TargetBackend::Native);
            }
        }
    }
    let mut result: Vec<TargetBackend> = result.into_iter().collect();
    result.sort();
    result
}
