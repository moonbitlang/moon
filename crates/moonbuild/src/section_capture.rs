use std::io::BufRead;

pub struct SectionCapture<'a> {
    begin_delimiter: &'a str,
    end_delimiter: &'a str,
    capture_buffer: String,
    include_delimiters: bool,
    found_begin: bool,
    found_end: bool,
}

pub enum LineCaptured {
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
pub fn handle_stdout<P: FnMut(&str)>(
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
