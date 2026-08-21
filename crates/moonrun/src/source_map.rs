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

const SOURCE_MAPPING_URL: &[u8] = b"sourceMappingURL";

/// Load the source map associated with these Wasm bytes, if it is locally
/// available. Source maps are optional diagnostics, so malformed metadata and
/// I/O errors deliberately degrade to an absent map.
pub(crate) fn load(wasm_path: &Path, wasm: &[u8]) -> Option<String> {
    let map_path = match source_mapping_url(wasm) {
        Some(url)
            if !url.is_empty()
                && !url.starts_with("data:")
                && !url.starts_with("http://")
                && !url.starts_with("https://") =>
        {
            let path = Path::new(url);
            if path.is_absolute() {
                path.to_owned()
            } else {
                wasm_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(path)
            }
        }
        _ => {
            let mut path = wasm_path.as_os_str().to_owned();
            path.push(".map");
            PathBuf::from(path)
        }
    };
    std::fs::read_to_string(map_path).ok()
}

fn source_mapping_url(wasm: &[u8]) -> Option<&str> {
    if !wasm.starts_with(b"\0asm\x01\0\0\0") {
        return None;
    }

    let mut section_start = 8;
    while section_start < wasm.len() {
        let section_id = *wasm.get(section_start)?;
        let (section_len, payload_start) = read_u32_leb(wasm, section_start + 1)?;
        let payload_end = payload_start.checked_add(section_len)?;
        let payload = wasm.get(payload_start..payload_end)?;

        if section_id == 0 {
            let (name_len, name_start) = read_u32_leb(payload, 0)?;
            let name_end = name_start.checked_add(name_len)?;
            if payload.get(name_start..name_end)? == SOURCE_MAPPING_URL {
                let (url_len, url_start) = read_u32_leb(payload, name_end)?;
                let url_end = url_start.checked_add(url_len)?;
                return std::str::from_utf8(payload.get(url_start..url_end)?).ok();
            }
        }

        section_start = payload_end;
    }
    None
}

fn read_u32_leb(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut value = 0_u32;
    for byte_index in 0..5 {
        let byte = *bytes.get(start + byte_index)?;
        if byte_index == 4 && byte & 0xf0 != 0 {
            return None;
        }
        value |= u32::from(byte & 0x7f) << (byte_index * 7);
        if byte & 0x80 == 0 {
            return Some((value as usize, start + byte_index + 1));
        }
    }
    None
}
