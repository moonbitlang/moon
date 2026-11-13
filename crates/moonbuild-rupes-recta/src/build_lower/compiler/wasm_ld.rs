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

//! Abstraction for `moonc wasm-ld`.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::build_lower::compiler::CmdlineAbstraction;

/// Abstraction for `moonc wasm-ld`, which links relocatable wasm1 objects
/// written by `moonc link-core` into a module.
///
/// The linker generates the runtime itself. The runtime depends on settings
/// the objects were compiled with, which this command repeats; `moonc` does
/// not check that they match.
#[derive(Debug)]
pub(crate) struct MooncWasmLd<'a> {
    /// The objects to link. The linker initializes library packages in this
    /// order, so it must be their dependency order.
    pub objects: &'a [PathBuf],
    /// The linked `.wasm` file.
    pub output_path: Cow<'a, Path>,
    /// Keep function names (the name section), as `link-core -g` does.
    pub symbols: bool,
    /// The objects were compiled to WASI.
    pub wasi: bool,
    /// The objects were compiled with the MoonBit TLSF allocator.
    pub new_allocator: bool,
    /// The objects were compiled with trial-deletion cycle collection.
    pub collect_ref_cycle: bool,
}

impl CmdlineAbstraction for MooncWasmLd<'_> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.push("wasm-ld".into());

        for object in self.objects {
            args.push(dunce::simplified(object).display().to_string());
        }

        args.push("-o".to_string());
        args.push(dunce::simplified(&self.output_path).display().to_string());

        if self.symbols {
            args.push("-g".to_string());
        }
        if self.wasi {
            args.push("-wasi".to_string());
        }
        if self.new_allocator {
            args.push("-allocator".to_string());
            args.push("tlsf-mbt".to_string());
        }
        if self.collect_ref_cycle {
            args.push("-enable-trial-deletion".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_precede_the_output_and_runtime_settings() {
        let objects = [PathBuf::from("core.o"), PathBuf::from("main.o")];
        let cmd = MooncWasmLd {
            objects: &objects,
            output_path: Path::new("main.wasm").into(),
            symbols: true,
            wasi: true,
            new_allocator: false,
            collect_ref_cycle: true,
        };
        expect_test::expect![[r#"
            [
                "moonc",
                "wasm-ld",
                "core.o",
                "main.o",
                "-o",
                "main.wasm",
                "-g",
                "-wasi",
                "-enable-trial-deletion",
            ]
        "#]]
        .assert_debug_eq(&cmd.build_command("moonc"));
    }
}
