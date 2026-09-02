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

#[derive(serde::Deserialize)]
struct RawSourceMap {
    sources: Vec<String>,
    mappings: String,
}

#[derive(Clone, Debug)]
struct Mapping {
    address: usize,
    source: usize,
    line: usize,
}

/// A parsed moon_wat2wasm address-oriented source map.
#[derive(Clone, Debug)]
pub(crate) struct SourceMap {
    sources: Vec<String>,
    mappings: Vec<Mapping>,
}

/// The source file and one-based line associated with a Wasm byte offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourcePosition<'a> {
    pub(crate) file: &'a str,
    pub(crate) line: usize,
}

impl SourceMap {
    fn parse(value: &str) -> Option<Self> {
        let source_map = serde_json::from_str::<RawSourceMap>(value).ok()?;
        Some(Self {
            sources: source_map.sources,
            mappings: decode_mappings(&source_map.mappings)?,
        })
    }

    /// Find the last mapping at or before `offset`.
    pub(crate) fn position(&self, offset: usize) -> Option<SourcePosition<'_>> {
        let mapping = self
            .mappings
            .partition_point(|mapping| mapping.address <= offset)
            .checked_sub(1)
            .and_then(|index| self.mappings.get(index))?;
        Some(SourcePosition {
            file: self.sources.get(mapping.source)?,
            line: mapping.line + 1,
        })
    }
}

fn decode_mappings(value: &str) -> Option<Vec<Mapping>> {
    let mut mappings = Vec::new();
    let mut source = 0_i64;
    let mut original_line = 0_i64;

    for (generated_line, line) in value.split(';').enumerate() {
        let mut generated_column = 0_i64;
        for segment in line.split(',').filter(|segment| !segment.is_empty()) {
            let values = decode_vlq_segment(segment)?;
            generated_column = generated_column.checked_add(*values.first()?)?;
            if values.len() >= 4 {
                source = source.checked_add(values[1])?;
                original_line = original_line.checked_add(values[2])?;
                if generated_line == 0 {
                    mappings.push(Mapping {
                        address: usize::try_from(generated_column).ok()?,
                        source: usize::try_from(source).ok()?,
                        line: usize::try_from(original_line).ok()?,
                    });
                }
            }
        }
    }
    Some(mappings)
}

fn decode_vlq_segment(segment: &str) -> Option<Vec<i64>> {
    let mut values = Vec::new();
    let mut chars = segment.bytes();
    while chars.len() != 0 {
        let mut value = 0_i64;
        let mut shift = 0_u32;
        loop {
            let digit = match chars.next()? {
                value @ b'A'..=b'Z' => value - b'A',
                value @ b'a'..=b'z' => value - b'a' + 26,
                value @ b'0'..=b'9' => value - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            };
            value |= i64::from(digit & 31).checked_shl(shift)?;
            if digit & 32 == 0 {
                break;
            }
            shift = shift.checked_add(5)?;
        }
        let negative = value & 1 != 0;
        value >>= 1;
        values.push(if negative { -value } else { value });
    }
    Some(values)
}

/// Load the source map associated with these Wasm bytes, if it is locally
/// available. Source maps are optional diagnostics, so malformed metadata and
/// I/O errors deliberately degrade to an absent map.
pub(crate) fn load(wasm_path: &Path, wasm: &[u8]) -> Option<SourceMap> {
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
    SourceMap::parse(&std::fs::read_to_string(map_path).ok()?)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_wasm_offsets_to_the_nearest_source_position() {
        let map = SourceMap::parse(r#"{"sources":["main.mbt","lib.mbt"],"mappings":"AAAA,UCCA"}"#)
            .unwrap();

        assert_eq!(
            map.position(9),
            Some(SourcePosition {
                file: "main.mbt",
                line: 1
            })
        );
        assert_eq!(
            map.position(10),
            Some(SourcePosition {
                file: "lib.mbt",
                line: 2
            })
        );
    }

    #[test]
    fn rejects_malformed_source_maps() {
        assert!(SourceMap::parse(r#"{"sources":[],"mappings":"?"}"#).is_none());
        assert!(SourceMap::parse("not json").is_none());
    }
}
