use super::gen;
use anyhow::{bail, Context};
use moonutil::common::{
    MoonbuildOpt, MooncOpt, MOON_COVERAGE_DELIMITER_BEGIN, MOON_COVERAGE_DELIMITER_END,
    MOON_TEST_DELIMITER_BEGIN, MOON_TEST_DELIMITER_END,
};
use moonutil::module::ModuleDB;
use n2::load::State;
use serde::{Deserialize, Serialize};
use std::io::BufRead;
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

struct SectionCapture<'a> {
    begin_delimiter: &'a str,
    end_delimiter: &'a str,
    capture_buffer: String,
    include_delimiters: bool,
    found_begin: bool,
    found_end: bool,
}

enum LineCaptured {
    All,
    Prefix(usize), // noncaptured start index
    Suffix(usize), // noncaptured end index
}

impl<'a> SectionCapture<'a> {
    pub fn new(begin_delimiter: &'a str, end_delimiter: &'a str, include_delimiters: bool) -> Self {
        SectionCapture {
            begin_delimiter,
            end_delimiter,
            capture_buffer: String::new(),
            include_delimiters,
            found_begin: false,
            found_end: false,
        }
    }

    /// Feed a line into the capture buffer. The line should contain the newline character.
    pub fn feed_line(&mut self, line: &str) -> Option<LineCaptured> {
        if line.trim_end().ends_with(self.begin_delimiter) {
            self.found_begin = true;
            self.found_end = false;
            if self.include_delimiters {
                self.capture_buffer.push_str(line);
            }
            let end_index = line.trim_end().len() - self.begin_delimiter.len();
            return Some(LineCaptured::Suffix(end_index));
        }
        if self.found_begin && line.starts_with(self.end_delimiter) {
            self.found_end = true;
            if self.include_delimiters {
                self.capture_buffer.push_str(line);
            }
            let start_index = self.end_delimiter.len();
            if line.trim().len() == self.end_delimiter.len() {
                // The whole line is just the end delimiter.
                return Some(LineCaptured::All);
            } else {
                return Some(LineCaptured::Prefix(start_index));
            }
        }
        if self.found_begin && !self.found_end {
            self.capture_buffer.push_str(line);
            return Some(LineCaptured::All);
        }
        None
    }

    /// Returns the captured section if the section is complete.
    pub fn finish(self) -> Option<String> {
        if self.found_begin && self.found_end {
            Some(self.capture_buffer)
        } else {
            None
        }
    }
}

/// Pipes the child stdout to stdout, with the ability to capture sections of the output.
/// If any of the captures captures a line, the line will not be printed to stdout.
/// The captures are processed in order: a line is captured by the first capture that captures it.
/// The program should not print overlapping sections, as the captures are not aware of each other.
fn handle_stdout<P: FnMut(&str)>(
    stdout: &mut impl BufRead,
    captures: &mut [&mut SectionCapture],
    mut print: P,
) -> anyhow::Result<()> {
    let mut buf = String::new();

    loop {
        buf.clear();
        let n = stdout.read_line(&mut buf)?;
        if n == 0 {
            break;
        }
        let capture_status = captures
            .iter_mut()
            .find_map(|capture| capture.feed_line(&buf));
        match capture_status {
            None => print(&buf),
            Some(LineCaptured::All) => {}
            Some(LineCaptured::Prefix(start_index)) => print(&buf[start_index..]),
            Some(LineCaptured::Suffix(end_index)) => print(&buf[..end_index]),
        }
    }
    Ok(())
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
