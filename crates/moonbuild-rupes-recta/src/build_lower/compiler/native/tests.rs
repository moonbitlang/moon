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

use std::{env, process::Command};

use moonutil::compiler_flags::{
    ARKind, CCKind, WINDOWS_MSVC_C_STANDARD_FLAG, WINDOWS_MSVC_STATIC_RUNTIME_FLAG,
};

use super::*;

#[test]
#[allow(clippy::disallowed_methods)] // I/O is confined to test fixtures.
fn selected_libraries_are_inputs_but_opaque_link_arguments_are_not() {
    let dir = tempfile::tempdir().expect("create test toolchain directory");
    let backtrace = dir.path().join("libbacktrace.a");
    std::fs::write(&backtrace, "").expect("write optional backtrace archive");
    let paths = CompilerPaths {
        include_path: dir.path().join("include").display().to_string(),
        lib_path: dir.path().display().to_string(),
    };
    let cc = fake_cc(CCKind::Gcc, None);
    let command = make_linker_command(
        cc.clone(),
        LinkerConfig::<&Path> {
            link_moonbitrun: true,
            link_libbacktrace: true,
            ..Default::default()
        },
        &[],
        &["main.o"],
        "build",
        "build/main",
        &paths,
    );
    let mut expected = Vec::new();
    if cfg!(any(target_os = "linux", target_os = "macos")) {
        expected.push(dir.path().join("libmoonbitrun.o"));
    }
    expected.push(backtrace.clone());
    assert_eq!(command.extra_inputs, expected);
    for input in &expected {
        assert!(
            command
                .commandline
                .args()
                .contains(&dunce::simplified(input).display().to_string())
        );
    }

    // An opaque user flag may look exactly like a toolchain library path.
    // Its interpretation must not depend on recognizing text in the argv.
    let opaque = NativeLinkArg::Opaque(backtrace.display().to_string());
    let command = make_linker_command(
        cc,
        LinkerConfig::<&Path>::default(),
        &[opaque],
        &["main.o"],
        "build",
        "build/main",
        &paths,
    );
    assert!(command.extra_inputs.is_empty());
    assert!(
        command
            .commandline
            .args()
            .contains(&backtrace.display().to_string())
    );
}

fn host_triple() -> String {
    let os = match env::consts::OS {
        "macos" => "apple-darwin",
        os => os,
    };
    format!("{}-unknown-{os}", env::consts::ARCH)
}

fn fake_cc(kind: CCKind, target_triple: Option<&str>) -> CC {
    CC {
        cc_kind: kind,
        cc_path: "cc".to_string(),
        ar_kind: ARKind::GnuAr,
        ar_path: "ar".to_string(),
        target_triple: target_triple.map(str::to_string),
        is_env_override: false,
    }
}

fn foreign_triple() -> &'static str {
    if env::consts::ARCH == "x86_64" {
        "aarch64-unknown-linux-musl"
    } else {
        "x86_64-unknown-linux-musl"
    }
}

fn executable_cc_config() -> CCConfig {
    CCConfig {
        debug_info: false,
        link_moonbitrun: false,
        link_libbacktrace: false,
        no_sys_header: false,
        output_ty: OutputType::Executable,
        opt_level: OptLevel::Speed,
        allow_stacktrace: false,
        define_use_shared_runtime_macro: false,
        use_simdutf: false,
        collect_ref_cycle: false,
        native_allocator: None,
    }
}

#[test]
fn gcc_like_floating_point_defaults_preserve_separate_rounding() {
    let paths = CompilerPaths {
        include_path: "include".into(),
        lib_path: "lib".into(),
    };
    for kind in [
        CCKind::SystemCC,
        CCKind::Gcc,
        CCKind::Clang,
        CCKind::Msvc,
        CCKind::ClangCl,
        CCKind::Tcc,
    ] {
        for opt_level in [OptLevel::None, OptLevel::Debug, OptLevel::Speed] {
            for output_ty in [
                OutputType::Object,
                OutputType::Executable,
                OutputType::SharedLib,
            ] {
                for user_flags in [vec![], vec!["-DUSER_OPTION=1"]] {
                    let command = make_cc_command(
                        &Toolchain::from_path_probe(fake_cc(kind, None)),
                        CCConfig {
                            opt_level,
                            output_ty,
                            ..executable_cc_config()
                        },
                        &user_flags,
                        &[],
                        ["main.c"],
                        "build/main",
                        Some("build/main/output"),
                        &paths,
                    )
                    .commandline
                    .args()
                    .to_vec();
                    assert_eq!(
                        command.iter().any(|flag| flag == "-ffp-contract=off"),
                        !matches!(kind, CCKind::Msvc | CCKind::ClangCl | CCKind::Tcc),
                        "unexpected floating-point defaults: {command:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn collect_ref_cycle_flag_is_independent_of_user_compiler_flags() {
    let paths = CompilerPaths {
        include_path: "moon/include".into(),
        lib_path: "moon/lib".into(),
    };
    for kind in [CCKind::Gcc, CCKind::Clang, CCKind::Msvc, CCKind::Tcc] {
        let cc = fake_cc(kind, None);
        let prefix = if cc.uses_msvc_cli() { "/D" } else { "-D" };
        let user_flag = format!("{prefix}USER_OPTION=1");
        for enabled in [false, true] {
            let mut config = executable_cc_config();
            config.collect_ref_cycle = enabled;
            let command = make_cc_command(
                &Toolchain::from_path_probe(cc.clone()),
                config,
                &[&user_flag],
                &[],
                ["program.c"],
                "build",
                Some("program.exe"),
                &paths,
            )
            .commandline
            .args()
            .to_vec();
            assert!(command.contains(&user_flag));
            assert_eq!(
                command.contains(&format!("{prefix}MOONBIT_TRIAL_DELETION=1")),
                enabled,
                "{command:?}"
            );
        }
    }
}

#[test]
fn runtime_allocator_flag_is_emitted_for_gcc_like_runtime_compile() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };
    let mut config = executable_cc_config();
    config.output_ty = OutputType::Object;
    config.native_allocator = Some(NativeAllocator::Mimalloc);

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu"))),
        config,
        &[] as &[&str],
        &[],
        ["runtime.c"],
        "build/runtime",
        Some("build/runtime/runtime.o"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(
        command
            .iter()
            .any(|flag| flag == "-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC")
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn allocator_flag_is_emitted_for_generated_program_c() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };
    let mut config = executable_cc_config();
    config.link_moonbitrun = true;
    config.native_allocator = Some(NativeAllocator::Default);

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some(&host_triple()))),
        config,
        &[] as &[&str],
        &[],
        ["main.c"],
        "build/main",
        Some("build/main/main.exe"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(
        command
            .iter()
            .any(|flag| flag == "-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC")
    );
}

#[test]
fn system_allocator_flag_is_emitted_for_generated_program_c() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };
    let mut config = executable_cc_config();
    config.link_moonbitrun = true;
    config.native_allocator = Some(NativeAllocator::System);

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu"))),
        config,
        &[] as &[&str],
        &[],
        ["main.c"],
        "build/main",
        Some("build/main/main.exe"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(
        command
            .iter()
            .any(|flag| flag == "-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_SYSTEM")
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn default_allocator_runtime_macro_follows_moonbitrun_fallback() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };
    let mut config = executable_cc_config();
    config.output_ty = OutputType::Object;
    config.link_moonbitrun = true;
    config.native_allocator = Some(NativeAllocator::Default);

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some(&host_triple()))),
        config,
        &[] as &[&str],
        &[],
        ["runtime.c"],
        "build/runtime",
        Some("build/runtime/runtime.o"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(
        command
            .iter()
            .any(|flag| flag == "-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC")
    );
}

#[test]
fn default_allocator_runtime_macro_uses_system_when_moonbitrun_is_suppressed() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };
    let mut config = executable_cc_config();
    config.output_ty = OutputType::Object;
    config.link_moonbitrun = true;
    config.native_allocator = Some(NativeAllocator::Default);
    let mut cc = fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu"));
    cc.is_env_override = true;

    let command = make_cc_command(
        &Toolchain::from_path_probe(cc),
        config,
        &[] as &[&str],
        &[],
        ["runtime.c"],
        "build/runtime",
        Some("build/runtime/runtime.o"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(
        command
            .iter()
            .any(|flag| flag == "-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_SYSTEM")
    );
}

#[test]
fn system_allocator_suppresses_moonbitrun_link_input() {
    let command = make_linker_command(
        fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu")),
        LinkerConfig::<&Path> {
            driver_flags: vec![],
            debug_info: false,
            opt_level: None,
            link_moonbitrun: true,
            link_libbacktrace: false,
            output_ty: OutputType::Executable,
            link_shared_runtime: None,
            native_allocator: NativeAllocator::System,
        },
        &[],
        &["main.o"],
        "build/main",
        "build/main/main",
        &CompilerPaths {
            include_path: "include".into(),
            lib_path: "custom-lib".to_string(),
        },
    )
    .commandline
    .args()
    .to_vec();

    assert!(!command.iter().any(|flag| flag.contains("libmoonbitrun.o")));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn explicit_mimalloc_links_moonbitrun_for_env_compiler() {
    let mut cc = fake_cc(CCKind::Gcc, Some(&host_triple()));
    cc.is_env_override = true;

    let command = make_linker_command(
        cc,
        LinkerConfig::<&Path> {
            driver_flags: vec![],
            debug_info: false,
            opt_level: None,
            link_moonbitrun: true,
            link_libbacktrace: false,
            output_ty: OutputType::Executable,
            link_shared_runtime: None,
            native_allocator: NativeAllocator::Mimalloc,
        },
        &[],
        &["main.o"],
        "build/main",
        "build/main/main",
        &CompilerPaths {
            include_path: "include".into(),
            lib_path: "custom-lib".to_string(),
        },
    )
    .commandline
    .args()
    .to_vec();

    let libmoonbitrun_arg = Path::new("custom-lib")
        .join("libmoonbitrun.o")
        .display()
        .to_string();
    assert!(command.iter().any(|arg| arg == &libmoonbitrun_arg));
}

#[test]
fn linker_preserves_driver_flags_before_msvc_linker_arguments() {
    let config = LinkerConfig::<&Path> {
        driver_flags: vec!["/GL".to_string()],
        debug_info: true,
        opt_level: Some(OptLevel::Speed),
        ..Default::default()
    };
    let command = make_linker_command(
        fake_cc(CCKind::Msvc, None),
        config,
        &[NativeLinkArg::SearchPath(PathBuf::from("libraries"))],
        &["program.obj"],
        "output",
        "program.exe",
        &CompilerPaths {
            include_path: "include".into(),
            lib_path: "runtime".to_string(),
        },
    )
    .commandline
    .args()
    .to_vec();
    let separator = command
        .iter()
        .position(|arg| arg == "/link")
        .expect("MSVC linker separator");
    assert!(command[..separator].iter().any(|arg| arg == "/GL"));
    assert!(command[..separator].iter().any(|arg| arg == "/Z7"));
    assert!(
        command[separator..]
            .iter()
            .any(|arg| arg == "/LIBPATH:libraries")
    );
    assert!(!command.iter().any(|arg| arg == "/c" || arg == "/O2"));
}

#[test]
fn archiver_flags_follow_the_resolved_archiver_kind() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };
    let mut apple_cc = fake_cc(CCKind::Clang, Some("arm64-apple-darwin"));
    apple_cc.ar_kind = ARKind::AppleLibtool;
    apple_cc.ar_path = "libtool".to_string();
    assert_eq!(
        make_archiver_command(
            apple_cc,
            ArchiverConfig {
                archive_moonbitrun: false,
                native_allocator: NativeAllocator::Default,
            },
            &["runtime.o"],
            "runtime.a",
            &paths
        )
        .commandline
        .args()
        .to_vec(),
        [
            "libtool",
            "-static",
            "-no_warning_for_no_symbols",
            "-o",
            "runtime.a",
            "runtime.o"
        ]
        .map(str::to_string)
    );

    let mut clang_msvc = fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc"));
    clang_msvc.ar_kind = ARKind::MsvcLib;
    clang_msvc.ar_path = "llvm-lib.exe".to_string();
    assert_eq!(
        make_archiver_command(
            clang_msvc,
            ArchiverConfig {
                archive_moonbitrun: false,
                native_allocator: NativeAllocator::Default,
            },
            &["runtime.obj"],
            "runtime.lib",
            &paths
        )
        .commandline
        .args()
        .to_vec(),
        ["llvm-lib.exe", "/nologo", "/Out:runtime.lib", "runtime.obj"].map(str::to_string)
    );
}

#[test]
fn msvc_abi_enforces_static_crt_after_user_compile_flags() {
    let paths = CompilerPaths {
        include_path: "moon/include".to_string(),
        lib_path: "moon/lib".to_string(),
    };

    for (cc, user_flag, expected_flag) in [
        (
            fake_cc(CCKind::Msvc, None),
            "/MD",
            WINDOWS_MSVC_STATIC_RUNTIME_FLAG,
        ),
        (
            fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc")),
            "-fms-runtime-lib=dll",
            "-fms-runtime-lib=static",
        ),
    ] {
        let command = make_cc_command(
            &Toolchain::from_path_probe(cc),
            CCConfig {
                output_ty: OutputType::Object,
                ..executable_cc_config()
            },
            &[user_flag],
            &[],
            ["stub.c"],
            "pkg",
            Some("stub.obj"),
            &paths,
        );
        let args = command.commandline.args();
        let user = args
            .iter()
            .position(|arg| arg == user_flag)
            .expect("user-supplied CRT flag");
        let forced = args
            .iter()
            .position(|arg| arg == expected_flag)
            .expect("toolchain's static CRT flag");
        assert!(user < forced, "{args:?}");
    }
}

#[test]
fn tcc_compile_driver_keeps_configured_toolchain_lib_path() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "custom-lib".to_string(),
    };
    let mut config = executable_cc_config();
    config.no_sys_header = true;

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Tcc, None)),
        config,
        &[] as &[&str],
        &[],
        ["main.c"],
        "build/main",
        Some("build/main/main"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(command.iter().any(|flag| flag == "-Lcustom-lib"));
}

#[test]
fn tcc_linker_keeps_configured_toolchain_lib_path() {
    let command = make_linker_command(
        fake_cc(CCKind::Tcc, None),
        LinkerConfig::<&Path> {
            driver_flags: vec![],
            debug_info: false,
            opt_level: None,
            link_moonbitrun: false,
            link_libbacktrace: false,
            output_ty: OutputType::Executable,
            link_shared_runtime: None,
            native_allocator: NativeAllocator::Default,
        },
        &[],
        &["main.o"],
        "build/main",
        "build/main/main",
        &CompilerPaths {
            include_path: "include".into(),
            lib_path: "custom-lib".to_string(),
        },
    )
    .commandline
    .args()
    .to_vec();

    assert!(command.iter().any(|flag| flag == "-Lcustom-lib"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn resolved_archiver_uses_configured_lib_path_for_available_moonbitrun() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "custom-lib".to_string(),
    };

    let command = make_archiver_command(
        fake_cc(CCKind::Gcc, Some(&host_triple())),
        ArchiverConfig {
            archive_moonbitrun: true,
            native_allocator: NativeAllocator::Default,
        },
        &["stub.o"],
        "libstub.a",
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    let libmoonbitrun_arg = Path::new("custom-lib")
        .join("libmoonbitrun.o")
        .display()
        .to_string();
    assert!(command.iter().any(|arg| arg == &libmoonbitrun_arg));
}

#[test]
#[allow(clippy::disallowed_methods)] // I/O is confined to test fixtures.
fn cross_compiler_does_not_link_host_libbacktrace() {
    let temp_dir = std::env::temp_dir().join(format!(
        "moonutil-cross-libbacktrace-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).expect("create temp lib dir");
    let libbacktrace_path = temp_dir.join("libbacktrace.a");
    std::fs::write(&libbacktrace_path, b"").expect("create temp libbacktrace.a");
    let libbacktrace_arg = libbacktrace_path.display().to_string();

    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: temp_dir.display().to_string(),
    };
    let mut config = executable_cc_config();
    config.link_libbacktrace = true;
    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some(foreign_triple()))),
        config,
        &[] as &[&str],
        &[],
        ["main.c"],
        "build/main",
        Some("build/main/main"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();
    assert!(!command.iter().any(|flag| flag == &libbacktrace_arg));

    let linker_config = LinkerConfig::<&Path> {
        link_moonbitrun: false,
        link_libbacktrace: true,
        output_ty: OutputType::Executable,
        ..Default::default()
    };
    let command = make_linker_command(
        fake_cc(CCKind::Gcc, Some(foreign_triple())),
        linker_config,
        &[],
        &["main.o"],
        "build/main",
        "build/main/main",
        &CompilerPaths {
            include_path: "include".into(),
            lib_path: temp_dir.display().to_string(),
        },
    )
    .commandline
    .args()
    .to_vec();
    assert!(!command.iter().any(|flag| flag == &libbacktrace_arg));

    std::fs::remove_dir_all(temp_dir).expect("remove temp lib dir");
}

#[test]
fn link_flags_do_not_disable_default_optimization_flags() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu"))),
        executable_cc_config(),
        &[] as &[&str],
        &[NativeLinkArg::Opaque("-lcustom".into())],
        ["main.c"],
        "build/main",
        Some("build/main/main"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(command.iter().any(|flag| flag == "-O2"));
    assert!(command.iter().any(|flag| flag == "-lcustom"));
}

#[test]
fn msvc_compile_flags_keep_nologo() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Msvc, None)),
        executable_cc_config(),
        &["/O2"],
        &[],
        ["main.c"],
        "build/main",
        Some("build/main/main.exe"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(command.iter().any(|flag| flag == "/nologo"));
    assert!(
        command
            .iter()
            .any(|flag| flag == WINDOWS_MSVC_C_STANDARD_FLAG)
    );
    assert!(command.iter().any(|flag| flag == "/O2"));
    assert!(!command.iter().any(|flag| flag == "/utf-8"));
    assert!(!command.iter().any(|flag| flag == "/wd4819"));
}

#[test]
fn native_link_args_use_driver_syntax() {
    let paths = CompilerPaths {
        include_path: "moon/include".into(),
        lib_path: "moon/lib".into(),
    };
    let link_args = [
        NativeLinkArg::Opaque("custom dir/custom.lib".into()),
        NativeLinkArg::Library("SDL3".into()),
        NativeLinkArg::SearchPath("SDL libs".into()),
        NativeLinkArg::Library("SDL3".into()),
    ];

    for (driver, kind, triple, expected) in [
        (
            "clang.exe",
            CCKind::Clang,
            Some("x86_64-pc-windows-msvc"),
            ["custom dir/custom.lib", "-lSDL3", "-LSDL libs", "-lSDL3"],
        ),
        (
            "clang.exe",
            CCKind::Clang,
            Some("x86_64-w64-windows-gnu"),
            ["custom dir/custom.lib", "-lSDL3", "-LSDL libs", "-lSDL3"],
        ),
        (
            "cl.exe",
            CCKind::Msvc,
            None,
            [
                "custom dir/custom.lib",
                "SDL3.lib",
                "/LIBPATH:SDL libs",
                "SDL3.lib",
            ],
        ),
        (
            "clang-cl.exe",
            CCKind::ClangCl,
            None,
            [
                "custom dir/custom.lib",
                "SDL3.lib",
                "/LIBPATH:SDL libs",
                "SDL3.lib",
            ],
        ),
    ] {
        let mut cc = fake_cc(kind, triple);
        cc.cc_path = driver.into();
        let toolchain = Toolchain::from_path_probe(cc.clone());
        let generated_c = make_cc_command(
            &toolchain,
            executable_cc_config(),
            &[] as &[&str],
            &link_args,
            ["main.c"],
            "build",
            Some("build/main.exe"),
            &paths,
        )
        .commandline
        .args()
        .to_vec();
        let link_only = make_linker_command(
            cc,
            LinkerConfig::<&Path> {
                driver_flags: vec![],
                debug_info: false,
                opt_level: None,
                link_moonbitrun: false,
                link_libbacktrace: false,
                output_ty: OutputType::Executable,
                link_shared_runtime: None,
                native_allocator: NativeAllocator::Default,
            },
            &link_args,
            &["main.obj"],
            "build",
            "build/main.exe",
            &paths,
        )
        .commandline
        .args()
        .to_vec();

        for command in [generated_c, link_only] {
            let start = command.len() - expected.len();
            assert_eq!(
                &command[start..],
                expected,
                "driver: {driver}, target: {triple:?}, command: {command:?}"
            );
            if toolchain.uses_msvc_cli() {
                let link = command
                    .iter()
                    .position(|arg| arg == "/link")
                    .expect("MSVC linker separator");
                assert!(link < start, "{command:?}");
            } else {
                assert!(!command.iter().any(|arg| arg == "/link"), "{command:?}");
            }
        }
    }
}

#[test]
fn msvc_cc_link_flags_follow_link_separator() {
    let paths = CompilerPaths {
        include_path: "moon/include".to_string(),
        lib_path: "moon/lib".to_string(),
    };

    for (driver, kind) in [("cl.exe", CCKind::Msvc), ("clang-cl.exe", CCKind::ClangCl)] {
        for output_ty in [OutputType::Executable, OutputType::SharedLib] {
            let mut cc = fake_cc(kind, None);
            cc.cc_path = driver.to_string();
            let toolchain = Toolchain::from_path_probe(cc);
            let mut config = executable_cc_config();
            config.output_ty = output_ty;
            let command = make_cc_command(
                &toolchain,
                config,
                &["/O2"],
                &[
                    NativeLinkArg::Library("SDL3".into()),
                    NativeLinkArg::SearchPath("SDL libs".into()),
                    NativeLinkArg::Opaque("/DEBUG".into()),
                ],
                ["main.c"],
                "build",
                Some("build/main.exe"),
                &paths,
            )
            .commandline
            .args()
            .to_vec();

            let link = command
                .iter()
                .position(|arg| arg == "/link")
                .expect("MSVC link command needs /link");
            for arg in ["main.c", "/O2", "/MT"] {
                assert!(
                    command[..link].iter().any(|flag| flag == arg),
                    "{command:?}"
                );
            }
            assert_eq!(
                &command[link + 1..],
                [
                    "/LIBPATH:moon/lib",
                    "SDL3.lib",
                    "/LIBPATH:SDL libs",
                    "/DEBUG"
                ],
                "driver: {driver}, command: {command:?}"
            );
        }
    }
}

#[test]
fn msvc_link_flags_keep_nologo() {
    let command = make_linker_command(
        fake_cc(CCKind::Msvc, None),
        LinkerConfig::<&Path> {
            driver_flags: vec![],
            debug_info: false,
            opt_level: None,
            link_moonbitrun: false,
            link_libbacktrace: false,
            output_ty: OutputType::Executable,
            link_shared_runtime: None,
            native_allocator: NativeAllocator::Default,
        },
        &[NativeLinkArg::Opaque("/DEBUG".into())],
        &["main.obj"],
        "build",
        "build/main.exe",
        &CompilerPaths {
            include_path: "include".into(),
            lib_path: "lib".to_string(),
        },
    )
    .commandline
    .args()
    .to_vec();

    assert!(command.iter().any(|flag| flag == "/nologo"));
    assert!(command.iter().any(|flag| flag == "/DEBUG"));
}

#[cfg(windows)]
#[test]
fn msvc_simplifies_short_inputs_but_preserves_long_paths_and_output_directories() {
    let short_source = r"\\?\C:\build\main.c";
    let long_source = format!(r"\\?\C:\build\{}main.c", "segment\\".repeat(40));
    let output_dir = r"\\?\C:\build";
    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Msvc, None)),
        executable_cc_config(),
        &[] as &[&str],
        &[],
        [short_source, &long_source],
        output_dir,
        Some(r"\\?\C:\build\main.exe"),
        &CompilerPaths {
            include_path: "include".into(),
            lib_path: "lib".into(),
        },
    )
    .commandline
    .args()
    .to_vec();

    assert!(command.iter().any(|arg| arg == r"C:\build\main.c"));
    assert!(!command.iter().any(|arg| arg == short_source));
    assert!(command.contains(&long_source));
    assert!(command.contains(&format!("/Fo{output_dir}\\")));
    assert!(command.contains(&r"/FeC:\build\main.exe".to_owned()));
}

#[test]
fn build_system_flags_keep_default_optimization_flags() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };
    let mut config = executable_cc_config();
    config.allow_stacktrace = true;

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu"))),
        config,
        &[] as &[&str],
        &[],
        ["runtime.c"],
        "build",
        Some("build/runtime.o"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(command.iter().any(|flag| flag == "-O2"));
    assert!(
        command
            .iter()
            .any(|flag| flag == "-DMOONBIT_ALLOW_STACKTRACE")
    );
}

#[test]
#[allow(clippy::disallowed_methods)] // I/O is confined to test fixtures.
fn configured_libbacktrace_does_not_disable_default_optimization_flags() {
    let temp_dir = std::env::temp_dir().join(format!(
        "moonutil-libbacktrace-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).expect("create temp lib dir");
    let libbacktrace_path = temp_dir.join("libbacktrace.a");
    std::fs::write(&libbacktrace_path, b"").expect("create temp libbacktrace.a");

    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: temp_dir.display().to_string(),
    };
    let mut config = executable_cc_config();
    config.link_libbacktrace = true;

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some(&host_triple()))),
        config,
        &[] as &[&str],
        &[NativeLinkArg::Opaque("-lcustom".into())],
        ["main.c"],
        "build/main",
        Some("build/main/main"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(command.iter().any(|flag| flag == "-O2"));
    let user_link_flag = command
        .iter()
        .position(|flag| flag == "-lcustom")
        .expect("user link flag should be present");
    let libbacktrace_arg = libbacktrace_path.display().to_string();
    let libbacktrace_flag = command
        .iter()
        .position(|flag| flag == &libbacktrace_arg)
        .expect("libbacktrace should be present");
    assert!(user_link_flag < libbacktrace_flag);

    std::fs::remove_dir_all(temp_dir).expect("remove temp lib dir");
}

#[test]
fn configured_simdutf_keeps_default_optimization_flags() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };
    let mut config = executable_cc_config();
    config.use_simdutf = true;

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu"))),
        config,
        &[] as &[&str],
        &[],
        ["runtime.c"],
        "build",
        Some("build/runtime.o"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(command.iter().any(|flag| flag == "-O2"));
    assert!(command.iter().any(|flag| flag == "-DMOONBIT_USE_SIMDUTF"));
}

#[test]
fn compile_flags_still_disable_default_optimization_flags() {
    let paths = CompilerPaths {
        include_path: "include".to_string(),
        lib_path: "lib".to_string(),
    };

    let command = make_cc_command(
        &Toolchain::from_path_probe(fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu"))),
        executable_cc_config(),
        &["-O3"],
        &[],
        ["main.c"],
        "build/main",
        Some("build/main/main"),
        &paths,
    )
    .commandline
    .args()
    .to_vec();

    assert!(!command.iter().any(|flag| flag == "-O2"));
    assert!(command.iter().any(|flag| flag == "-O3"));
}

#[test]
#[allow(clippy::disallowed_methods)] // I/O is confined to test fixtures.
fn llvm_lib_archives_real_msvc_object_with_or_without_target_metadata() {
    let tools = moonutil::toolchain::resolve_executable("clang")
        .ok()
        .and_then(|clang| {
            let output = Command::new(&clang)
                .arg("-print-prog-name=llvm-lib")
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let reported = String::from_utf8(output.stdout).ok()?;
            let llvm_lib =
                moonutil::toolchain::resolve_executable(reported.lines().next()?.trim()).ok()?;
            Some((clang, llvm_lib.display().to_string()))
        });
    let Some((clang, llvm_lib)) = tools else {
        if cfg!(windows) {
            panic!("Windows CI requires clang and llvm-lib");
        }
        eprintln!("skipping MSVC archive test: clang and llvm-lib are unavailable");
        return;
    };

    // Compile a header-free MSVC object so the test needs no Windows SDK
    // and can exercise the real tools on any host.
    let dir = tempfile::tempdir().expect("create compiler working directory");
    std::fs::write(
        dir.path().join("answer.c"),
        "int answer(void) { return 42; }\n",
    )
    .expect("write C source");
    let target_flag = "--target=x86_64-pc-windows-msvc";
    let target = Command::new(&clang)
        .args([target_flag, "-dumpmachine"])
        .output()
        .expect("probe Clang target");
    assert!(target.status.success(), "{target:?}");
    let target = String::from_utf8(target.stdout)
        .expect("Clang target is UTF-8")
        .trim()
        .to_string();
    let compiled = Command::new(&clang)
        .current_dir(dir.path())
        .args([target_flag, "-c", "answer.c", "-o", "answer.o"])
        .output()
        .expect("compile MSVC object");
    assert!(compiled.status.success(), "{compiled:?}");

    // Missing probe metadata must not change the selected librarian's CLI.
    for target in [Some(target.as_str()), None] {
        let mut cc = CC::try_from_path_with_ar(&clang.display().to_string(), &llvm_lib)
            .expect("select explicit LLVM librarian");
        cc.target_triple = target.map(str::to_string);
        let command = make_archiver_command(
            cc,
            ArchiverConfig {
                archive_moonbitrun: false,
                native_allocator: NativeAllocator::Default,
            },
            &["answer.o"],
            "answer.lib",
            &CompilerPaths {
                include_path: ".".to_string(),
                lib_path: ".".to_string(),
            },
        )
        .commandline
        .args()
        .to_vec();
        let archived = Command::new(&command[0])
            .current_dir(dir.path())
            .args(&command[1..])
            .output()
            .expect("run Moon's archive command");
        assert!(archived.status.success(), "{command:?}: {archived:?}");

        let members = Command::new(&llvm_lib)
            .current_dir(dir.path())
            .args(["/list", "answer.lib"])
            .output()
            .expect("inspect archive members");
        assert!(members.status.success(), "{members:?}");
        assert_eq!(String::from_utf8_lossy(&members.stdout).trim(), "answer.o");
        std::fs::remove_file(dir.path().join("answer.lib")).expect("remove checked archive");
    }
}

#[test]
fn clang_msvc_target_does_not_link_libm() {
    let cc = fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc"));

    let mut cc_flags = vec![];
    gnu::add_cc_common_libraries(&cc, &mut cc_flags, &executable_cc_config());
    assert!(!cc_flags.iter().any(|f| f == "-lm"));

    let linker_config = LinkerConfig::<&Path> {
        driver_flags: vec![],
        debug_info: false,
        opt_level: None,
        link_moonbitrun: false,
        link_libbacktrace: false,
        output_ty: OutputType::Executable,
        link_shared_runtime: None,
        native_allocator: NativeAllocator::Default,
    };
    let mut linker_flags = vec![];
    gnu::add_linker_common_libraries(&cc, &mut linker_flags, &linker_config);
    assert!(!linker_flags.iter().any(|f| f == "-lm"));
}

#[test]
fn clang_gnu_target_keeps_linking_libm() {
    let cc = fake_cc(CCKind::Clang, Some("x86_64-unknown-linux-gnu"));

    let mut cc_flags = vec![];
    gnu::add_cc_common_libraries(&cc, &mut cc_flags, &executable_cc_config());
    assert!(cc_flags.iter().any(|f| f == "-lm"));

    let linker_config = LinkerConfig::<&Path> {
        driver_flags: vec![],
        debug_info: false,
        opt_level: None,
        link_moonbitrun: false,
        link_libbacktrace: false,
        output_ty: OutputType::Executable,
        link_shared_runtime: None,
        native_allocator: NativeAllocator::Default,
    };
    let mut linker_flags = vec![];
    gnu::add_linker_common_libraries(&cc, &mut linker_flags, &linker_config);
    assert!(linker_flags.iter().any(|f| f == "-lm"));
}
