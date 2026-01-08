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

use crate::benchmark::BATCHBENCH;
use crate::entry::{FileTestInfo, TestArgs, TestFailedStatus};
use crate::expect::{ERROR, EXPECT_FAILED, FAILED, RUNTIME_ERROR, SNAPSHOT_TESTING, snapshot_eq};
use crate::section_capture::{SectionCapture, handle_stdout};

use super::r#gen;
use anyhow::{Context, bail};
use moonutil::common::{
    DYN_EXT, MOON_COVERAGE_DELIMITER_BEGIN, MOON_COVERAGE_DELIMITER_END, MOON_TEST_DELIMITER_BEGIN,
    MOON_TEST_DELIMITER_END, MoonbuildOpt, MooncOpt,
};
use moonutil::module::ModuleDB;
use moonutil::moon_dir::MOON_DIRS;
use moonutil::platform::macos_with_sigchild_blocked;
use n2::load::State;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::{path::Path, process::Stdio};
use tokio::io::AsyncReadExt;

pub fn load_moon_proj(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let n2_input = r#gen::gen_runtest::gen_runtest(module, moonc_opt, moonbuild_opt)?;
    log::debug!("n2_input: {:#?}", n2_input);
    r#gen::gen_runtest::gen_n2_runtest_state(&n2_input, moonc_opt, moonbuild_opt)
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct TestStatistics {
    pub package: String,
    pub filename: String,
    pub index: String,
    pub test_name: String,
    pub message: String,
}

/// Convert MoonBit-style unicode escapes `\u{XX}` to JSON-style `\uXXXX`.
///
/// The test driver templates (see `template/test_driver/test_driver_template.mbt`)
/// use MoonBit's `String::escape()` method to escape the message field before
/// outputting JSON:
///
/// ```moonbit
/// let message = message.escape()
/// println("{\"package\": \"...\", \"message\": \{message}}")
/// ```
///
/// However, MoonBit's `escape()` uses `\u{XX}` syntax for unicode escapes,
/// which is invalid JSON. JSON requires exactly 4 hex digits: `\uXXXX`.
///
/// This function converts MoonBit-style escapes to valid JSON escapes so that
/// `serde_json` can parse the test output correctly.
fn fix_moonbit_unicode_escapes(s: &str) -> Cow<'_, str> {
    if !s.contains("\\u{") {
        return Cow::Borrowed(s);
    }

    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'u') {
            chars.next(); // consume 'u'
            if chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let mut hex = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '}' {
                        chars.next(); // consume '}'
                        break;
                    }
                    if ch.is_ascii_hexdigit() {
                        hex.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                if let Ok(codepoint) = u32::from_str_radix(&hex, 16) {
                    if codepoint <= 0xFFFF {
                        result.push_str(&format!("\\u{:04X}", codepoint));
                    } else {
                        // Supplementary character: use surrogate pair
                        let adjusted = codepoint - 0x10000;
                        let high = 0xD800 + (adjusted >> 10);
                        let low = 0xDC00 + (adjusted & 0x3FF);
                        result.push_str(&format!("\\u{:04X}\\u{:04X}", high, low));
                    }
                } else {
                    // Invalid hex, keep original
                    result.push_str("\\u{");
                    result.push_str(&hex);
                    result.push('}');
                }
            } else {
                result.push_str("\\u");
            }
        } else {
            result.push(c);
        }
    }

    Cow::Owned(result)
}

impl std::fmt::Display for TestStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}::{}::test#{}, message: {}",
            self.package, self.filename, self.test_name, self.message
        )
    }
}

pub async fn run_wat(
    path: &Path,
    target_dir: &Path,
    args: &TestArgs,
    file_test_info_map: &FileTestInfo,
    verbose: bool,
    module: &ModuleDB,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    let mut cmd = tokio::process::Command::new(&*moonutil::BINARIES.moonrun);
    cmd.arg(path)
        .arg("--test-args")
        .arg(serde_json_lenient::to_string(args).expect("valid JSON"));
    run(path, cmd, target_dir, file_test_info_map, verbose, module).await
}

pub async fn run_js(
    path: &Path,
    target_dir: &Path,
    args: &TestArgs,
    file_test_info_map: &FileTestInfo,
    verbose: bool,
    module: &ModuleDB,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    let mut cmd = tokio::process::Command::new(
        moonutil::BINARIES
            .node
            .as_deref()
            .context("Unable to find the `node` executable in PATH")?,
    );
    cmd.arg("--enable-source-maps")
        .arg(path)
        .arg(serde_json_lenient::to_string(args).expect("valid JSON"));
    run(path, cmd, target_dir, file_test_info_map, verbose, module).await
}

pub async fn run_native(
    moonbuild_opt: &MoonbuildOpt,
    path: &Path,
    target_dir: &Path,
    args: &TestArgs,
    file_test_info_map: &FileTestInfo,
    verbose: bool,
    module: &ModuleDB,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    let args = args.to_cli_args_for_native();
    let cmd = if moonbuild_opt.use_tcc_run {
        let path = path.with_extension("c");
        // TODO
        let mut cmd = tokio::process::Command::new(&MOON_DIRS.internal_tcc_path);
        cmd.arg(format!("-I{}", MOON_DIRS.moon_include_path.display()))
            .arg(format!("-L{}", MOON_DIRS.moon_lib_path.display()))
            .arg(target_dir.join(format!("libruntime.{DYN_EXT}")))
            .args(moonbuild_opt.dynamic_stub_libs.iter().flatten())
            .arg("-DMOONBIT_NATIVE_NO_SYS_HEADER")
            .arg("-DMOONBIT_USE_SHARED_RUNTIME")
            .arg("-run")
            .arg(path)
            .arg(args);
        cmd
    } else {
        let mut cmd = tokio::process::Command::new(path);
        cmd.arg(args);
        cmd
    };
    run(path, cmd, target_dir, file_test_info_map, verbose, module).await
}

pub async fn run_llvm(
    path: &Path,
    target_dir: &Path,
    args: &TestArgs,
    file_test_info_map: &FileTestInfo,
    verbose: bool,
    module: &ModuleDB,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    let mut cmd = tokio::process::Command::new(path);
    cmd.arg(args.to_cli_args_for_native());
    run(path, cmd, target_dir, file_test_info_map, verbose, module).await
}

async fn run(
    path: &Path,
    mut subprocess: tokio::process::Command,
    target_dir: &Path,
    file_test_info_map: &FileTestInfo,
    verbose: bool,
    module: &ModuleDB,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    if verbose {
        eprintln!("{:?}", subprocess.as_std());
    }

    let mut execution = macos_with_sigchild_blocked(|| {
        subprocess
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to execute: {:?}", subprocess))
    })?;
    let mut stdout = execution.stdout.take().unwrap();

    let mut test_capture =
        SectionCapture::new(MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END, false);
    let mut coverage_capture = SectionCapture::new(
        MOON_COVERAGE_DELIMITER_BEGIN,
        MOON_COVERAGE_DELIMITER_END,
        true,
    );

    let mut stdout_buffer = Vec::new();
    stdout
        .read_to_end(&mut stdout_buffer)
        .await
        .with_context(|| format!("failed to read stdout: {:?}", subprocess))?;

    handle_stdout(
        &mut std::io::BufReader::new(stdout_buffer.as_slice()),
        &mut [&mut test_capture, &mut coverage_capture],
        |line| print!("{line}"),
    )?;
    std::io::Write::flush(&mut std::io::stdout())?;
    let output = execution.wait().await?;

    if !output.success() {
        bail!(format!(
            "Failed to run the test: {}\nThe test executable exited with {}",
            path.display(),
            output
        ));
    }
    if let Some(coverage_output) = coverage_capture.finish() {
        // Output to moonbit_coverage_<time>.txt
        // TODO: do we need to move this out of the runtest module?
        let time = chrono::Local::now().timestamp_micros();
        let rnd = rand::random::<u32>();
        let filename = target_dir.join(format!("moonbit_coverage_{time}_{rnd:08x}.txt"));
        std::fs::write(&filename, coverage_output)
            .context(format!("failed to write {}", filename.to_string_lossy()))?;
    }

    let mut res = vec![];
    if let Some(test_output) = test_capture.finish() {
        let mut test_statistics: Vec<TestStatistics> = vec![];
        for s in test_output.split('\n') {
            if s.is_empty() {
                continue;
            }
            // Fix MoonBit-style \u{XX} escapes to JSON-style \uXXXX
            let fixed = fix_moonbit_unicode_escapes(s.trim());
            let ts: TestStatistics = serde_json_lenient::from_str(&fixed)
                .context(format!("failed to parse test summary: {s}"))?;

            if ts.message == "skipped test" {
                continue;
            }

            test_statistics.push(ts);
        }

        for mut test_statistic in test_statistics {
            let filename = &test_statistic.filename;
            let index = &test_statistic.index.parse::<u32>().unwrap();
            let test_name = file_test_info_map
                .get(filename)
                .and_then(|m| m.get(index))
                .and_then(|s| s.name.as_ref())
                .unwrap_or(&test_statistic.index);

            if test_name.starts_with("panic") {
                // should panic but not panic
                if test_statistic.message.is_empty() {
                    test_statistic.message = "panic is expected".to_string();
                }
                // it does panic, treat it as ok
                else {
                    test_statistic.message = "".to_string();
                }
            }

            test_statistic.test_name = test_name.clone();

            let return_message = test_statistic.message.clone();
            if return_message.is_empty() {
                res.push(Ok(test_statistic));
            } else if return_message.starts_with(EXPECT_FAILED) {
                res.push(Err(TestFailedStatus::ExpectTestFailed(test_statistic)));
            } else if return_message.starts_with(SNAPSHOT_TESTING) {
                let ok = snapshot_eq(module, &test_statistic.message)?;
                if ok {
                    res.push(Ok(test_statistic));
                } else {
                    res.push(Err(TestFailedStatus::SnapshotPending(test_statistic)));
                }
            } else if return_message.starts_with(RUNTIME_ERROR) || return_message.starts_with(ERROR)
            {
                res.push(Err(TestFailedStatus::RuntimeError(test_statistic)));
            } else if return_message.starts_with(BATCHBENCH) {
                res.push(Ok(test_statistic));
            } else if return_message.starts_with(FAILED) || !return_message.is_empty() {
                // FAILED(moonbit) or something like "panic is expected"
                res.push(Err(TestFailedStatus::Failed(test_statistic)));
            } else {
                res.push(Err(TestFailedStatus::Others(return_message.to_string())));
            }
        }
    }

    Ok(res)
}