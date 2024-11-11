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

use crate::entry::{FileTestInfo, TestArgs, TestFailedStatus};
use crate::expect::{snapshot_eq, ERROR, EXPECT_FAILED, FAILED, RUNTIME_ERROR, SNAPSHOT_TESTING};
use crate::section_capture::{handle_stdout, SectionCapture};

use super::gen;
use anyhow::{bail, Context};
use moonutil::common::{
    MoonbuildOpt, MooncOpt, MOON_COVERAGE_DELIMITER_BEGIN, MOON_COVERAGE_DELIMITER_END,
    MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END,
};
use moonutil::module::ModuleDB;
use n2::load::State;
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio};
use tokio::io::AsyncReadExt;

pub fn load_moon_proj(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let n2_input = gen::gen_runtest::gen_runtest(module, moonc_opt, moonbuild_opt)?;
    log::debug!("n2_input: {:#?}", n2_input);
    gen::gen_runtest::gen_n2_runtest_state(&n2_input, moonc_opt, moonbuild_opt)
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct TestStatistics {
    pub package: String,
    pub filename: String,
    pub index: String,
    pub test_name: String,
    pub message: String,
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
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    // put "--test-mode" at the front of args
    let mut _args = vec!["--test-mode".to_string()];
    _args.push(serde_json_lenient::to_string(args).unwrap());
    run(
        "moonrun",
        path,
        target_dir,
        &_args,
        file_test_info_map,
        verbose,
    )
    .await
}

pub async fn run_js(
    path: &Path,
    target_dir: &Path,
    args: &TestArgs,
    file_test_info_map: &FileTestInfo,
    verbose: bool,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    run(
        "node",
        path,
        target_dir,
        &[serde_json_lenient::to_string(args).unwrap()],
        file_test_info_map,
        verbose,
    )
    .await
}

pub async fn run_native(
    path: &Path,
    target_dir: &Path,
    args: &TestArgs,
    file_test_info_map: &FileTestInfo,
    verbose: bool,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    run(
        path.to_str().unwrap(),
        path,
        target_dir,
        &[serde_json_lenient::to_string(args).unwrap()],
        file_test_info_map,
        verbose,
    )
    .await
}

async fn run(
    command: &str,
    path: &Path,
    target_dir: &Path,
    args: &[String],
    file_test_info_map: &FileTestInfo,
    verbose: bool,
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    if verbose {
        eprintln!("{} {} {}", command, path.display(), args.join(" "));
    }
    let mut execution = tokio::process::Command::new(command)
        .arg(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to execute '{} {}'", command, path.display()))?;
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
        .context(format!(
            "failed to read stdout for {} {} {}",
            command,
            path.display(),
            args.join(" ")
        ))?;

    handle_stdout(
        &mut std::io::BufReader::new(stdout_buffer.as_slice()),
        &mut [&mut test_capture, &mut coverage_capture],
        |line| print!("{}", line),
    )?;
    let output = execution.wait().await?;

    if !output.success() {
        bail!(format!("Failed to run the test: {}", path.display()));
    }
    if let Some(coverage_output) = coverage_capture.finish() {
        // Output to moonbit_coverage_<time>.txt
        // TODO: do we need to move this out of the runtest module?
        let time = chrono::Local::now().timestamp_micros();
        let filename = target_dir.join(format!("moonbit_coverage_{}.txt", time));
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
            let a = serde_json_lenient::from_str(s.trim())
                .context(format!("failed to parse test summary: {}", s))?;
            test_statistics.push(a);
        }

        for mut test_statistic in test_statistics {
            let filename = &test_statistic.filename;
            let index = &test_statistic.index.parse::<u32>().unwrap();
            let test_name = file_test_info_map
                .get(filename)
                .and_then(|m| m.get(index))
                .and_then(|s| s.as_ref())
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
                let ok = snapshot_eq(&test_statistic.message)?;
                if ok {
                    res.push(Ok(test_statistic));
                } else {
                    res.push(Err(TestFailedStatus::SnapshotPending(test_statistic)));
                }
            } else if return_message.starts_with(RUNTIME_ERROR) || return_message.starts_with(ERROR)
            {
                res.push(Err(TestFailedStatus::RuntimeError(test_statistic)));
            } else if return_message.starts_with(FAILED) || !return_message.is_empty() {
                // FAILED(moonbit) or something like "panic is expected"
                res.push(Err(TestFailedStatus::Failed(test_statistic)));
            } else {
                res.push(Err(TestFailedStatus::Others(return_message.to_string())));
            }
        }
    } else {
        res.push(Err(TestFailedStatus::Others(String::from(
            "No test output found",
        ))));
    }

    Ok(res)
}
