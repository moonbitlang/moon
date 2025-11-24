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

use super::parse_windows_argv0;
use super::{join_windows, split_windows};

fn assert_parse_tail(tail: &str, expected: &[&str]) {
    let input = format!("program.exe {}", tail);
    let got = split_windows(&input);
    assert_eq!(
        got.first().unwrap(),
        "program.exe",
        "argv[0] mismatch for input: {}",
        tail
    );
    let expected_vec: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        &got[1..],
        expected_vec.as_slice(),
        "argv[1..] mismatch for input: {}",
        tail
    );
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
