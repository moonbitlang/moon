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

use crate::entry::{TestFailedStatus, TestResult};
use crate::expect::{EXPECT_FAILED, FAILED};
use crate::section_capture::{handle_stdout, SectionCapture};

use super::gen;
use anyhow::{anyhow, bail, Context};
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
    args: &[String],
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    run("mmr", path, target_dir, args).await
}

pub async fn run_js(
    path: &Path,
    target_dir: &Path,
    args: &[String],
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    run("node", path, target_dir, args).await
}

async fn run(
    command: &str,
    path: &Path,
    target_dir: &Path,
    args: &[String],
) -> anyhow::Result<Vec<Result<TestStatistics, TestFailedStatus>>> {
    let mut execution = tokio::process::Command::new(command)
        .arg(path)
        .args(args)
        .arg("--test")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute '{} {}'", command, path.display()))?;
    let mut stdout = execution.stdout.take().unwrap();
    let mut stderr = execution.stderr.take().unwrap();

    let mut test_capture =
        SectionCapture::new(MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END, false);
    let mut coverage_capture = SectionCapture::new(
        MOON_COVERAGE_DELIMITER_BEGIN,
        MOON_COVERAGE_DELIMITER_END,
        true,
    );

    let mut stdout_buffer = Vec::new();
    stdout.read_to_end(&mut stdout_buffer).await.context(format!(
        "failed to read stdout for {} {}",
        command,
        path.display()
    ))?;

    let mut stderr_buffer = Vec::new();
    stderr.read_to_end(&mut stderr_buffer).await.context(format!(
        "failed to read stdout for {} {}",
        command,
        path.display()
    ))?;

    // let s = String::from_utf8_lossy(&stdout_buffer).to_string();
    // s.split("----- END MOON TEST RESULT -----")
    // .for_each(|it| {
    //     println!("it: {}", it);
    // });

    handle_stdout(
        &mut std::io::BufReader::new(stdout_buffer.as_slice()),
        &mut [&mut test_capture, &mut coverage_capture],
        |line| print!("{}", line),
    )?;
    let output = execution.wait().await?;

    if !output.success() {
        println!("exec failed");
        // return Err(TestFailedStatus::RuntimeError(TestStatistics::default()))
    }

    let mut res = vec![];

    if let Some(test_output) = test_capture.finish() {
        let mut test_statistics: Vec<TestStatistics> = vec![];
        for s in test_output.split('\n') {
            if s == "" {
                continue;
            }
            let a = serde_json_lenient::from_str(s.trim())
                .context(format!("failed to parse test summary: {}", s))?;
            test_statistics.push(a);
        }

        for test_statistic in test_statistics {
            let return_message = &test_statistic.message;
            if return_message.starts_with(EXPECT_FAILED) {
                res.push(Err(TestFailedStatus::ExpectTestFailed(test_statistic)));
            } else if return_message.starts_with(FAILED) {
                res.push(Err(TestFailedStatus::Failed(test_statistic)));
            } else {
                res.push(Ok(test_statistic));
            }
        }
    } else {
        let s = String::from_utf8_lossy(&stdout_buffer).to_string();
        println!("stdout: {}", s);
        let s = String::from_utf8_lossy(&stderr_buffer).to_string();
        println!("stderr: {}", s);
        res.push(Err(TestFailedStatus::Others(anyhow!(
            "No test output found"
        ))));
    }

    Ok(res)

    // if output.success() {
    //     Ok(0)
    // } else {
    //     Err(TestFailedStatus::Others(anyhow!("Failed to run the test")))
    // }

    // if output.success() {
    //     if let Some(coverage_output) = coverage_capture.finish() {
    //         // Output to moonbit_coverage_<time>.txt
    //         // TODO: do we need to move this out of the runtest module?
    //         let time = chrono::Local::now().timestamp_micros();
    //         let filename = target_dir.join(format!("moonbit_coverage_{}.txt", time));
    //         std::fs::write(&filename, coverage_output)
    //             .context(format!("failed to write {}", filename.to_string_lossy()))?;
    //     }
    //     if let Some(test_output) = test_capture.finish() {
    //         let j: TestStatistics = serde_json_lenient::from_str(test_output.trim())
    //             .context(format!("failed to parse test summary: {}", test_output))?;
    //         Ok(j)
    //     } else {
    //         bail!("No test output found");
    //     }
    // } else {
    //     bail!("Failed to run the test");
    // }
}
