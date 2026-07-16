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

//! MSVC-specific command helpers for the direct native backend.

use std::path::Path;

use moonutil::compiler_flags::{MsvcCrtPolicy, Toolchain, WINDOWS_MSVC_DEFAULT_LIBS};

pub(crate) fn command_env(toolchain: &Toolchain) -> Vec<(String, String)> {
    toolchain
        .msvc_environment()
        .map(|environment| environment.command_env().to_vec())
        .unwrap_or_default()
}

pub(crate) fn compile_runtime_command(
    toolchain: &Toolchain,
    source: &Path,
    dest: &Path,
    moon_include_path: &str,
    crt: MsvcCrtPolicy,
) -> Vec<String> {
    let mut command = vec![
        toolchain.cc_command_path(),
        "/nologo".to_string(),
        "/utf-8".to_string(),
        "/wd4819".to_string(),
        "/c".to_string(),
        "/Z7".to_string(),
        "/O2".to_string(),
        crt.compiler_flag().to_string(),
        format!("/Fo{}", dest.display()),
        format!("/I{moon_include_path}"),
    ];
    command.push(source.display().to_string());
    command
}

pub(crate) fn link_executable_command(
    toolchain: &Toolchain,
    sources: &[String],
    user_link_flags: &[String],
    dest: &str,
    lib_path: &str,
) -> Vec<String> {
    let mut command = vec![
        toolchain.cc_command_path(),
        "/nologo".to_string(),
        format!("/Fe{dest}"),
    ];
    command.extend(sources.iter().cloned());
    command.push("/link".to_string());
    command.extend([
        "/nologo".to_string(),
        "/subsystem:console".to_string(),
        format!("/LIBPATH:{lib_path}"),
    ]);
    command.extend(user_link_flags.iter().cloned());
    command.extend(WINDOWS_MSVC_DEFAULT_LIBS.iter().map(|lib| lib.to_string()));
    command
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use moonutil::compiler_flags::{ARKind, CC, CCKind, MsvcEnvironment, Toolchain};

    use super::*;

    fn fake_msvc_toolchain() -> Toolchain {
        Toolchain::from_path_probe(CC {
            cc_kind: CCKind::Msvc,
            cc_path: "cl.exe".to_string(),
            ar_kind: ARKind::MsvcLib,
            ar_path: "lib.exe".to_string(),
            target_triple: None,
            is_env_override: false,
        })
        .with_msvc_environment(MsvcEnvironment {
            command_env: vec![
                ("INCLUDE".to_string(), "crt/include;sdk/include".to_string()),
                ("LIB".to_string(), "crt/lib;sdk/lib".to_string()),
            ],
        })
    }

    #[test]
    fn runtime_compile_command_uses_command_env_for_msvc_environment() {
        let command = compile_runtime_command(
            &fake_msvc_toolchain(),
            Path::new("runtime.c"),
            Path::new("runtime.obj"),
            "moon/include",
            MsvcCrtPolicy::StaticMt,
        );

        assert!(command.iter().any(|arg| arg == "/Imoon/include"));
        assert!(
            command
                .iter()
                .any(|arg| arg == MsvcCrtPolicy::StaticMt.compiler_flag())
        );
    }

    #[test]
    fn executable_link_command_uses_command_env_for_msvc_environment() {
        let command = link_executable_command(
            &fake_msvc_toolchain(),
            &["main.obj".to_string(), "runtime.obj".to_string()],
            &["custom.lib".to_string()],
            "main.exe",
            "moon/lib",
        );

        assert!(command.iter().any(|arg| arg == "/LIBPATH:moon/lib"));
        assert!(command.iter().any(|arg| arg == "custom.lib"));
        assert!(command.iter().any(|arg| arg == "libcmt.lib"));
        assert!(command.iter().any(|arg| arg == "kernel32.lib"));
    }
}
