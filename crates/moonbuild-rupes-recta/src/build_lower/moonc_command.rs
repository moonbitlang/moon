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

use std::path::{Path, PathBuf};

use n2::graph::RspFile;

use super::{Commandline, LoweringError};

// Keep a portable margin below Windows' 32,767 UTF-16-code-unit process limit.
// UTF-8 byte length is a conservative approximation for this purpose.
const RESPONSE_FILE_THRESHOLD: usize = 16 * 1024;

pub(super) fn lower(
    args: Vec<String>,
    primary_output: &Path,
) -> Result<Commandline, LoweringError> {
    let (executable, response_args) = args
        .split_first()
        .expect("moonc command builders always provide an executable");
    let command = moonutil::shlex::join_native(args.iter().map(String::as_str));
    if command.len() < RESPONSE_FILE_THRESHOLD {
        return Ok(args.into());
    }

    let content = encode(response_args)?;
    let mut rsp_path = primary_output.as_os_str().to_owned();
    rsp_path.push(".rsp");
    let rsp_path = PathBuf::from(rsp_path);
    let rsp_path_arg = rsp_path.to_string_lossy();
    let command = moonutil::shlex::join_native(
        [executable.as_str(), "-rsp-file", rsp_path_arg.as_ref()].into_iter(),
    );

    Ok(Commandline::from(args).with_response_file(
        command,
        RspFile {
            path: rsp_path,
            content,
        },
    ))
}

/// Encode arguments in the line-based format accepted by `moonc -rsp-file`.
fn encode(args: &[String]) -> Result<String, LoweringError> {
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
            return Err(LoweringError::MooncResponseFile { index, reason });
        }
    }

    Ok(format!("{}\n", args.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_logical_args_while_selecting_execution_transport() {
        let output = Path::new("build/pkg.core");
        let short = vec!["moonc".to_owned(), "build-package".to_owned()];
        let lowered = lower(short.clone(), output).expect("short command should lower");
        let (command, rspfile) = lowered.into_n2();
        assert_eq!(moonutil::shlex::split_native(&command), short);
        assert!(rspfile.is_none());

        let long = vec![
            "moonc".to_owned(),
            "build-package".to_owned(),
            "x".repeat(RESPONSE_FILE_THRESHOLD),
        ];
        let lowered = lower(long.clone(), output).expect("oversized command should lower");
        assert_eq!(lowered.args(), Some(&long));
        let (command, rspfile) = lowered.into_n2();
        let rspfile = rspfile.expect("oversized command should use a response file");

        assert_eq!(
            moonutil::shlex::split_native(&command),
            [
                "moonc".to_owned(),
                "-rsp-file".to_owned(),
                rspfile.path.to_string_lossy().into_owned(),
            ]
        );
        assert_eq!(rspfile.path, Path::new("build/pkg.core.rsp"));
        assert_eq!(rspfile.content.lines().collect::<Vec<_>>(), long[1..]);
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
}
