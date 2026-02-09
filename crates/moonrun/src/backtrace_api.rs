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

use crate::v8_builder::{ArgsExt, ObjectExt, ScopeExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};

pub fn should_use_backtrace_color() -> bool {
    if let Ok(explicit) = std::env::var("MOONBIT_BACKTRACE_COLOR") {
        match explicit.as_str() {
            "0" | "false" | "never" => return false,
            "1" | "true" | "always" => return true,
            _ => {}
        }
    }

    if let Ok(no_color) = std::env::var("NO_COLOR") {
        if !no_color.is_empty() {
            return false;
        }
    }

    std::io::stderr().is_terminal()
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    let absolute = path.is_absolute();

    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !absolute {
                    out.push("..");
                }
            }
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                out.push(comp.as_os_str());
            }
        }
    }

    if out.as_os_str().is_empty() && !absolute {
        PathBuf::from(".")
    } else {
        out
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalize_source_path(path: &Path) -> PathBuf {
    normalize_windows_drive_case(normalize_path(path))
}

#[cfg(windows)]
fn normalize_windows_drive_case(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path;
    };

    let (drive, verbatim) = match prefix.kind() {
        Prefix::Disk(drive) => (drive, false),
        Prefix::VerbatimDisk(drive) => (drive, true),
        _ => return path,
    };

    if !drive.is_ascii_alphabetic() {
        return path;
    }

    let lower = drive.to_ascii_lowercase() as char;
    let mut normalized = if verbatim {
        PathBuf::from(format!(r"\\?\{lower}:"))
    } else {
        PathBuf::from(format!("{lower}:"))
    };
    for component in components {
        normalized.push(component.as_os_str());
    }
    normalized
}

#[cfg(not(windows))]
fn normalize_windows_drive_case(path: PathBuf) -> PathBuf {
    path
}

fn format_source_path_auto_impl(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let source = normalize_source_path(Path::new(path));
    let normalized = display_path(&source);
    if !source.is_absolute() {
        return normalized;
    }

    let Ok(cwd) = std::env::current_dir() else {
        return normalized;
    };
    let cwd = normalize_source_path(&cwd);
    let Ok(rel) = source.strip_prefix(&cwd) else {
        return normalized;
    };
    if rel.as_os_str().is_empty() {
        return normalized;
    }
    display_path(rel)
}

#[derive(Clone, Debug)]
struct SourceMapEntry {
    addr: u32,
    source: i32,
    line: u32,
}

#[derive(Clone, Debug)]
struct ParsedSourceMap {
    sources: Vec<String>,
    mappings: Vec<SourceMapEntry>,
}

#[derive(Deserialize)]
struct RawSourceMap {
    mappings: String,
    sources: Vec<String>,
}

static SOURCE_MAP_CACHE: LazyLock<Mutex<HashMap<PathBuf, Option<ParsedSourceMap>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn read_uleb128(buf: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut i = start;
    let mut n = 0usize;
    let mut shift = 0usize;
    while i < buf.len() {
        let b = buf[i];
        i += 1;
        n |= ((b & 0x7f) as usize) << shift;
        if (b & 0x80) == 0 {
            return Some((n, i));
        }
        shift += 7;
    }
    None
}

fn extract_source_map_url_from_wasm(buf: &[u8]) -> Option<String> {
    const NAME: &str = "sourceMappingURL";
    let mut pos = 8usize; // skip wasm magic and version
    while pos < buf.len() {
        let (sec_id, id_end) = read_uleb128(buf, pos)?;
        let (sec_size, size_end) = read_uleb128(buf, id_end)?;
        let sec_end = size_end.checked_add(sec_size)?;
        if sec_end > buf.len() {
            return None;
        }

        if sec_id == 0 {
            let (sec_name_len, sec_name_pos) = read_uleb128(buf, size_end)?;
            let sec_name_end = sec_name_pos.checked_add(sec_name_len)?;
            if sec_name_end > sec_end {
                return None;
            }
            let sec_name = std::str::from_utf8(&buf[sec_name_pos..sec_name_end]).ok()?;
            if sec_name == NAME {
                let (val_len, val_pos) = read_uleb128(buf, sec_name_end)?;
                let val_end = val_pos.checked_add(val_len)?;
                if val_end > sec_end {
                    return None;
                }
                let value = std::str::from_utf8(&buf[val_pos..val_end]).ok()?;
                return Some(value.to_string());
            }
        }

        pos = sec_end;
    }
    None
}

fn base64_index(ch: u8) -> Option<i32> {
    match ch {
        b'A'..=b'Z' => Some((ch - b'A') as i32),
        b'a'..=b'z' => Some(26 + (ch - b'a') as i32),
        b'0'..=b'9' => Some(52 + (ch - b'0') as i32),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_vlq_value(bytes: &[u8], i: &mut usize) -> Option<i32> {
    let mut value = 0u32;
    let mut shift = 0u32;
    loop {
        if *i >= bytes.len() {
            return None;
        }
        let digit = base64_index(bytes[*i])? as u32;
        *i += 1;
        if shift >= 32 {
            return None;
        }
        let shifted = (digit & 31).checked_shl(shift)?;
        value = value.checked_add(shifted)?;
        let cont = (digit & 32) != 0;
        if !cont {
            break;
        }
        shift += 5;
    }

    // Keep decoded values within i32 so malformed source maps fail gracefully.
    if (value >> 1) > i32::MAX as u32 {
        return None;
    }
    let neg = (value & 1) != 0;
    let value = (value >> 1) as i32;
    Some(if neg { -value } else { value })
}

fn decode_vlq_segment(seg: &str) -> Option<Vec<i32>> {
    let bytes = seg.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < bytes.len() {
        out.push(decode_vlq_value(bytes, &mut i)?);
    }
    Some(out)
}

fn parse_wasm_source_map(raw: RawSourceMap) -> Option<ParsedSourceMap> {
    let text = raw.mappings;
    let mut mappings = Vec::new();
    let mut generated_line = 0i32;
    let mut generated_column = 0i32;
    let mut source = 0i32;
    let mut original_line = 0i32;
    let mut _original_column = 0i32;

    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b';' => {
                generated_line += 1;
                generated_column = 0;
                i += 1;
                continue;
            }
            b',' => {
                i += 1;
                continue;
            }
            _ => {}
        }

        let mut j = i;
        while j < bytes.len() && bytes[j] != b',' && bytes[j] != b';' {
            j += 1;
        }
        let seg = decode_vlq_segment(&text[i..j])?;
        if seg.is_empty() {
            return None;
        }

        generated_column += seg[0];
        if seg.len() >= 4 {
            source += seg[1];
            original_line += seg[2];
            _original_column += seg[3];
            // moon_wat2wasm encodes address into generated column.
            if generated_line == 0 {
                if generated_column < 0 || original_line < 0 {
                    return None;
                }
                mappings.push(SourceMapEntry {
                    addr: generated_column as u32,
                    source,
                    line: (original_line + 1) as u32,
                });
            }
        }
        i = j;
    }

    Some(ParsedSourceMap {
        sources: raw.sources,
        mappings,
    })
}

fn resolve_map_path(wasm_path: &Path, embedded: &str) -> Option<PathBuf> {
    if embedded.is_empty()
        || embedded.starts_with("data:")
        || embedded.starts_with("http://")
        || embedded.starts_with("https://")
    {
        return None;
    }

    let embedded_path = Path::new(embedded);
    if embedded_path.is_absolute() {
        return Some(embedded_path.to_path_buf());
    }
    let base = wasm_path.parent().unwrap_or_else(|| Path::new("."));
    Some(base.join(embedded_path))
}

fn load_source_map_for_module(wasm_path: &Path) -> Option<ParsedSourceMap> {
    if let Some(cached) = SOURCE_MAP_CACHE.lock().unwrap().get(wasm_path).cloned() {
        return cached;
    }

    let parsed = (|| {
        let wasm_bytes = std::fs::read(wasm_path).ok()?;
        let embedded = extract_source_map_url_from_wasm(&wasm_bytes);
        let map_path = embedded
            .as_deref()
            .and_then(|u| resolve_map_path(wasm_path, u))
            .unwrap_or_else(|| PathBuf::from(format!("{}.map", wasm_path.to_string_lossy())));
        let map_bytes = std::fs::read(map_path).ok()?;
        let raw: RawSourceMap = serde_json_lenient::from_slice(&map_bytes).ok()?;
        parse_wasm_source_map(raw)
    })();

    SOURCE_MAP_CACHE
        .lock()
        .unwrap()
        .insert(wasm_path.to_path_buf(), parsed.clone());
    parsed
}

fn parse_offset_from_wasm_location(location: &str) -> Option<u32> {
    let s = location.trim_end();
    let idx = s.rfind(":0x")?;
    let hex = &s[idx + 3..];
    if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

fn source_pos_for_offset(module_name: &str, offset: u32) -> Option<String> {
    let sm = load_source_map_for_module(Path::new(module_name))?;
    let idx = sm.mappings.partition_point(|m| m.addr <= offset);
    if idx == 0 {
        return None;
    }
    let m = &sm.mappings[idx - 1];
    let source_idx = usize::try_from(m.source).ok()?;
    let source_file = format_source_path_auto_impl(sm.sources.get(source_idx)?);
    Some(format!("{source_file}:{}", m.line))
}

fn source_pos_for_wasm_location(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let module_name = args.string_lossy(scope, 0);
    let location = args.string_lossy(scope, 1);
    let source_pos = parse_offset_from_wasm_location(&location)
        .and_then(|offset| source_pos_for_offset(&module_name, offset))
        .unwrap_or_default();
    let source_pos = scope.string(&source_pos);
    ret.set(source_pos.into());
}

pub fn init_backtrace<'s>(obj: v8::Local<'s, v8::Object>, scope: &mut v8::HandleScope<'s>) {
    obj.set_func(
        scope,
        "source_pos_for_wasm_location",
        source_pos_for_wasm_location,
    );
}

#[cfg(test)]
mod tests {
    use super::{
        RawSourceMap, decode_vlq_segment, display_path, format_source_path_auto_impl,
        normalize_path, parse_offset_from_wasm_location, parse_wasm_source_map,
    };
    use std::path::Path;

    #[test]
    fn normalize_keeps_semantics() {
        assert_eq!(
            display_path(&normalize_path(Path::new("./a/./b/../c"))),
            "a/c"
        );
        assert_eq!(
            display_path(&normalize_path(Path::new("/a/./b/../c"))),
            "/a/c"
        );
    }

    #[test]
    fn relative_path_stays_relative() {
        assert_eq!(format_source_path_auto_impl("a/b/c.mbt"), "a/b/c.mbt");
    }

    #[test]
    fn path_inside_cwd_becomes_relative() {
        let cwd = std::env::current_dir().unwrap();
        let path = cwd.join("foo/bar.mbt");
        assert_eq!(
            format_source_path_auto_impl(path.to_string_lossy().as_ref()),
            "foo/bar.mbt"
        );
    }

    #[test]
    fn decode_vlq_segment_works() {
        assert_eq!(decode_vlq_segment("A"), Some(vec![0]));
        assert_eq!(decode_vlq_segment("C"), Some(vec![1]));
        assert_eq!(decode_vlq_segment("D"), Some(vec![-1]));
    }

    #[test]
    fn decode_vlq_segment_rejects_shift_overflow() {
        assert_eq!(decode_vlq_segment("ggggggggA"), None);
    }

    #[test]
    fn parse_offset_from_location_works() {
        assert_eq!(parse_offset_from_wasm_location("wasm://x:0x10"), Some(0x10));
        assert_eq!(
            parse_offset_from_wasm_location("wasm://x:0x2a   "),
            Some(0x2a)
        );
        assert_eq!(parse_offset_from_wasm_location("wasm://x"), None);
    }

    #[test]
    fn parse_wasm_source_map_handles_basic_case() {
        // One segment with generated col=0, source=0, line=0, col=0.
        let raw = RawSourceMap {
            mappings: "AAAA".to_string(),
            sources: vec!["main/main.mbt".to_string()],
        };
        let parsed = parse_wasm_source_map(raw).unwrap();
        assert_eq!(parsed.sources.len(), 1);
        assert_eq!(parsed.mappings.len(), 1);
        assert_eq!(parsed.mappings[0].addr, 0);
        assert_eq!(parsed.mappings[0].source, 0);
        assert_eq!(parsed.mappings[0].line, 1);
    }

    #[cfg(windows)]
    #[test]
    fn path_inside_cwd_with_drive_case_mismatch_becomes_relative() {
        let cwd = std::env::current_dir().unwrap();
        let cwd = display_path(&normalize_path(&cwd));
        let mut cwd_bytes = cwd.into_bytes();
        if cwd_bytes.len() < 2 || cwd_bytes[1] != b':' || !cwd_bytes[0].is_ascii_alphabetic() {
            return;
        }
        cwd_bytes[0] = if cwd_bytes[0].is_ascii_uppercase() {
            cwd_bytes[0].to_ascii_lowercase()
        } else {
            cwd_bytes[0].to_ascii_uppercase()
        };
        let cwd_with_mismatched_drive = String::from_utf8(cwd_bytes).unwrap();
        let path = format!("{cwd_with_mismatched_drive}/foo/bar.mbt");
        assert_eq!(format_source_path_auto_impl(&path), "foo/bar.mbt");
    }
}
