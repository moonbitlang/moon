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

//! MSVC command construction for generated C and direct native payloads.

use std::path::Path;

use super::{CCConfig, LinkerConfig, OutputType, render_link_arg};

use moonutil::compiler_flags::{
    CC, CCKind, MsvcCrtPolicy, NativeAllocator, Toolchain, WINDOWS_MSVC_C_STANDARD_FLAG,
    WINDOWS_MSVC_DEFAULT_LIBS, WINDOWS_MSVC_STATIC_RUNTIME_FLAG,
};

use crate::model::NativeLinkArg;

const CLANG_MSVC_STATIC_RUNTIME_FLAG: &str = "-fms-runtime-lib=static";

pub(super) fn add_cc_runtime_flags(toolchain: &Toolchain, args: &mut Vec<String>) {
    let Some(crt) = toolchain.msvc_crt_policy() else {
        return;
    };
    let flag = match (crt, toolchain.uses_msvc_cli()) {
        (MsvcCrtPolicy::StaticMt, true) => WINDOWS_MSVC_STATIC_RUNTIME_FLAG,
        (MsvcCrtPolicy::StaticMt, false) => CLANG_MSVC_STATIC_RUNTIME_FLAG,
    };
    args.push(flag.to_owned());
}

pub(in crate::build_lower) fn command_env(toolchain: &Toolchain) -> Vec<(String, String)> {
    toolchain
        .msvc_environment()
        .map(|environment| environment.command_env().to_vec())
        .unwrap_or_default()
}

pub(in crate::build_lower) fn compile_runtime_command(
    toolchain: &Toolchain,
    source: &Path,
    dest: &Path,
    moon_include_path: &str,
    native_allocator: NativeAllocator,
    collect_ref_cycle: bool,
) -> Vec<String> {
    let mut command = vec![
        toolchain.cc_command_path(),
        "/nologo".to_string(),
        WINDOWS_MSVC_C_STANDARD_FLAG.to_string(),
        "/bigobj".to_string(),
        "/utf-8".to_string(),
        "/wd4819".to_string(),
        "/c".to_string(),
        "/Z7".to_string(),
        "/O2".to_string(),
    ];
    add_cc_runtime_flags(toolchain, &mut command);
    command.extend([
        format!("/Fo{}", dunce::simplified(dest).display()),
        format!(
            "/I{}",
            dunce::simplified(Path::new(moon_include_path)).display()
        ),
        format!(
            "/DMOONBIT_ALLOCATOR={}",
            native_allocator.moonbit_allocator_macro(toolchain.cc(), false)
        ),
    ]);
    if collect_ref_cycle {
        command.push("/DMOONBIT_TRIAL_DELETION=1".to_string());
    }
    command.push(dunce::simplified(source).display().to_string());
    command
}

pub(in crate::build_lower) fn link_executable_command(
    toolchain: &Toolchain,
    sources: &[String],
    link_args: &[NativeLinkArg],
    dest: &str,
    lib_path: &str,
) -> Vec<String> {
    let cc = toolchain.cc();
    let dest = dunce::simplified(Path::new(dest)).display();
    let (mut command, linker_args, library_prefix) = if cc.uses_msvc_cli() {
        (
            vec![
                toolchain.cc_command_path(),
                "/nologo".into(),
                format!("/Fe{dest}"),
            ],
            vec![
                "/link".into(),
                "/nologo".into(),
                "/subsystem:console".into(),
                format!("/LIBPATH:{lib_path}"),
            ],
            "",
        )
    } else {
        // A bare .lib is a file input to clang. Forward the name to the linker
        // so it searches the SDK and CRT library paths.
        (
            vec![toolchain.cc_command_path(), "-o".into(), dest.to_string()],
            vec!["-Wl,/subsystem:console".into(), format!("-L{lib_path}")],
            "-Wl,",
        )
    };
    command.extend(
        sources
            .iter()
            .map(|s| dunce::simplified(Path::new(s)).display().to_string()),
    );
    command.extend(linker_args);
    command.extend(link_args.iter().map(|arg| render_link_arg(arg, cc)));
    command.extend(
        WINDOWS_MSVC_DEFAULT_LIBS
            .iter()
            .map(|lib| format!("{library_prefix}{lib}")),
    );
    command
}

pub(super) fn add_linker_specific_flags(cc: &CC, buf: &mut Vec<String>) {
    if cc.uses_msvc_cli() {
        buf.push("/nologo".to_string());
    }
}

pub(super) fn add_linker_runtime<P: AsRef<Path>>(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<P>,
    lpath: &str,
) {
    if cc.uses_msvc_cli() {
        if let Some(dyn_lib_path) = config.link_shared_runtime.as_ref() {
            buf.push(
                dunce::simplified(&dyn_lib_path.as_ref().join("libruntime.lib"))
                    .display()
                    .to_string(),
            );
        }
        buf.push("/link".to_string());
        buf.push(format!("/LIBPATH:{lpath}"));
    }
}

pub(super) fn add_cc_specific_flags(cc: &CC, buf: &mut Vec<String>, has_user_flags: bool) {
    if !cc.uses_msvc_cli() {
        return;
    }

    buf.push(WINDOWS_MSVC_C_STANDARD_FLAG.to_string());
    // Large generated C files can exceed the default COFF section limit.
    buf.push("/bigobj".to_string());

    if matches!(cc.cc_kind, CCKind::ClangCl) {
        // Generated C relies on wrapping signed arithmetic. Older clang-cl
        // versions otherwise optimize as if signed overflow cannot occur.
        buf.push("/clang:-fwrapv".to_string());
    }

    if !has_user_flags {
        buf.push("/utf-8".to_string());
        buf.push("/wd4819".to_string());
    }
    buf.push("/nologo".to_string());
}

pub(super) fn add_cc_linker_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig, lpath: &str) {
    if cc.uses_msvc_cli() && config.output_ty != OutputType::Object {
        buf.push("/link".to_string());
        buf.push(format!("/LIBPATH:{lpath}"));
    }
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
        for collect_ref_cycle in [false, true] {
            let command = compile_runtime_command(
                &fake_msvc_toolchain(),
                Path::new("runtime.c"),
                Path::new("runtime.obj"),
                "moon/include",
                NativeAllocator::Default,
                collect_ref_cycle,
            );

            assert!(command.iter().any(|arg| arg == "/Imoon/include"));
            assert!(
                command
                    .iter()
                    .any(|arg| arg == WINDOWS_MSVC_C_STANDARD_FLAG)
            );
            assert!(
                command
                    .iter()
                    .any(|arg| arg == WINDOWS_MSVC_STATIC_RUNTIME_FLAG)
            );
            assert!(
                command
                    .iter()
                    .any(|arg| arg == "/DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_SYSTEM")
            );
            assert_eq!(
                command
                    .iter()
                    .any(|arg| arg == "/DMOONBIT_TRIAL_DELETION=1"),
                collect_ref_cycle
            );
        }
    }

    #[test]
    fn plain_clang_links_msvc_objects_using_driver_and_linker_arguments() {
        let toolchain = Toolchain::from_env_override(CC {
            cc_kind: CCKind::Clang,
            cc_path: "clang".into(),
            ar_kind: ARKind::GnuAr,
            ar_path: "llvm-ar".into(),
            target_triple: Some("x86_64-pc-windows-msvc".into()),
            is_env_override: true,
        });
        let args = link_executable_command(
            &toolchain,
            &["main.obj".into()],
            &[
                NativeLinkArg::SearchPath("custom libs".into()),
                NativeLinkArg::Library("custom".into()),
            ],
            "main.exe",
            "moon/lib",
        );
        assert_eq!(
            args,
            [
                "clang",
                "-o",
                "main.exe",
                "main.obj",
                "-Wl,/subsystem:console",
                "-Lmoon/lib",
                "-Lcustom libs",
                "-lcustom",
                "-Wl,libcmt.lib",
                "-Wl,oldnames.lib",
                "-Wl,kernel32.lib",
                "-Wl,shell32.lib",
                "-Wl,user32.lib",
                "-Wl,dbghelp.lib",
                "-Wl,uuid.lib",
            ]
        );
    }

    #[test]
    fn executable_link_command_uses_command_env_for_msvc_environment() {
        let command = link_executable_command(
            &fake_msvc_toolchain(),
            &["main.obj".to_string(), "runtime.obj".to_string()],
            &[
                NativeLinkArg::Library("custom".into()),
                NativeLinkArg::SearchPath("custom libs".into()),
                NativeLinkArg::Opaque("/DEBUG".into()),
            ],
            "main.exe",
            "moon/lib",
        );

        assert_eq!(
            command,
            [
                "cl.exe",
                "/nologo",
                "/Femain.exe",
                "main.obj",
                "runtime.obj",
                "/link",
                "/nologo",
                "/subsystem:console",
                "/LIBPATH:moon/lib",
                "custom.lib",
                "/LIBPATH:custom libs",
                "/DEBUG",
                "libcmt.lib",
                "oldnames.lib",
                "kernel32.lib",
                "shell32.lib",
                "user32.lib",
                "dbghelp.lib",
                "uuid.lib",
            ]
        );
    }
}
