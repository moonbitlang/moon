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

//! Backend-neutral rendering for Wasm guest diagnostics.

use std::fmt::Write as _;

use crate::source_map::SourceMap;

pub(crate) enum DiagnosticLine {
    Text(String),
    Frame {
        indentation: String,
        function: String,
        module_offset: Option<usize>,
    },
}

pub(crate) fn render(
    lines: impl IntoIterator<Item = DiagnosticLine>,
    source_map: Option<&SourceMap>,
    no_stack_trace: bool,
) -> String {
    let mut result = String::new();
    // Compiler-generated export wrappers are backend presentation details, not
    // useful MoonBit frames. Apply this policy after backend extraction so V8
    // and Wasmtime cannot disagree about whether to show them.
    let lines = lines.into_iter().filter(|line| match line {
        DiagnosticLine::Frame { function, .. } => {
            !moonutil::demangle::demangle_mangled_function_name(function)
                .contains(".moonbit_test_driver_internal_execute_wrapper/")
        }
        DiagnosticLine::Text(_) => true,
    });
    for (index, line) in lines.enumerate() {
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
                module_offset,
            } => {
                let function = moonutil::demangle::demangle_mangled_function_name(&function);
                write!(result, "{indentation}at {function}").unwrap();
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
                    module_offset: None,
                },
            ],
            None,
            false,
        );
        assert_eq!(diagnostic, "RuntimeError: unreachable\n    at @pkg.foo");
    }

    #[test]
    fn hides_frames_when_stack_traces_are_disabled() {
        let diagnostic = render(
            [
                DiagnosticLine::Text("Error".to_owned()),
                DiagnosticLine::Frame {
                    indentation: "    ".to_owned(),
                    function: "throw".to_owned(),
                    module_offset: None,
                },
            ],
            None,
            true,
        );
        assert_eq!(diagnostic, "Error");
    }

    #[test]
    fn hides_generated_test_driver_wrapper_frames() {
        let diagnostic = render(
            [
                DiagnosticLine::Text("Error".to_owned()),
                DiagnosticLine::Frame {
                    indentation: "    ".to_owned(),
                    function: "@pkg.moonbit_test_driver_internal_execute_wrapper/12".to_owned(),
                    module_offset: None,
                },
                DiagnosticLine::Frame {
                    indentation: "    ".to_owned(),
                    function: "@pkg.moonbit_test_driver_internal_execute".to_owned(),
                    module_offset: None,
                },
            ],
            None,
            false,
        );
        assert_eq!(
            diagnostic,
            "Error\n    at @pkg.moonbit_test_driver_internal_execute"
        );
    }
}
