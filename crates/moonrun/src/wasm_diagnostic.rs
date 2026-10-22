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

//! Backend-neutral rendering for Wasm guest diagnostics.

use std::fmt::Write as _;

use crate::source_map::SourceMap;

pub(crate) enum DiagnosticLine {
    Text(String),
    Frame {
        indentation: String,
        function: String,
        location: Option<String>,
        module_offset: Option<usize>,
    },
}

pub(crate) fn render(
    lines: impl IntoIterator<Item = DiagnosticLine>,
    source_map: Option<&SourceMap>,
    no_stack_trace: bool,
) -> String {
    let mut result = String::new();
    for (index, line) in lines.into_iter().enumerate() {
        if no_stack_trace && index != 0 {
            break;
        }
        if index != 0 {
            result.push('\n');
        }
        match line {
            DiagnosticLine::Text(line) => result.push_str(&line),
            DiagnosticLine::Frame {
                indentation,
                function,
                location,
                module_offset,
            } => {
                let function = moonutil::demangle::demangle_mangled_function_name(&function);
                write!(result, "{indentation}at {function}").unwrap();
                // Runtime locations and source positions are complementary: keep the
                // original function index and offset even when a source map is available.
                if let Some(location) = location {
                    write!(result, " ({location})").unwrap();
                } else if let Some(offset) = module_offset {
                    write!(result, " (0x{offset:x})").unwrap();
                }
                if let (Some(source_map), Some(offset)) = (source_map, module_offset)
                    && let Some(position) = source_map.position(offset)
                {
                    write!(result, " {}:{}", position.file, position.line).unwrap();
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_text_and_demangled_frames() {
        let diagnostic = render(
            [
                DiagnosticLine::Text("RuntimeError: unreachable".to_owned()),
                DiagnosticLine::Frame {
                    indentation: "    ".to_owned(),
                    function: "_M0FP13pkg3foo".to_owned(),
                    location: None,
                    module_offset: None,
                },
            ],
            None,
            false,
        );
        assert_eq!(diagnostic, "RuntimeError: unreachable\n    at @pkg.foo");
    }

    #[test]
    fn retains_offsets_without_source_maps() {
        let diagnostic = render(
            [DiagnosticLine::Frame {
                indentation: "    ".to_owned(),
                function: "wasm-function[42]".to_owned(),
                location: None,
                module_offset: Some(0x1234),
            }],
            None,
            false,
        );
        assert_eq!(diagnostic, "    at wasm-function[42] (0x1234)");
    }

    #[test]
    fn retains_runtime_locations_alongside_source_positions() {
        let directory = tempfile::tempdir().unwrap();
        let wasm = directory.path().join("module.wasm");
        std::fs::write(
            directory.path().join("module.wasm.map"),
            r#"{"sources":["main.mbt"],"mappings":"AAAA"}"#,
        )
        .unwrap();
        let source_map = crate::source_map::load(&wasm, b"").unwrap();
        let diagnostic = render(
            [DiagnosticLine::Frame {
                indentation: "    ".to_owned(),
                function: "_M0FP13pkg3foo".to_owned(),
                location: Some("wasm://module:wasm-function[42]:0x1234".to_owned()),
                module_offset: Some(0x1234),
            }],
            Some(&source_map),
            false,
        );
        assert_eq!(
            diagnostic,
            "    at @pkg.foo (wasm://module:wasm-function[42]:0x1234) main.mbt:1"
        );
    }

    #[test]
    fn hides_frames_when_stack_traces_are_disabled() {
        let diagnostic = render(
            [
                DiagnosticLine::Text("Error".to_owned()),
                DiagnosticLine::Frame {
                    indentation: "    ".to_owned(),
                    function: "throw".to_owned(),
                    location: Some("module:wasm-function[0]:0x12".to_owned()),
                    module_offset: None,
                },
            ],
            None,
            true,
        );
        assert_eq!(diagnostic, "Error");
    }
}
