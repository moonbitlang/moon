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

pub fn run_wat(path: &Path, target_dir: &Path) -> anyhow::Result<TestStatistics> {
    run("moonrun", path, target_dir)
}

pub fn run_js(path: &Path, target_dir: &Path) -> anyhow::Result<TestStatistics> {
    run("node", path, target_dir)
}

fn run(command: &str, path: &Path, target_dir: &Path) -> anyhow::Result<TestStatistics> {
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

    handle_stdout(
        &mut std::io::BufReader::new(stdout),
        &mut [&mut test_capture, &mut coverage_capture],
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
        if let Some(test_output) = test_capture.finish() {
            let j: TestStatistics = serde_json_lenient::from_str(test_output.trim())
                .context(format!("failed to parse test summary: {}", test_output))?;
            Ok(j)
        } else {
            bail!("No test output found");
        }
    } else {
        bail!("Failed to run the test");
    }
}

#[test]
fn test_handle_output() {
    let out = "abcde
---begin---
text
---end---
fghij";
    let expected_out = "abcde
fghij";
    let mut capture = SectionCapture::new("---begin---", "---end---", false);
    let mut buf = String::new();
    let mut captures = [&mut capture];
    handle_stdout(
        &mut std::io::BufReader::new(out.as_bytes()),
        &mut captures,
        |line| buf.push_str(line),
    )
    .unwrap();
    assert_eq!(buf, expected_out);
    assert_eq!(capture.finish().unwrap(), "text\n");
}

#[test]
fn test_handle_intermixed_output() {
    let out = "abcde
blahblah---begin---
text
---end---blahblah
fghij";
    let expected_out = "abcde
blahblahblahblah
fghij";
    let mut capture = SectionCapture::new("---begin---", "---end---", false);
    let mut buf = String::new();
    let mut captures = [&mut capture];
    handle_stdout(
        &mut std::io::BufReader::new(out.as_bytes()),
        &mut captures,
        |line| buf.push_str(line),
    )
    .unwrap();
    assert_eq!(buf, expected_out);
    assert_eq!(capture.finish().unwrap(), "text\n");
}
#[test]
fn test_handle_wrong_output() {
    let out = "abcde
blahblah---begin--
text
---end---blahblah
fghij";

    let mut capture = SectionCapture::new("---begin---", "---end---", false);
    let mut buf = String::new();
    let mut captures = [&mut capture];
    handle_stdout(
        &mut std::io::BufReader::new(out.as_bytes()),
        &mut captures,
        |line| buf.push_str(line),
    )
    .unwrap();
    assert_eq!(buf, out);
    assert!(capture.finish().is_none());
}
