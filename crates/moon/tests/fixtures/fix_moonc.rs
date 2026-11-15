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

//! A stand-in for `moonc check` that replays canned JSON diagnostics, so that
//! `moon fix` tests control exactly which fixes are proposed.
//!
//! For each source file `F` it compiles, it reads `F.diagnostics`, whose lines
//! are `TARGET|NEEDLE|JSON`. A line is printed to stderr, with `$FILE`
//! replaced by the absolute path of `F`, if `TARGET` is `*` or the `-target`
//! of this invocation and `F` contains `NEEDLE` (so a fixed file stops
//! reporting). It writes the `-o` output and fails if it printed an error.
//! `-doctest-only` checks report nothing, like moonc's for ordinary code.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("check") {
        return;
    }
    let value_of = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|i| args.get(i + 1))
    };
    if let Some(output) = value_of("-o") {
        let output = Path::new(output);
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::write(output, "").unwrap();
    }
    if args.iter().any(|arg| arg == "-doctest-only") {
        return;
    }
    let target = value_of("-target").cloned().unwrap_or_default();
    let mut failed = false;
    for source in args.iter().filter(|arg| arg.ends_with(".mbt")) {
        let Ok(canned) = std::fs::read_to_string(format!("{source}.diagnostics")) else {
            continue;
        };
        let text = std::fs::read_to_string(source).unwrap();
        // Like moonc, report the path as moon passed it.
        let absolute = std::env::current_dir().unwrap().join(source);
        let escaped = absolute.to_str().unwrap().replace('\\', "\\\\");
        for line in canned.lines().filter(|line| !line.trim().is_empty()) {
            let mut parts = line.splitn(3, '|');
            let (Some(line_target), Some(needle), Some(json)) =
                (parts.next(), parts.next(), parts.next())
            else {
                panic!("malformed canned diagnostic: {line}");
            };
            if (line_target == "*" || line_target == target) && text.contains(needle) {
                failed |= json.contains(r#""level":"error""#);
                eprintln!("{}", json.replace("$FILE", &escaped));
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
