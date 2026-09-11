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

use super::{ARKind, ArchiverConfig, CC, CompilerPaths, moonbitrun_object, resolve_cc, tcc};

fn add_archiver_flags(cc: &CC, buf: &mut Vec<String>, dest: &str) {
    match cc.ar_kind {
        ARKind::MsvcLib => {
            buf.push("/nologo".to_string());
            buf.push(format!("/Out:{dest}"));
        }
        ARKind::AppleLibtool => {
            buf.push("-static".to_string());
            buf.push("-o".to_string());
            buf.push(dest.to_string());
        }
        ARKind::GnuAr | ARKind::LlvmAr => {
            buf.push("-r".to_string());
            buf.push("-c".to_string());
            buf.push("-s".to_string());
            buf.push(dest.to_string());
        }
        ARKind::TccAr => {
            tcc::add_archiver_flags(buf, dest);
        }
    }
}

fn add_archiver_moonbitrun(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &ArchiverConfig,
    paths: &CompilerPaths,
) {
    if let Some(object) = moonbitrun_object(
        cc,
        config.archive_moonbitrun,
        config.native_allocator,
        &paths.lib_path,
    ) {
        buf.push(object);
    }
}

pub fn make_archiver_command<S>(
    cc: CC,
    user_cc: Option<CC>,
    config: ArchiverConfig,
    src: &[S],
    dest: &str,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let resolved_cc = resolve_cc(&cc, user_cc.as_ref());
    let paths = CompilerPaths::from_moon_dirs();
    make_archiver_command_resolved(resolved_cc, config, src, dest, &paths)
}

pub fn make_archiver_command_resolved<S>(
    cc: CC,
    config: ArchiverConfig,
    src: &[S],
    dest: &str,
    paths: &CompilerPaths,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let mut buf = vec![cc.ar_path.clone()];

    add_archiver_flags(&cc, &mut buf, dest);
    add_archiver_moonbitrun(&cc, &mut buf, &config, paths);
    buf.extend(src.iter().map(|s| s.as_ref().to_string()));

    buf
}
