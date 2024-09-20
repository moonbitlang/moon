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

use super::BuildFlags;
use anyhow::{bail, Context};
use colored::Colorize;
use mooncake::pkg::sync::auto_sync;
use moonutil::cli::UniversalFlags;
use moonutil::common::{
    lower_surface_targets, DriverKind, MoonbuildOpt, MooncGenTestInfo, RunMode, TargetBackend,
    TestOpt, BLACKBOX_TEST_DRIVER, INTERNAL_TEST_DRIVER, MOONBITLANG_CORE,
    MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END, TEST_INFO_FILE, WHITEBOX_TEST_DRIVER,
};
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Generate tests for the provided packages
#[derive(Debug, clap::Parser)]
pub struct GenerateTestDriverSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// The paths of the packages
    #[clap(short, long, num_args(0..))]
    pub package: Option<Vec<PathBuf>>,

    /// Override coverage package name; `@self` is a special value that means the package itself
    #[clap(long)]
    pub coverage_package_override: Option<String>,

    /// The test driver kind
    #[clap(long)]
    pub driver_kind: DriverKind,
}

fn moonc_gen_test_info(files: &[PathBuf], output_path: &Path) -> anyhow::Result<String> {
    let mut generated = std::process::Command::new("moonc")
        .arg("gen-test-info")
        .arg("-json")
        .args(files)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .with_context(|| gen_error_message(files))?;
    let mut out = String::new();
    generated
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .with_context(|| gen_error_message(files))?;
    generated.wait()?;

    let test_info_json_path = output_path;
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(test_info_json_path)
        .context(format!(
            "failed to open file: {}",
            test_info_json_path.display()
        ))?
        .write_all(out.as_bytes())
        .context(format!(
            "failed to write file: {}",
            test_info_json_path.display()
        ))?;

    let t: MooncGenTestInfo = serde_json_lenient::from_str(&out)?;
    return Ok(t.to_mbt());

    fn gen_error_message(files: &[PathBuf]) -> String {
        format!(
            "failed to execute `moonc gen-test-info {}`",
            files
                .iter()
                .map(|it| it.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}

pub fn generate_test_driver(
    cli: UniversalFlags,
    cmd: GenerateTestDriverSubcommand,
) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let mut cmd = cmd;
    let target_backend = cmd.build_flags.target.as_ref().and_then(|surface_targets| {
        if surface_targets.is_empty() {
            None
        } else {
            Some(lower_surface_targets(surface_targets)[0])
        }
    });
    cmd.build_flags.target_backend = target_backend;

    let moonc_opt = super::get_compiler_flags(&source_dir, &cmd.build_flags)?;

    let run_mode = RunMode::Test;
    let sort_input = cmd.build_flags.sort_input;
    let filter_package = cmd.package.map(|it| it.into_iter().collect());

    // Resolve dependencies, but don't download anything
    let (resolved_env, dir_sync_result) = auto_sync(
        &source_dir,
        &AutoSyncFlags { frozen: true },
        &RegistryConfig::load(),
        cli.quiet,
    )?;
    let raw_target_dir = target_dir.to_path_buf();

    let moonbuild_opt = MoonbuildOpt {
        source_dir,
        raw_target_dir,
        target_dir: target_dir.clone(),
        test_opt: Some(TestOpt {
            filter_package: filter_package.clone(),
            filter_file: None,
            filter_index: None,
            limit: 256,
            test_failure_json: false,
            display_backend_hint: None,
        }),
        fmt_opt: None,
        sort_input,
        run_mode,
        args: vec![],
        verbose: cli.verbose,
        quiet: cli.quiet,
        output_json: false,
        no_parallelize: false,
        build_graph: false,
    };

    let module = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &moonbuild_opt,
    )?;
    if cli.dry_run {
        bail!("dry-run is not implemented for generate-test-driver");
    }

    for (pkgname, pkg) in module.packages.iter() {
        if let Some(ref package) = filter_package {
            if !package.contains(Path::new(pkgname)) {
                continue;
            }
        }

        if pkg.is_third_party {
            continue;
        }

        let (files, driver_name) = match cmd.driver_kind {
            DriverKind::Internal => (&pkg.files, INTERNAL_TEST_DRIVER),
            DriverKind::Whitebox => (&pkg.wbtest_files, WHITEBOX_TEST_DRIVER),
            DriverKind::Blackbox => (&pkg.test_files, BLACKBOX_TEST_DRIVER),
        };

        let backend_filtered: Vec<PathBuf> = moonutil::common::backend_filter(
            files,
            moonc_opt.build_opt.debug_flag,
            moonc_opt.build_opt.target_backend,
        )
        .into_iter()
        .filter(|file| {
            // workaround for skip test coverage.mbt in builtin when --enable-coverage is specified
            !(moonc_opt.build_opt.enable_coverage
                && pkgname == "moonbitlang/core/builtin"
                && file.to_str().unwrap().contains("coverage.mbt"))
        })
        .collect();

        let mbts_test_data = moonc_gen_test_info(
            &backend_filtered,
            &target_dir.join(pkg.rel.fs_full_name()).join(format!(
                "__{}_{}",
                cmd.driver_kind.to_string(),
                TEST_INFO_FILE,
            )),
        )?;

        if pkg.is_main && mbts_test_data.contains("(__test_") {
            eprintln!(
                "{}: tests in the main package `{}` will be ignored",
                "Warning".yellow().bold(),
                pkgname
            )
        }
        let generated_content = generate_driver(
            &mbts_test_data,
            pkgname,
            target_backend,
            cmd.build_flags.enable_coverage,
            cmd.coverage_package_override.as_deref(),
        );
        let generated_file = target_dir.join(pkg.rel.fs_full_name()).join(driver_name);

        if !generated_file.parent().unwrap().exists() {
            std::fs::create_dir_all(generated_file.parent().unwrap())?;
        }
        std::fs::write(&generated_file, generated_content)?;
    }

    Ok(0)
}

fn generate_driver(
    data: &str,
    pkgname: &str,
    target_backend: Option<TargetBackend>,
    enable_coverage: bool,
    coverage_package_override: Option<&str>,
) -> String {
    let index = data
        .find("let moonbit_test_driver_internal_with_args_tests =")
        .unwrap_or(data.len());
    let no_args = &data[0..index];
    let with_args = &data[index..];

    let only_no_arg_tests = !data[index..].contains("__test_");

    let args_processing = match target_backend.unwrap_or_default() {
        TargetBackend::Wasm | TargetBackend::WasmGC => {
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../moonbuild/template/wasm_args.mbt"
            ))
        }
        TargetBackend::Js => {
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../moonbuild/template/js_args.mbt"
            ))
        }
    };

    let mut template = if only_no_arg_tests {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/no_args_driver_template.mbt"
        )).to_string()
    }
    else {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/with_args_driver_template.mbt"
        )).to_string()
    }
    .replace("\r\n", "\n")
    .replace("fn moonbit_test_driver_internal_get_file_name(file_name : MoonbitTestDriverInternalExternString) -> String { panic() }\n", "")
    .replace("type MoonbitTestDriverInternalExternString\n", "");

    let coverage_end_template = if enable_coverage {
        let coverage_package_name =
            if let Some(coverage_package_override) = coverage_package_override {
                if coverage_package_override == "@self" {
                    "".into()
                } else {
                    format!("@{}.", coverage_package_override)
                }
            } else {
                "@moonbitlang/core/coverage.".into()
            };
        format!("{}end();", coverage_package_name)
    } else {
        "".into()
    };

    template.push_str(args_processing);
    template = template
        .replace("\r\n", "\n")
        .replace(
            "let moonbit_test_driver_internal_no_args_tests : Moonbit_Test_Driver_Internal_No_Args_Map = { }  // WILL BE REPLACED\n",
            no_args,
        )
        .replace(
            "let moonbit_test_driver_internal_with_args_tests : Moonbit_Test_Driver_Internal_TestDriver_With_Args_Map = { }  // WILL BE REPLACED\n",
            with_args,
        )
        .replace(
            "let moonbit_test_driver_internal_no_args_tests =",
            "let moonbit_test_driver_internal_no_args_tests : Moonbit_Test_Driver_Internal_No_Args_Map =",
        )
        .replace(
            "let moonbit_test_driver_internal_with_args_tests =",
            "let moonbit_test_driver_internal_with_args_tests : Moonbit_Test_Driver_Internal_TestDriver_With_Args_Map =",
        )
        .replace("{PACKAGE}", pkgname)
        .replace("{BEGIN_MOONTEST}", MOON_TEST_DELIMITER_BEGIN)
        .replace("{END_MOONTEST}", MOON_TEST_DELIMITER_END)
        .replace("// {COVERAGE_END}", &coverage_end_template);

    if pkgname.starts_with(MOONBITLANG_CORE) {
        template.replace(&format!("@{}/builtin.", MOONBITLANG_CORE), "")
    } else {
        template
    }
}

#[test]
fn test_base16() {
    /// This function is currently unused.
    /// It is retained for documentation purposes, particularly for test name encoding.
    fn base16_encode_lower(bytes: &[u8]) -> String {
        fn to_char(x: u8) -> char {
            if x < 10 {
                (b'0' + x) as char
            } else {
                (b'a' + x - 10) as char
            }
        }

        let mut result = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            let high = to_char(b >> 4);
            let low = to_char(b & 0xf);
            result.push(high);
            result.push(low);
        }
        result
    }

    use expect_test::expect;

    fn check(a: &str, b: expect_test::Expect) {
        let bytes = a.as_bytes();
        let b16 = base16_encode_lower(bytes);
        b.assert_eq(&b16)
    }

    check(
        "abcdefghijklmnopqrstuvwxyz0123456789",
        expect!["6162636465666768696a6b6c6d6e6f707172737475767778797a30313233343536373839"],
    );
    check(
        "一个中文字符串的编码",
        expect!["e4b880e4b8aae4b8ade69687e5ad97e7aca6e4b8b2e79a84e7bc96e7a081"],
    );
    check(
        "(){}[].+-*/='\"\\|~_:",
        expect!["28297b7d5b5d2e2b2d2a2f3d27225c7c7e5f3a"],
    );
    check("filename.mbt", expect!["66696c656e616d652e6d6274"]);
}
