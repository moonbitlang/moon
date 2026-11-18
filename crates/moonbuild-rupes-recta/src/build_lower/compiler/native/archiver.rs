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

//! Native archive construction and selected library inputs.

use std::path::Path;

use moonutil::compiler_flags::{ARKind, CC, CompilerPaths, NativeAllocator};

use super::{BuildCommand, add_library_input};

#[derive(Clone, Default)]
pub(in crate::build_lower) struct ArchiverConfig {
    pub archive_moonbitrun: bool,
    pub native_allocator: NativeAllocator,
}

fn add_archiver_flags(cc: &CC, buf: &mut Vec<String>, dest: &str) {
    match cc.ar_kind {
        ARKind::MsvcLib => {
            buf.push("/nologo".to_string());
            buf.push(format!(
                "/Out:{}",
                dunce::simplified(Path::new(dest)).display()
            ));
        }
        ARKind::AppleLibtool => {
            buf.push("-static".to_string());
            // Conditional compilation can leave C stubs without any symbols.
            buf.push("-no_warning_for_no_symbols".to_string());
            buf.push("-o".to_string());
            buf.push(dunce::simplified(Path::new(dest)).display().to_string());
        }
        ARKind::GnuAr | ARKind::LlvmAr => {
            buf.push("-r".to_string());
            buf.push("-c".to_string());
            buf.push("-s".to_string());
            buf.push(dunce::simplified(Path::new(dest)).display().to_string());
        }
        ARKind::TccAr => {
            buf.push("-ar".to_string());
            buf.push("rcs".to_string());
            buf.push(dunce::simplified(Path::new(dest)).display().to_string());
        }
    }
}

pub(in crate::build_lower) fn make_archiver_command<S>(
    cc: CC,
    config: ArchiverConfig,
    src: &[S],
    dest: &str,
    paths: &CompilerPaths,
) -> BuildCommand
where
    S: AsRef<str>,
{
    let mut buf = vec![cc.ar_path.clone()];
    let mut inputs = Vec::new();

    add_archiver_flags(&cc, &mut buf, dest);
    if config.archive_moonbitrun
        && let Some(path) = paths.moonbitrun_object(&cc, config.native_allocator)
    {
        add_library_input(&mut buf, &mut inputs, path);
    }
    buf.extend(src.iter().map(|s| {
        dunce::simplified(Path::new(s.as_ref()))
            .display()
            .to_string()
    }));

    BuildCommand {
        commandline: buf.into(),
        extra_inputs: inputs,
    }
}
