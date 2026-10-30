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

use std::io::Write;

fn main() {
    if let Some(marker) = std::env::var_os("FAKE_MOON_CRAM_MARKER") {
        let mut file = std::fs::File::create(marker).expect("failed to create marker file");
        writeln!(file, "ran").expect("failed to write marker file");
    }

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    println!("fake-moon-cram-args={}", args.join("|"));

    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut build_entries = std::env::split_paths(&path)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| path.contains("_build/native/") && path.contains("/build/"))
        .collect::<Vec<_>>();
    build_entries.sort();
    build_entries.dedup();

    for entry in build_entries {
        println!("fake-moon-cram-path={entry}");
    }

}
