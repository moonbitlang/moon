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

//! Encoding and display support for moonc's line-based response files.

use std::path::Path;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("moonc response files cannot represent argument {index}: {reason}")]
pub struct EncodeError {
    index: usize,
    reason: &'static str,
}

/// Encode arguments in the format accepted by `moonc -rsp-file`.
///
/// moonc trims each line and treats lines beginning with `#` as comments, so
/// values affected by those rules must be rejected instead of silently changed.
pub fn encode(args: &[String]) -> Result<String, EncodeError> {
    for (index, arg) in args.iter().enumerate() {
        let reason = if arg.is_empty() {
            Some("the argument is empty")
        } else if arg.trim() != arg {
            Some("leading or trailing whitespace would be trimmed")
        } else if arg.contains(['\r', '\n']) {
            Some("line breaks delimit arguments")
        } else if arg.starts_with('#') {
            Some("the argument would be parsed as a comment")
        } else if arg == "-rsp-file" {
            Some("nested response files are not supported")
        } else {
            None
        };

        if let Some(reason) = reason {
            return Err(EncodeError { index, reason });
        }
    }

    let mut content = args.join("\n");
    content.push('\n');
    Ok(content)
}

/// Expand a generated moonc response-file invocation for human-facing output.
pub fn command_for_display(command: &str, rspfile: Option<(&Path, &str)>) -> String {
    let Some((rspfile_path, rspfile_content)) = rspfile else {
        return command.to_owned();
    };
    let command_args = crate::shlex::split_native(command);
    let [executable, flag, path] = command_args.as_slice() else {
        return command.to_owned();
    };
    if flag != "-rsp-file" || Path::new(path) != rspfile_path {
        return command.to_owned();
    }

    crate::shlex::join_native(std::iter::once(executable.as_str()).chain(rspfile_content.lines()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{command_for_display, encode};

    #[test]
    fn encodes_arguments_without_changing_them() {
        let args = vec![
            "build-package".to_owned(),
            "/source/a file.mbt".to_owned(),
            "-o".to_owned(),
            "/build/#pkg.core".to_owned(),
        ];

        let content = encode(&args).expect("arguments should be representable");

        assert_eq!(content.lines().collect::<Vec<_>>(), args);
    }

    #[test]
    fn rejects_arguments_changed_by_moonc_response_file_parsing() {
        for arg in [
            "",
            " leading",
            "trailing ",
            "line\nbreak",
            "#comment",
            "-rsp-file",
        ] {
            assert!(encode(&[arg.to_owned()]).is_err(), "accepted {arg:?}");
        }
    }

    #[test]
    fn expands_response_file_command_for_display() {
        let rspfile_path = Path::new("/build/pkg.core.rsp");
        let rspfile_content = "build-package\n/source/a.mbt\n-o\n/build/pkg.core\n";
        let command = crate::shlex::join_native(
            ["/tool/moonc", "-rsp-file", "/build/pkg.core.rsp"].into_iter(),
        );

        assert_eq!(
            crate::shlex::split_native(&command_for_display(
                &command,
                Some((rspfile_path, rspfile_content))
            )),
            [
                "/tool/moonc",
                "build-package",
                "/source/a.mbt",
                "-o",
                "/build/pkg.core",
            ]
        );
    }
}
