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

/// Split an argument tail according to native quoting rules, reporting
/// malformed quoting or escaping when the platform grammar supports it.
#[cfg(not(target_os = "windows"))]
pub fn split_native_args(args: &str) -> Option<Vec<String>> {
    shlex::split(args)
}

/// Split an argument tail according to native quoting rules, reporting
/// malformed quoting or escaping when the platform grammar supports it.
#[cfg(target_os = "windows")]
pub fn split_native_args(args: &str) -> Option<Vec<String>> {
    Some(split_windows_args(args))
}

/// Split the given command line according to Windows rules.
///
/// Reference:
/// https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments?view=msvc-170
pub fn split_windows(args: &str) -> Vec<String> {
    let (argv0, rest) = parse_windows_argv0(args);
    let mut result = vec![argv0];
    result.extend(split_windows_args(rest));
    result
}

/// Parse native Windows arguments without losing unpaired UTF-16 surrogates.
#[cfg(windows)]
pub fn split_windows_os(args: &std::ffi::OsStr) -> std::io::Result<Vec<std::ffi::OsString>> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let mut wide: Vec<_> = args.encode_wide().collect();
    // CommandLineToArgvW treats empty input as the host executable. That is not
    // a guest spawn request, and embedded NULs must not truncate the request.
    if wide.is_empty() || wide.contains(&0) {
        return Err(std::io::Error::from_raw_os_error(
            windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER as i32,
        ));
    }
    wide.push(0);
    let mut count = 0;
    // The API owns one allocation containing the pointer array and terminated
    // arguments. Copy the code units before releasing that whole allocation.
    let argv =
        unsafe { windows_sys::Win32::UI::Shell::CommandLineToArgvW(wide.as_ptr(), &mut count) };
    if argv.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let result = unsafe { std::slice::from_raw_parts(argv, count as usize) }
        .iter()
        .map(|arg| {
            let mut len = 0;
            while unsafe { *(*arg).add(len) } != 0 {
                len += 1;
            }
            std::ffi::OsString::from_wide(unsafe { std::slice::from_raw_parts(*arg, len) })
        })
        .collect();
    unsafe { windows_sys::Win32::Foundation::LocalFree(argv.cast()) };
    Ok(result)
}

/// Split an argument tail according to Windows rules, without applying the
/// special parsing rules for `argv[0]`.
pub fn split_windows_args(mut args: &str) -> Vec<String> {
    let mut result = Vec::new();

    while let Some((arg, new_rest)) = next_windows_arg(args) {
        result.push(arg);
        args = new_rest;
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
    let mut argv0 = String::new();
    let mut in_quotes = false;
    for (index, c) in args.char_indices() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if !in_quotes && matches!(c, ' ' | '\t') {
            return (argv0, &args[index..]);
        } else {
            argv0.push(c);
        }
    }
    (argv0, "")
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
