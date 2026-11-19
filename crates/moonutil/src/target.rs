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

use anyhow::bail;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::constants::O_EXT;

#[rustfmt::skip]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize, Deserialize, Default, Hash)]
#[repr(u8)]
pub enum TargetBackend {
    #[default]
    Wasm,
    WasmGC,
    Js,
    Native,
    LLVM
}

impl std::fmt::Display for TargetBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_flag())
    }
}

impl TargetBackend {
    /// Whether this backend includes a file guarded by the given target.
    /// Native file conditions also apply to LLVM, matching the compiler's
    /// `#cfg(target="native")` behavior. LLVM conditions remain LLVM-only.
    pub fn matches_file_target(self, file_target: Self) -> bool {
        self == file_target || matches!((self, file_target), (Self::LLVM, Self::Native))
    }

    pub fn to_flag(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
            Self::Native => "native",
            Self::LLVM => "llvm",
        }
    }

    pub fn to_extension(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm",
            Self::Js => "js",
            Self::Native => "exe",
            Self::LLVM => "exe",
        }
    }

    pub fn to_artifact(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm",
            Self::Js => "js",
            Self::Native => "c",
            Self::LLVM => O_EXT,
        }
    }

    pub fn to_dir_name(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
            Self::Native => "native",
            Self::LLVM => "llvm",
        }
    }

    pub fn to_backend_ext(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
            Self::Native => "native",
            Self::LLVM => "llvm",
        }
    }

    pub fn str_to_backend(s: &str) -> anyhow::Result<Self> {
        match s {
            "wasm" => Ok(Self::Wasm),
            "wasm-gc" => Ok(Self::WasmGC),
            "js" => Ok(Self::Js),
            "native" => Ok(Self::Native),
            "llvm" => Ok(Self::LLVM),
            _ => bail!(
                "invalid backend: {}, only support wasm, wasm-gc, js, native, llvm",
                s
            ),
        }
    }

    pub fn indexset_to_string(backends: &indexmap::IndexSet<TargetBackend>) -> String {
        let mut backends = backends
            .iter()
            .map(|b| b.to_flag().to_string())
            .collect::<Vec<_>>();
        backends.sort();
        format!("[{}]", backends.join(", "))
    }

    pub fn is_native(self) -> bool {
        match self {
            Self::Native | Self::LLVM => true,
            Self::Wasm | Self::WasmGC | Self::Js => false,
        }
    }

    pub fn is_wasm(self) -> bool {
        match self {
            Self::Wasm | Self::WasmGC => true,
            Self::Js | Self::Native | Self::LLVM => false,
        }
    }

    pub fn allowed_as_project_target(self) -> bool {
        match self {
            Self::Wasm | Self::WasmGC | Self::Js | Self::Native => true,
            Self::LLVM => false,
        }
    }

    pub fn supports_source_map(self) -> bool {
        match self {
            Self::WasmGC | Self::Js => true,
            Self::Wasm | Self::Native | Self::LLVM => false,
        }
    }

    pub fn all() -> &'static [Self] {
        Self::value_variants()
    }
}
