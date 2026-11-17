// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use super::parse_windows_argv0;
#[cfg(not(target_os = "windows"))]
use super::split_native_args;
use super::{join_windows, split_windows, split_windows_args};

fn assert_parse_tail(tail: &str, expected: &[&str]) {
    let got = split_windows_args(tail);
    let expected_vec: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    assert_eq!(got, expected_vec, "argument mismatch for input: {}", tail);
}

#[cfg(not(target_os = "windows"))]
#[test]
fn native_argument_tail_rejects_unterminated_quote() {
    assert_eq!(split_native_args(r#"a "unterminated"#), None);
    assert_eq!(split_native_args("a \\"), None);
}

// MS official doc examples
#[test]
fn ms_example_1() {
    assert_parse_tail(r#""a b c" d e"#, &["a b c", "d", "e"]);
}

#[test]
fn ms_example_2() {
    assert_parse_tail(r#""ab\"c" "\\" d"#, &["ab\"c", "\\", "d"]);
}

#[test]
fn ms_example_3() {
    assert_parse_tail(r#"a\\\b d"e f"g h"#, &[r"a\\\b", "de fg", "h"]);
}

#[test]
fn ms_example_4() {
    assert_parse_tail(r#"a\\\"b c d"#, &[r#"a\"b"#, "c", "d"]);
}

#[test]
fn ms_example_5() {
    assert_parse_tail(r#"a\\\\"b c" d e"#, &[r"a\\b c", "d", "e"]);
}

#[test]
fn ms_example_6() {
    assert_parse_tail(r#"a"b"" c d"#, &[r#"ab" c d"#]);
}

// Additional parser tests
#[test]
fn argv0_parsing() {
    let (argv0, rest) = parse_windows_argv0(r#""C:\Program Files\app.exe" a b"#);
    assert_eq!(argv0, r#"C:\Program Files\app.exe"#);
    assert_eq!(rest, " a b");

    let (argv0, rest) = parse_windows_argv0(r#"C:\app.exe a b"#);
    assert_eq!(argv0, r#"C:\app.exe"#);
    assert_eq!(rest, " a b");

    let (argv0, rest) = parse_windows_argv0(r#"C:\app.exe"#);
    assert_eq!(argv0, r#"C:\app.exe"#);
    assert_eq!(rest, "");

    let (argv0, rest) = parse_windows_argv0(r#""moon"x user/module"#);
    assert_eq!(argv0, "moonx");
    assert_eq!(rest, " user/module");

    let (argv0, rest) = parse_windows_argv0(r#"mo"onx" user/module"#);
    assert_eq!(argv0, "moonx");
    assert_eq!(rest, " user/module");
}

#[test]
fn embedded_double_quotes_pair_inside_quotes() {
    // Within quotes, "" yields a literal "
    assert_parse_tail(r#""a""b" c"#, &[r#"a"b"#, "c"]);
}

#[test]
fn whitespace_and_empty_args() {
    assert_parse_tail("   a   b  ", &["a", "b"]);
    assert_parse_tail(r#""""#, &[""]);
    assert_parse_tail(r#""a b" "" "#, &["a b", ""]);
}

#[test]
fn unterminated_quote_last_arg() {
    // If input ends before a closing quote, accumulated chars become the last argument.
    assert_parse_tail(r#""unterminated"#, &["unterminated"]);
}

// Roundtrip tests for join_windows/split_windows
fn roundtrip(args: &[&str]) {
    let cmd = format!("prog {}", join_windows(args.iter().cloned()));
    let parsed = split_windows(&cmd);
    assert_eq!(&parsed[1..], args);
}

#[test]
fn roundtrip_no_whitespace() {
    roundtrip(&["a", r#"a\"b"#, r#"a\\\"b"#, r#"a\\\\"#, "b"]);
}

#[test]
fn roundtrip_with_spaces_and_empty() {
    roundtrip(&["", "a b", "de fg", r#"ab" c"#]);
    roundtrip(&[r#"abc\ def\"#, "x y", ""]);
    roundtrip(&[r#"trailing\\"#, "has space"]);
}

#[test]
fn backslashes_before_quote_and_trailing() {
    // Backslashes before a quote: even -> delimiter (pairs into literal \), odd -> escapes quote to literal '"'
    assert_parse_tail(r#"a\\\"b "x""#, &[r#"a\"b"#, r#"x"#]);

    // Trailing backslashes inside quoted arg must be doubled in the command line and restored by split
    let args = [r#"x\ y\\"#, r#"z\\ "#, r#"w"#];
    roundtrip(&args);
}
