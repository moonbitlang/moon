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

use anyhow::{Context, bail};
use moonutil::cli::UniversalFlags;
use moonutil::common::{
    DriverKind, MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END, MOONBITLANG_CORE,
    MOONBITLANG_CORE_BUILTIN, MOONBITLANG_CORE_PRELUDE, MooncGenTestInfo, TargetBackend,
};
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Generate tests for a provided package. This is a thin wrapper around
/// `moonc gen-test-info`, which does the actual parsing and generation.
#[derive(Debug, clap::Parser)]
pub(crate) struct GenerateTestDriverSubcommand {
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
    target_backend: TargetBackend,

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

    /// Max concurrent tests for `async test`
    #[clap(long)]
    max_concurrent_tests: Option<u32>,
}

fn moonc_gen_test_info(
    files: &[PathBuf],
    doctest_only_files: &[PathBuf],
    driver_kind: DriverKind,
    output_path: &Path,
    patch_file: Option<PathBuf>,
    target_backend: TargetBackend,
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
    let mut generated = std::process::Command::new(&*moonutil::BINARIES.moonc)
        .arg("gen-test-info")
        .arg("-json")
        .args(files)
        .args(
            doctest_only_files
                .iter()
                .flat_map(|x| [OsStr::new("-doctest-only"), x.as_os_str()]),
        )
        .arg("-target")
        .arg(target_backend.to_flag())
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
            "failed to execute `{:?} gen-test-info {}`",
            moonutil::BINARIES.moonc,
            files
                .iter()
                .map(|it| it.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}

pub(crate) fn generate_test_driver(
    cli: UniversalFlags,
    cmd: GenerateTestDriverSubcommand,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for generate-test-driver");
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
        cmd.target_backend,
    )?;

    let generated_content = generate_driver(
        &mbts_test_data,
        &cmd.pkg_name,
        cmd.enable_coverage,
        cmd.bench,
        cmd.coverage_package_override.as_deref(),
        cmd.max_concurrent_tests,
    );
    std::fs::write(&cmd.output_driver, generated_content)?;

    Ok(0)
}

const TYPES_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver_project/types.mbt"
));

const COMMON_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver_project/common.mbt"
));

const ENTRY_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver_project/entry.mbt"
));

const TEMPLATE_NO_ARGS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver_project/template_no_args.mbt"
));

const TEMPLATE_WITH_ARGS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver_project/template_with_args.mbt"
));

const TEMPLATE_ASYNC_RUNTIME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver_project/template_async.mbt"
));

const TEMPLATE_NO_ARGS_ASYNC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver_project/template_no_args_async.mbt"
));

const TEMPLATE_WITH_ARGS_ASYNC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver_project/template_with_args_async.mbt"
));

const TEMPLATE_WITH_BENCH_ARGS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../moonbuild/template/test_driver_project/template_with_bench_args.mbt"
));

fn generate_driver(
    data: &MooncGenTestInfo,
    pkgname: &str,
    enable_coverage: bool,
    enable_bench: bool,
    coverage_package_override: Option<&str>,
    max_concurrent_tests: Option<u32>,
) -> String {
    // Driver selection : determine which templates to include
    let has_no_args = data.no_args_tests.values().any(|v| !v.is_empty());
    let has_with_args = data.with_args_tests.values().any(|v| !v.is_empty());
    let has_no_args_async = data.async_tests.values().any(|v| !v.is_empty());
    let has_with_args_async = data.async_tests_with_args.values().any(|v| !v.is_empty());
    let has_bench_args = data.with_bench_args_tests.values().any(|v| !v.is_empty());

    let has_any_async = has_no_args_async || has_with_args_async;

    let mut template = String::new();
    template.push_str(TYPES_TEMPLATE);
    template.push_str(COMMON_TEMPLATE);

    if enable_bench {
        if has_bench_args {
            template.push_str(
                TEMPLATE_WITH_BENCH_ARGS
                    .replace(
                        "{} // REPLACE ME: moonbit_test_driver_internal_with_bench_args_tests",
                        &MooncGenTestInfo::section_to_mbt(&data.with_bench_args_tests),
                    )
                    .as_str(),
            );
        }
    } else {
        if has_no_args {
            template.push_str(
                TEMPLATE_NO_ARGS
                    .replace(
                        "{} // REPLACE ME: moonbit_test_driver_internal_no_args_tests",
                        &MooncGenTestInfo::section_to_mbt(&data.no_args_tests),
                    )
                    .as_str(),
            );
        }
        if has_with_args {
            template.push_str(
                TEMPLATE_WITH_ARGS
                    .replace(
                        "{} // REPLACE ME: moonbit_test_driver_internal_with_args_tests",
                        &MooncGenTestInfo::section_to_mbt(&data.with_args_tests),
                    )
                    .as_str(),
            );
        }
        if has_any_async {
            template.push_str(
                TEMPLATE_ASYNC_RUNTIME
                    .replace(
                        "0 // REPLACE ME: moonbit_test_driver_internal_max_concurrent_tests",
                        &format!("{}", max_concurrent_tests.unwrap_or(10)),
                    )
                    .as_str(),
            );
        }
        if has_no_args_async {
            template.push_str(
                TEMPLATE_NO_ARGS_ASYNC
                    .replace(
                        "{} // REPLACE ME: moonbit_test_driver_internal_async_tests",
                        &MooncGenTestInfo::section_to_mbt(&data.async_tests),
                    )
                    .as_str(),
            );
        }
        if has_with_args_async {
            template.push_str(
                TEMPLATE_WITH_ARGS_ASYNC
                    .replace(
                        "{} // REPLACE ME: moonbit_test_driver_internal_async_tests_with_args",
                        &MooncGenTestInfo::section_to_mbt(&data.async_tests_with_args),
                    )
                    .as_str(),
            );
        }
    }

    template.push_str(ENTRY_TEMPLATE);

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

    let template = template
        .replace("{PACKAGE}", pkgname)
        .replace("{BEGIN_MOONTEST}", MOON_TEST_DELIMITER_BEGIN)
        .replace("{END_MOONTEST}", MOON_TEST_DELIMITER_END)
        .replace("// {COVERAGE_END}", &coverage_end_template);

    if pkgname == MOONBITLANG_CORE_BUILTIN {
        template.replace(&format!("@{MOONBITLANG_CORE_PRELUDE}."), "")
    } else if pkgname.starts_with(MOONBITLANG_CORE) {
        template.replace(
            &format!("@{MOONBITLANG_CORE_PRELUDE}."),
            &format!("@{MOONBITLANG_CORE_BUILTIN}."),
        )
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
