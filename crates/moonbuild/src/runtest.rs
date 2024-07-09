use crate::expect::{
    apply_snapshot, expect_failed_to_snapshot_result, parse_filename, ExpectFailedRaw,
};
use crate::section_capture::{self, SectionCapture};

use super::gen;
use anyhow::{bail, Context};
use moonutil::common::{
    MoonbuildOpt, MooncOpt, MOON_COVERAGE_DELIMITER_BEGIN, MOON_COVERAGE_DELIMITER_END,
    MOON_SNAPSHOT_DELIMITER_BEGIN, MOON_SNAPSHOT_DELIMITER_END, MOON_TEST_DELIMITER_BEGIN,
    MOON_TEST_DELIMITER_END,
};
use moonutil::module::ModuleDB;
use n2::load::State;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::{path::Path, process::Stdio};

pub fn load_moon_proj(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    // let module = module.clone();
    let n2_input = gen::gen_runtest::gen_runtest(module, moonc_opt, moonbuild_opt)?;
    log::debug!("n2_input: {:#?}", n2_input);
    gen::gen_runtest::gen_n2_runtest_state(&n2_input, moonc_opt, moonbuild_opt)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestStatistics {
    pub passed: u32,
    pub package: String,
    pub filenames: Vec<String>,
    pub messages: Vec<String>,
    pub test_names: Vec<String>,
}

pub fn run_wat(
    path: &Path,
    target_dir: &Path,
    auto_update: bool,
) -> anyhow::Result<TestStatistics> {
    run("moonrun", path, target_dir, auto_update)
}

pub fn run_js(path: &Path, target_dir: &Path, auto_update: bool) -> anyhow::Result<TestStatistics> {
    run("node", path, target_dir, auto_update)
}

fn run(
    command: &str,
    path: &Path,
    target_dir: &Path,
    auto_update: bool,
) -> anyhow::Result<TestStatistics> {
    let mut execution = Command::new(command)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let stdout = execution.stdout.take().unwrap();

    let mut test_capture =
        SectionCapture::new(MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END, false);
    let mut coverage_capture = SectionCapture::new(
        MOON_COVERAGE_DELIMITER_BEGIN,
        MOON_COVERAGE_DELIMITER_END,
        true,
    );
    let mut snapshot_capture = SectionCapture::new(
        MOON_SNAPSHOT_DELIMITER_BEGIN,
        MOON_SNAPSHOT_DELIMITER_END,
        false,
    );

    section_capture::handle_stdout(
        &mut std::io::BufReader::new(stdout),
        &mut [
            &mut test_capture,
            &mut coverage_capture,
            &mut snapshot_capture,
        ],
        |line| print!("{}", line),
    )?;
    let output = execution.wait()?;

    if output.success() {
        if let Some(coverage_output) = coverage_capture.finish() {
            // Output to moonbit_coverage_<time>.txt
            // TODO: do we need to move this out of the runtest module?
            let time = chrono::Local::now().timestamp_micros();
            let filename = target_dir.join(format!("moonbit_coverage_{}.txt", time));
            std::fs::write(&filename, coverage_output)
                .context(format!("failed to write {}", filename.to_string_lossy()))?;
        }
        let snapshots = if let Some(snapshot_testing_output) = snapshot_capture.finish() {
            let mut xs = vec![];
            for line in snapshot_testing_output.lines() {
                let t: crate::expect::ExpectFailedRaw = serde_json_lenient::from_str(line)
                    .context(format!("failed to parse snapshot testing output: {}", line))?;
                // apply_snapshot(t, auto_update)?;
                xs.push(expect_failed_to_snapshot_result(t));
            }
            xs
        } else {
            vec![]
        };

        if let Some(test_output) = test_capture.finish() {
            let j: TestStatistics = serde_json_lenient::from_str(test_output.trim())
                .context(format!("failed to parse test summary: {}", test_output))?;
            let j = if !snapshots.is_empty() {
                let mut j = j;
                let mut index = j.filenames.len() - j.passed as usize;
                for snap in snapshots.iter() {
                    let expect_failed = ExpectFailedRaw {
                        loc: snap.loc.clone(),
                        args_loc: snap.args_loc.clone(),
                        expect: snap.expect_file.display().to_string(),
                        actual: snap.actual.clone(),
                        snapshot: Some(true),
                    };

                    if snap.succ {
                        j.messages.push("".to_string());
                        let filename = parse_filename(&snap.loc)?;
                        j.filenames.push(filename);
                        j.test_names.push("snapshot".to_string());
                        j.passed += 1;
                    } else {
                        j.messages.insert(
                            index,
                            format!(
                                "@SNAPSHOT_TESTING {}",
                                serde_json_lenient::to_string(&expect_failed)?
                            ),
                        );
                        let filename = parse_filename(&snap.loc)?;
                        j.filenames.insert(index, filename);
                        j.test_names.insert(index, "snapshot".to_string());
                        index += 1;
                    }
                }
                j
            } else {
                j
            };

            Ok(j)
        } else {
            bail!("No test output found");
        }
    } else {
        bail!("Failed to run the test");
    }
}
