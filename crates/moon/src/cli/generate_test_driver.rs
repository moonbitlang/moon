use super::BuildFlags;
use anyhow::bail;
use colored::Colorize;
use mooncake::pkg::sync::auto_sync;
use moonutil::cli::UniversalFlags;
use moonutil::common::{
    MoonbuildOpt, RunMode, TestOpt, MOONBITLANG_CORE, MOON_TEST_DELIMITER_BEGIN,
    MOON_TEST_DELIMITER_END,
};
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use regex::Regex;
use std::path::{Path, PathBuf};

/// Test the current package
#[derive(Debug, clap::Parser)]
pub struct GeneratedTestDriverSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(short, long, num_args(0..))]
    pub package: Option<Vec<PathBuf>>,

    #[clap(short, long, requires("package"))]
    pub file: Option<String>,

    #[clap(short, long, requires("file"))]
    pub index: Option<u32>,
}

pub fn generate_test_driver(
    cli: UniversalFlags,
    cmd: GeneratedTestDriverSubcommand,
) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let moonc_opt = super::get_compiler_flags(&source_dir, &cmd.build_flags)?;

    let run_mode = RunMode::Test;
    let sort_input = cmd.build_flags.sort_input;
    let (filter_package, filter_file, filter_index) = (
        cmd.package.map(|it| it.into_iter().collect()),
        cmd.file,
        cmd.index,
    );

    // Resolve dependencies, but don't download anything
    let (resolved_env, dir_sync_result) = auto_sync(
        &source_dir,
        &AutoSyncFlags { frozen: true },
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let moonbuild_opt = MoonbuildOpt {
        source_dir,
        target_dir: target_dir.clone(),
        test_opt: Some(TestOpt {
            filter_package: filter_package.clone(),
            filter_file: filter_file.clone(),
            filter_index,
        }),
        fmt_opt: None,
        sort_input,
        run_mode,
        ..Default::default()
    };

    let mut module = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &moonbuild_opt,
    )?;
    if cli.dry_run {
        bail!("dry-run is not implemented for generate-test-driver");
    }

    for (pkgname, pkg) in module.packages.iter_mut() {
        if let Some(ref package) = filter_package {
            if !package.contains(Path::new(pkgname)) {
                continue;
            }
        }

        if pkg.is_third_party {
            continue;
        }

        let mut testcase_internal = vec![];
        let mut testcase_underscore = vec![];
        let mut testcase_blackbox = vec![];
        let mut main_contain_test = false;

        for file in pkg
            .files
            .iter()
            .chain(pkg.test_files.iter())
            .chain(pkg.bbtest_files.iter())
        {
            let content = std::fs::read_to_string(file)?;
            let mut counter = 0;
            let pattern =
                Regex::new(r#"^test[[:blank:]]*("(?P<name>([^\\"]|\\.)*)")?.*$"#).unwrap();

            let filename = file.file_name().unwrap().to_str().unwrap();
            if let Some(ref filter_file) = filter_file {
                if filter_file != filename {
                    continue;
                }
            }

            for line in content.lines() {
                let escaped_filename = base16_encode_lower(filename.as_bytes());

                if let Some(captured) = pattern.captures(line) {
                    main_contain_test = true;
                    let test_func_name = format!("__test_{}_{}", escaped_filename, counter);

                    let description = if let Some(test_name) = captured.name("name") {
                        if test_name.is_empty() {
                            format!("{:?}", counter.to_string())
                        } else {
                            format!(r#""{}""#, test_name.as_str())
                        }
                    } else {
                        format!("{:?}", counter.to_string())
                    };

                    counter += 1;
                    if let Some(filter_index) = filter_index {
                        if (filter_index + 1) != counter {
                            continue;
                        }
                    }

                    let line = format!("({:?}, {}, {}),", filename, description, test_func_name);
                    let file_name = &file.file_stem().unwrap().to_str().unwrap();
                    if file_name.ends_with("_test") {
                        testcase_underscore.push(line);
                    } else if file_name.ends_with("_bbtest") {
                        testcase_blackbox.push(line);
                    } else {
                        testcase_internal.push(line);
                    }
                }
            }
        }
        if pkg.is_main {
            if main_contain_test {
                eprintln!(
                    "{}: tests in the main package `{}` will be ignored",
                    "Warning".yellow().bold(),
                    pkgname
                )
            }
            continue;
        }

        {
            let generated_content = generate_driver(&testcase_internal, pkgname);
            let generated_file = target_dir
                .join(pkg.rel.fs_full_name())
                .join("__generated_driver_for_internal_test.mbt");

            if !generated_file.parent().unwrap().exists() {
                std::fs::create_dir_all(generated_file.parent().unwrap())?;
            }
            std::fs::write(&generated_file, &generated_content)?;
        }

        {
            let generated_content = generate_driver(&testcase_underscore, pkgname);
            let generated_file = target_dir
                .join(pkg.rel.fs_full_name())
                .join("__generated_driver_for_underscore_test.mbt");

            if !generated_file.parent().unwrap().exists() {
                std::fs::create_dir_all(generated_file.parent().unwrap())?;
            }
            std::fs::write(&generated_file, &generated_content)?;
        }

        {
            let generated_content = generate_driver(&testcase_blackbox, pkgname);
            let generated_file = target_dir
                .join(pkg.rel.fs_full_name())
                .join("__generated_driver_for_blackbox_test.mbt");

            if !generated_file.parent().unwrap().exists() {
                std::fs::create_dir_all(generated_file.parent().unwrap())?;
            }
            std::fs::write(&generated_file, &generated_content)?;
        }
    }

    Ok(0)
}

fn generate_driver(lines: &[String], pkgname: &str) -> String {
    let test_driver_template = {
        let template = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/test_driver_template.mbt"
        ));
        if pkgname.starts_with(MOONBITLANG_CORE) {
            template.replace(&format!("@{}/builtin.", MOONBITLANG_CORE), "")
        } else {
            template.to_string()
        }
    };

    test_driver_template
        .replace("// test identifiers", &lines.join("\n    "))
        .replace("{package}", pkgname)
        .replace("{begin_moontest}", MOON_TEST_DELIMITER_BEGIN)
        .replace("{end_moontest}", MOON_TEST_DELIMITER_END)
}

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

#[test]
fn test_base16() {
    #[allow(unused)]
    use expect_test::{expect, Expect};

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
