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
