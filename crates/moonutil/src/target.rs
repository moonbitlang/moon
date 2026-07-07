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

use std::path::PathBuf;

use anyhow::bail;
use clap::ValueEnum;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    cond_expr::{CompileCondition, OptLevel},
    constants::O_EXT,
};

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
#[repr(u8)]
pub enum OutputFormat {
    #[default]
    Wat,
    Wasm,
    Js,
    Native,
    LLVM,
}

impl OutputFormat {
    pub fn to_str(&self) -> &str {
        match self {
            OutputFormat::Wat => "wat",
            OutputFormat::Wasm => "wasm",
            OutputFormat::Js => "js",
            OutputFormat::Native => "c",
            OutputFormat::LLVM => O_EXT,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, ValueEnum)]
pub enum SurfaceTarget {
    Wasm,
    WasmGC,
    Js,
    Native,
    LLVM,
    All,
}

pub fn lower_surface_targets(st: &[SurfaceTarget]) -> Vec<TargetBackend> {
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
            SurfaceTarget::LLVM => {
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

#[rustfmt::skip]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize, Deserialize, Default, Hash)]
#[repr(u8)]
pub enum TargetBackend {
    Wasm,
    #[default]
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

pub fn backend_filter(
    files_and_con: &IndexMap<PathBuf, CompileCondition>,
    debug_flag: bool,
    target_backend: TargetBackend,
) -> Vec<std::path::PathBuf> {
    files_and_con
        .iter()
        .filter_map(|(file, cond)| {
            if cond.eval(OptLevel::from_debug_flag(debug_flag), target_backend) {
                Some(file)
            } else {
                None
            }
        })
        .cloned()
        .collect()
}

#[test]
fn test_backend_filter() {
    use expect_test::expect;

    let cond = CompileCondition {
        backend: vec![
            TargetBackend::Js,
            TargetBackend::Wasm,
            TargetBackend::WasmGC,
        ],
        optlevel: vec![OptLevel::Debug, OptLevel::Release],
    };

    let files: IndexMap<PathBuf, CompileCondition> = IndexMap::from([
        (PathBuf::from("a.mbt"), cond.clone()),
        (PathBuf::from("a_test.mbt"), cond.clone()),
        (PathBuf::from("b.mbt"), cond.clone()),
        (PathBuf::from("b_test.mbt"), cond.clone()),
        (
            PathBuf::from("x.js.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::Js],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x_test.js.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::Js],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x.wasm.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::Wasm],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x_test.wasm.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::Wasm],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x.wasm-gc.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::WasmGC],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x_test.wasm-gc.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::WasmGC],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
    ]);

    expect![[r#"
        [
            "a.mbt",
            "a_test.mbt",
            "b.mbt",
            "b_test.mbt",
            "x.js.mbt",
            "x_test.js.mbt",
        ]
    "#]]
    .assert_debug_eq(&backend_filter(&files, false, TargetBackend::Js));
    expect![[r#"
        [
            "a.mbt",
            "a_test.mbt",
            "b.mbt",
            "b_test.mbt",
            "x.wasm.mbt",
            "x_test.wasm.mbt",
        ]
    "#]]
    .assert_debug_eq(&backend_filter(&files, false, TargetBackend::Wasm));
    expect![[r#"
        [
            "a.mbt",
            "a_test.mbt",
            "b.mbt",
            "b_test.mbt",
            "x.wasm-gc.mbt",
            "x_test.wasm-gc.mbt",
        ]
    "#]]
    .assert_debug_eq(&backend_filter(&files, false, TargetBackend::WasmGC));
}
