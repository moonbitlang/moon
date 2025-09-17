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

use anyhow::{bail, Context};
use moonutil::cli::UniversalFlags;
use moonutil::common::{
    DriverKind, MooncGenTestInfo, TargetBackend, MOONBITLANG_CORE_BUILTIN,
    MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END,
};
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Generate tests for a provided package. This is a thin wrapper around
/// `moonc gen-test-info`, which does the actual parsing and generation.
#[derive(Debug, clap::Parser)]
pub struct GenerateTestDriverSubcommand {
    /// The paths of the source files to be mapped
    files: Vec<PathBuf>,

    /// Files that need to be mapped, but only extract the doctests, not main contents
    #[clap(long = "doctest-only")]
    doctest_only_files: Vec<PathBuf>,

    /// The output test driver `.mbt` file
    #[clap(long)]
    output_driver: PathBuf,

    /// The output test metadata file
    #[clap(long)]
    output_metadata: PathBuf,

    /// The target backend for the generated test driver.
    #[clap(long = "target")]
    _target_backend: TargetBackend,

    /// The name of the package for which the test driver is generated for.
    #[clap(long)]
    pkg_name: String,

    /// Whether to generate the test driver in bench mode. Not providing this
    /// option will result in test mode.
    #[clap(long)]
    bench: bool,

    /// Whether coverage is enabled in this build. Enabling it will insert
    /// coverage-custom code at the end of the test..
    #[clap(long)]
    enable_coverage: bool,

    /// Override coverage package name; `@self` is a special value that means the package itself
    #[clap(long)]
    coverage_package_override: Option<String>,

    /// The test driver kind
    #[clap(long)]
    driver_kind: DriverKind,

    /// Path to the patch file
    #[clap(long)]
    patch_file: Option<PathBuf>,
}

fn moonc_gen_test_info(
    files: &[PathBuf],
    doctest_only_files: &[PathBuf],
    driver_kind: DriverKind,
    output_path: &Path,
    patch_file: Option<PathBuf>,
) -> anyhow::Result<MooncGenTestInfo> {
    let patch_args = if let Some(patch_file) = patch_file {
        vec!["-patch-file".to_string(), patch_file.display().to_string()]
    } else {
        vec![]
    };
    let include_doctests = match driver_kind {
        DriverKind::Blackbox => Some("-include-doctests"),
        _ => None,
    };
    let mut generated = std::process::Command::new("moonc")
        .arg("gen-test-info")
        .arg("-json")
        .args(files)
        .args(
            doctest_only_files
                .iter()
                .flat_map(|x| [OsStr::new("-doctest-only"), x.as_os_str()]),
        )
        .args(patch_args)
        .args(include_doctests)
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

    // when mauanlly execute command to generate test driver, we need to create the parent directory
    if !output_path.parent().unwrap().exists() {
        std::fs::create_dir_all(output_path.parent().unwrap())?;
    }

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
    return Ok(t);

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
    if cli.dry_run {
        bail!("dry-run is not implemented for generate-test-driver");
    }

    // Create directories if not exists
    cmd.output_metadata
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()?;
    cmd.output_driver
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()?;

    let mbts_test_data = moonc_gen_test_info(
        &cmd.files,
        &cmd.doctest_only_files,
        cmd.driver_kind,
        &cmd.output_metadata,
        cmd.patch_file.clone(),
    )?;

    let generated_content = generate_driver(
        &mbts_test_data,
        &cmd.pkg_name,
        cmd.enable_coverage,
        cmd.bench,
        cmd.coverage_package_override.as_deref(),
    );
    std::fs::write(&cmd.output_driver, generated_content)?;

    Ok(0)
}

const NO_ARGS_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver/no_args_template.mbt"
));

const WITH_ARGS_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver/with_args_template.mbt"
));

const NO_ASYNC_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver/no_async.mbt"
));

const ASYNC_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver/async.mbt"
));

const BENCH_DRIVER_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver/bench_driver_template.mbt"
));

const TEST_DRIVER_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver/test_driver_template.mbt"
));

const COMMON_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver/common.mbt"
));

fn generate_driver(
    data: &MooncGenTestInfo,
    pkgname: &str,
    enable_coverage: bool,
    enable_bench: bool,
    coverage_package_override: Option<&str>,
) -> String {
    // Drivers selection:
    // enable_bench -> only bench tests
    // !enable_bench ->
    //   only no_arg tests are present -> no_arg_test
    //   otherwise -> no_arg_test and with_arg_test

    let no_async_tests = data.async_tests.iter().all(|x| x.1.is_empty());
    let only_no_arg_tests = data.with_args_tests.iter().all(|x| x.1.is_empty()) && no_async_tests;

    let template = if enable_bench {
        BENCH_DRIVER_TEMPLATE
    } else {
        TEST_DRIVER_TEMPLATE
    };
    let mut template = template.to_string();
    if !enable_bench {
        if only_no_arg_tests {
            template.push_str(NO_ARGS_TEMPLATE)
        } else {
            template.push_str(WITH_ARGS_TEMPLATE)
        }
        if no_async_tests {
            template.push_str(NO_ASYNC_TEMPLATE)
        } else {
            template.push_str(ASYNC_TEMPLATE)
        }
    }
    template.push_str(COMMON_TEMPLATE);

    let template = template.replace("\r\n", "\n");

    let coverage_end_template = if enable_coverage {
        let coverage_package_name =
            if let Some(coverage_package_override) = coverage_package_override {
                if coverage_package_override == "@self" {
                    "".into()
                } else {
                    format!("@{coverage_package_override}.")
                }
            } else {
                "@moonbitlang/core/coverage.".into()
            };
        format!("{coverage_package_name}end();")
    } else {
        "".into()
    };

    let template = if enable_bench {
        template.replace(
            "let moonbit_test_driver_internal_with_bench_args_tests : Moonbit_Test_Driver_Internal_TestDriver_With_Bench_Args_Map = { }  // WILL BE REPLACED\n",
            &   MooncGenTestInfo::section_to_mbt("moonbit_test_driver_internal_with_bench_args_tests", &data.with_bench_args_tests),
        )
    } else {
        let template = template.replace(
            "let moonbit_test_driver_internal_no_args_tests : Moonbit_Test_Driver_Internal_No_Args_Map = { }  // WILL BE REPLACED\n",
            &MooncGenTestInfo::section_to_mbt("moonbit_test_driver_internal_no_args_tests", &data.no_args_tests),
        );
        let template = if only_no_arg_tests {
            template
        } else {
            template.replace(
                "let moonbit_test_driver_internal_with_args_tests : Moonbit_Test_Driver_Internal_TestDriver_With_Args_Map = { }  // WILL BE REPLACED\n",
                &MooncGenTestInfo::section_to_mbt("moonbit_test_driver_internal_with_args_tests", &data.with_args_tests),
            )
        };
        if no_async_tests {
            template
        } else {
            template.replace(
                "let moonbit_test_driver_internal_async_tests : Moonbit_Test_Driver_Internal_TestDriver_Async_Map = { }  // WILL BE REPLACED",
                &MooncGenTestInfo::section_to_mbt("moonbit_test_driver_internal_async_tests", &data.async_tests),
            )
        }
    };

    let template =  template
        .replace(
            "let moonbit_test_driver_internal_no_args_tests =",
            "let moonbit_test_driver_internal_no_args_tests : Moonbit_Test_Driver_Internal_No_Args_Map =",
        )
        .replace(
            "let moonbit_test_driver_internal_with_args_tests =",
            "let moonbit_test_driver_internal_with_args_tests : Moonbit_Test_Driver_Internal_TestDriver_With_Args_Map =",
        )
        .replace(
            "let moonbit_test_driver_internal_with_bench_args_tests =",
            "let moonbit_test_driver_internal_with_bench_args_tests : Moonbit_Test_Driver_Internal_TestDriver_With_Bench_Args_Map =",
        )
        .replace(
            "let moonbit_test_driver_internal_async_tests =",
            "let moonbit_test_driver_internal_async_tests : Moonbit_Test_Driver_Internal_TestDriver_Async_Map =",
        )
        .replace("{PACKAGE}", pkgname)
        .replace("{BEGIN_MOONTEST}", MOON_TEST_DELIMITER_BEGIN)
        .replace("{END_MOONTEST}", MOON_TEST_DELIMITER_END)
        .replace("// {COVERAGE_END}", &coverage_end_template);

    if pkgname == MOONBITLANG_CORE_BUILTIN {
        template.replace(&format!("@{MOONBITLANG_CORE_BUILTIN}."), "")
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
