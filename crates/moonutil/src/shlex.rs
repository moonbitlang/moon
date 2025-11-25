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

//! Cross platform shell lexer.
//!
//! In particular, this is intended to correctly handle Windows `CreateProcess`
//! quoting rules, which are different from Unix shells. Blame `n2`/`ninja` for
//! the difference.

pub fn split_unix(args: &str) -> Vec<String> {
    shlex::Shlex::new(args).collect()
}

pub fn get_argv0_unix(args: &str) -> String {
    let mut lexer = shlex::Shlex::new(args);
    lexer.next().unwrap_or_default()
}

pub fn join_unix<'a>(args: impl Iterator<Item = &'a str>) -> String {
    shlex::try_join(args).expect("Failed to join args with shlex, likely due to null bytes")
}

#[cfg(not(target_os = "windows"))]
pub use join_unix as join_native;
#[cfg(target_os = "windows")]
pub use join_windows as join_native;

#[cfg(not(target_os = "windows"))]
pub use get_argv0_unix as get_argv0_native;
#[cfg(target_os = "windows")]
pub use get_argv0_windows as get_argv0_native;

#[cfg(not(target_os = "windows"))]
pub use split_unix as split_native;
#[cfg(target_os = "windows")]
pub use split_windows as split_native;

/// Split the given command line according to Windows rules.
///
/// Reference:
/// https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments?view=msvc-170
pub fn split_windows(args: &str) -> Vec<String> {
    let (argv0, mut rest) = parse_windows_argv0(args);
    let mut result = vec![argv0];

    while let Some((arg, new_rest)) = next_windows_arg(rest) {
        result.push(arg);
        rest = new_rest;
    }

    result
}

pub fn get_argv0_windows(args: &str) -> String {
    let (argv0, _) = parse_windows_argv0(args);
    argv0
}

/// Split the argv[0] from the rest of the command line according to Windows rules.
pub fn split_argv0_windows(args: &str) -> (String, &str) {
    parse_windows_argv0(args)
}

fn parse_windows_argv0(args: &str) -> (String, &str) {
    let (quoted, rest) = if let Some(rest) = args.strip_prefix('"') {
        (true, rest)
    } else {
        (false, args)
    };
    for (i, c) in rest.char_indices() {
        // Exit condition
        if quoted && c == '"' {
            return (rest[..i].to_string(), &rest[i + 1..]);
        } else if !quoted && c.is_whitespace() {
            return (rest[..i].to_string(), &rest[i..]);
        }
    }
    (rest.to_string(), "")
}

fn next_windows_arg(mut args: &str) -> Option<(String, &str)> {
    args = args.trim_start();
    if args.is_empty() {
        return None;
    }

    /*
    Implement Microsoft argument parsing rules:
    - Quotes can start anywhere (embedded quoted strings).
    - Whitespace ends an argument only when not inside quotes.
    - Backslashes before a quote: pairs produce literal backslashes; if odd, the
      remaining backslash escapes the quote to a literal '"'.
    - Inside quotes, a pair of double quotes ("") yields a single literal '"'.
    - If input ends while still in quotes, everything accumulated becomes the final arg.
    */
    let rest = args;

    let mut acc = String::new();
    let mut backslash_count = 0;
    let mut in_quotes = false;

    // Work with byte indices for precise remainder slicing while peeking next char.
    let chars: Vec<(usize, char)> = rest.char_indices().collect();
    let mut idx = 0;

    while idx < chars.len() {
        let (byte_pos, c) = chars[idx];

        if c == '\\' {
            backslash_count += 1;
            idx += 1;
            continue;
        }

        if c == '"' {
            if backslash_count % 2 == 0 {
                // Emit one backslash per pair
                for _ in 0..(backslash_count / 2) {
                    acc.push('\\');
                }
                backslash_count = 0;

                if in_quotes {
                    // Pair of quotes inside quotes => literal quote, stay in quotes
                    if idx + 1 < chars.len() && chars[idx + 1].1 == '"' {
                        acc.push('"');
                        idx += 2;
                        continue;
                    } else {
                        // End quoted segment
                        in_quotes = false;
                        idx += 1;
                        continue;
                    }
                } else {
                    // Start quoted segment
                    in_quotes = true;
                    idx += 1;
                    continue;
                }
            } else {
                // Odd number of backslashes before quote => escape quote to literal
                for _ in 0..(backslash_count / 2) {
                    acc.push('\\');
                }
                acc.push('"');
                backslash_count = 0;
                idx += 1;
                continue;
            }
        }

        // Non-quote char: flush accumulated backslashes
        if backslash_count > 0 {
            for _ in 0..backslash_count {
                acc.push('\\');
            }
            backslash_count = 0;
        }

        if !in_quotes && c.is_whitespace() {
            // End of argument; remainder starts at current byte position
            return Some((acc, &rest[byte_pos..]));
        } else {
            acc.push(c);
            idx += 1;
        }
    }

    // End of input: flush trailing backslashes and return final arg
    if backslash_count > 0 {
        for _ in 0..backslash_count {
            acc.push('\\');
        }
    }

    Some((acc, ""))
}

/// Join the given arguments into a command line according to Windows rules.
///    
/// Reference:
/// https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments
pub fn join_windows<'a>(args: impl Iterator<Item = &'a str>) -> String {
    let mut result = String::new();
    for arg in args {
        if !result.is_empty() {
            result.push(' ');
        }

        // Determine if quoting is needed
        let needs_quotes = arg.chars().any(|c| c.is_whitespace()) || arg.is_empty();
        let mut backslash_count = 0;
        if needs_quotes {
            result.push('"');
        }
        // Iterate through characters
        for c in arg.chars() {
            if c == '\\' {
                // Accumulate backslashes
                backslash_count += 1;
            } else if c == '"' {
                // Escape all backslashes
                // Backslashes need to be doubled before a quote
                for _ in 0..(backslash_count * 2) {
                    result.push('\\');
                }
                backslash_count = 0;
                // Escape the quote
                result.push('\\');
                result.push('"');
            } else {
                // Push accumulated backslashes
                for _ in 0..backslash_count {
                    result.push('\\');
                }
                backslash_count = 0;
                result.push(c);
            }
        }
        // Escape trailing backslashes
        if needs_quotes {
            for _ in 0..(backslash_count * 2) {
                result.push('\\');
            }
            result.push('"');
        } else {
            for _ in 0..backslash_count {
                result.push('\\');
            }
        }
    }

    result
}

#[cfg(test)]
mod tests;
