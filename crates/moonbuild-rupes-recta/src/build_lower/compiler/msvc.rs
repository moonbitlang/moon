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

use moonutil::compiler_flags::{MsvcCrtPolicy, NativeToolchain, WINDOWS_MSVC_DEFAULT_LIBS};

pub(crate) fn add_environment_include_paths(
    toolchain: &NativeToolchain,
    command: &mut Vec<String>,
) {
    let Some(environment) = toolchain.msvc_environment() else {
        return;
    };
    command.extend(
        environment
            .include_paths
            .iter()
            .map(|path| format!("/I{}", path.display())),
    );
}

fn add_environment_lib_paths(toolchain: &NativeToolchain, command: &mut Vec<String>) {
    let Some(environment) = toolchain.msvc_environment() else {
        return;
    };
    command.extend(
        environment
            .lib_paths
            .iter()
            .map(|path| format!("/LIBPATH:{}", path.display())),
    );
}

pub(crate) fn compile_runtime_command(
    toolchain: &NativeToolchain,
    source: &Path,
    dest: &Path,
    moon_include_path: &str,
    crt: MsvcCrtPolicy,
) -> Vec<String> {
    let mut command = vec![
        toolchain.cc().cc_path.clone(),
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
    add_environment_include_paths(toolchain, &mut command);
    command.push(source.display().to_string());
    command
}

pub(crate) fn link_executable_command(
    toolchain: &NativeToolchain,
    sources: &[String],
    user_link_flags: &[String],
    dest: &str,
    lib_path: &str,
) -> Vec<String> {
    let mut command = vec![
        toolchain.cc().cc_path.clone(),
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
    add_environment_lib_paths(toolchain, &mut command);
    command.extend(user_link_flags.iter().cloned());
    command.extend(WINDOWS_MSVC_DEFAULT_LIBS.iter().map(|lib| lib.to_string()));
    command
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use moonutil::compiler_flags::{ARKind, CC, CCKind, MsvcEnvironment, NativeToolchain};

    use super::*;

    fn fake_msvc_toolchain() -> NativeToolchain {
        NativeToolchain::from_path_probe(CC {
            cc_kind: CCKind::Msvc,
            cc_path: "cl.exe".to_string(),
            ar_kind: ARKind::MsvcLib,
            ar_path: "lib.exe".to_string(),
            target_triple: None,
            is_env_override: false,
        })
        .with_msvc_environment(MsvcEnvironment {
            cl_exe: PathBuf::from("cl.exe"),
            env_pairs: vec![(
                std::ffi::OsString::from("PATH"),
                std::ffi::OsString::from("msvc/bin"),
            )],
            include_paths: vec![PathBuf::from("crt/include"), PathBuf::from("sdk/include")],
            lib_paths: vec![PathBuf::from("crt/lib"), PathBuf::from("sdk/lib")],
        })
    }

    #[test]
    fn runtime_compile_command_includes_msvc_environment_paths() {
        let command = compile_runtime_command(
            &fake_msvc_toolchain(),
            Path::new("runtime.c"),
            Path::new("runtime.obj"),
            "moon/include",
            MsvcCrtPolicy::StaticMt,
        );

        assert!(command.iter().any(|arg| arg == "/Icrt/include"));
        assert!(command.iter().any(|arg| arg == "/Isdk/include"));
        assert!(command.iter().any(|arg| arg == "/Imoon/include"));
        assert!(
            command
                .iter()
                .any(|arg| arg == MsvcCrtPolicy::StaticMt.compiler_flag())
        );
    }

    #[test]
    fn executable_link_command_includes_msvc_environment_paths_and_default_libs() {
        let command = link_executable_command(
            &fake_msvc_toolchain(),
            &["main.obj".to_string(), "runtime.obj".to_string()],
            &["custom.lib".to_string()],
            "main.exe",
            "moon/lib",
        );

        assert!(command.iter().any(|arg| arg == "/LIBPATH:crt/lib"));
        assert!(command.iter().any(|arg| arg == "/LIBPATH:sdk/lib"));
        assert!(command.iter().any(|arg| arg == "/LIBPATH:moon/lib"));
        assert!(command.iter().any(|arg| arg == "custom.lib"));
        assert!(command.iter().any(|arg| arg == "libcmt.lib"));
        assert!(command.iter().any(|arg| arg == "kernel32.lib"));
    }
}
