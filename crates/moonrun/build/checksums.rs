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

//! Parse the SHA-256 manifests emitted by the archive producer on every OS.

use std::error::Error;

pub fn checksum<'a>(manifest: &'a str, name: &str) -> Result<&'a str, Box<dyn Error>> {
    let checksum = manifest
        .lines()
        .find_map(|line| {
            let (hash, file) = line.split_once(char::is_whitespace)?;
            // sha256sum marks binary-mode records with '*', including the
            // Windows producer's "*./name" entries. It is not part of the name.
            let file = file.trim();
            let file = file.strip_prefix('*').unwrap_or(file);
            let file = file.strip_prefix("./").unwrap_or(file);
            (file == name).then_some(hash)
        })
        .ok_or_else(|| format!("release checksum missing for {name}"))?;
    if checksum.len() != 64 || !checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("invalid release checksum for {name}").into());
    }
    Ok(checksum)
}

#[cfg(test)]
mod tests {
    use super::checksum;

    const HASH: &str = "0bb40132c411bcb2f9ebb7681dfc9b753b5136d709d75a09d44579e531298ebf";

    #[test]
    fn accepts_text_and_binary_checksum_records() {
        for prefix in ["  ", "  ./", " *", " *./"] {
            let manifest = format!("{HASH}{prefix}wee8.lib.gz\r\n");
            assert_eq!(checksum(&manifest, "wee8.lib.gz").unwrap(), HASH);
        }
    }

    #[test]
    fn reads_published_windows_manifest() {
        // These are the actual archive records from v0.106.0-moonbit.1.
        let manifest = "ed50668739bb85ced36f7e952b2828f5413dc64fbe8beb751685c347a544d89b *./rusty_v8_release_x86_64-pc-windows-msvc.lib.gz\n0bb40132c411bcb2f9ebb7681dfc9b753b5136d709d75a09d44579e531298ebf *./wee8_release_x86_64-pc-windows-msvc.lib.gz\n";
        assert_eq!(
            checksum(manifest, "wee8_release_x86_64-pc-windows-msvc.lib.gz").unwrap(),
            HASH
        );
    }

    #[test]
    fn rejects_missing_or_invalid_checksums() {
        assert!(checksum(&format!("{HASH} *./other.lib.gz\n"), "wee8.lib.gz").is_err());
        for hash in ["bad", &"z".repeat(64)] {
            assert!(checksum(&format!("{hash}  ./wee8.lib.gz\n"), "wee8.lib.gz").is_err());
        }
    }
}
