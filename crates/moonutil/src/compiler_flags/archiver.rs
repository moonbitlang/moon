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
