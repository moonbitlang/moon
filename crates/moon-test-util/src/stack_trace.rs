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

use std::path::Path;

fn stack_trace_line_number_regex() -> regex::Regex {
    regex::Regex::new(r"(?<redacted>:[0-9]+)(?:[ \t]+(?:at|by)|\n|$)")
        .expect("valid stack trace line number regex")
}

fn toolchain_root_prefix() -> Option<String> {
    if let Ok(toolchain_root) = std::env::var("MOON_TOOLCHAIN_ROOT") {
        return Some(regex::escape(&toolchain_root.replace('\\', "/")));
    }

    let moonc = which::which("moonc").ok()?;
    let bin_dir = moonc.parent()?;
    let root = (bin_dir.file_name()? == "bin").then(|| {
        bin_dir
            .parent()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
    })??;
    Some(regex::escape(&root))
}

pub fn stack_trace_redactions(_src_dir: &Path) -> snapbox::Redactions {
    let mut redactions = snapbox::Redactions::new();
    redactions
        .insert("[LINE_NUMBER]", stack_trace_line_number_regex())
        .expect("valid stack trace line number redaction");
    redactions
        .insert(
            "[CORE_PATH]",
            regex::Regex::new(
                &toolchain_root_prefix()
                    .map(|toolchain_root| {
                        format!(r"(?<redacted>(?:\$MOON_TOOLCHAIN_ROOT|\$MOON_HOME|{toolchain_root}|(?:[A-Za-z]:)?/[^ \t\r\n]*\.moon)/lib/core)")
                    })
                    .unwrap_or_else(|| {
                        r"(?<redacted>(?:\$MOON_TOOLCHAIN_ROOT|\$MOON_HOME|(?:[A-Za-z]:)?/[^ \t\r\n]*\.moon)/lib/core)".to_owned()
                    }),
            )
            .expect("valid moon core path regex"),
        )
        .expect("valid moon core path redaction");
    redactions
}
