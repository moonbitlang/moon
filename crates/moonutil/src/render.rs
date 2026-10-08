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

use std::{
    cell::OnceCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

use ariadne::{Fmt, ReportKind};
use clap::ValueEnum;
use log::{error, warn};
use serde::{Deserialize, Serialize};

use crate::{error_code_docs::get_error_code_doc, test_metadata::DiagnosticLevel};

#[derive(Debug, Deserialize)]
pub struct PatchJSON {
    pub drops: Vec<String>,
    pub patches: Vec<PatchItem>,
}

#[derive(Debug, Deserialize)]
pub struct PatchItem {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct MooncDiagnostic {
    pub path: String,
    pub loc: Loc,
    pub level: String,
    pub message: String,
    pub error_code: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<MooncDiagnostic>,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Loc {
    pub start: Position,
    pub end: Position,
}

impl Loc {
    fn zero() -> Self {
        Self {
            start: Position { line: 0, col: 0 },
            end: Position { line: 0, col: 0 },
        }
    }

    fn parse_span(text: &str) -> Option<Self> {
        let (start, end) = text.split_once('-')?;

        Some(Self {
            start: Position::parse(start)?,
            end: Position::parse(end)?,
        })
    }

    fn as_range(&self) -> (&Position, &Position) {
        (&self.start, &self.end)
    }
}

impl Serialize for Loc {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!(
            "{}:{}-{}:{}",
            self.start.line, self.start.col, self.end.line, self.end.col
        ))
    }
}

impl<'de> Deserialize<'de> for Loc {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        if text.is_empty() {
            return Ok(Self::zero());
        }
        let loc = Self::parse_span(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid location: {text}")))?;
        Ok(loc)
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    fn parse(text: &str) -> Option<Self> {
        let (line, col) = text.split_once(':')?;
        Some(Self {
            line: line.parse::<usize>().ok()?,
            col: col.parse::<usize>().ok()?,
        })
    }
}

struct DiagnosticSource {
    display_filename: String,
    source: ariadne::Source,
    line_offsets: Vec<usize>,
}

impl DiagnosticSource {
    fn new(display_filename: String, content: String) -> Self {
        // Compiler columns and Ariadne spans count Unicode scalar values.
        // Only LF starts a compiler line; Ariadne also recognizes other
        // separators, so its line table cannot replace this index.
        let line_offsets = std::iter::once(0)
            .chain(
                content
                    .chars()
                    .enumerate()
                    .filter_map(|(offset, ch)| (ch == '\n').then_some(offset + 1)),
            )
            .collect();
        Self {
            display_filename,
            source: ariadne::Source::from(content),
            line_offsets,
        }
    }

    fn calculate_offset(&self, position: &Position) -> Option<usize> {
        let line_offset = self.line_offsets.get(position.line.checked_sub(1)?)?;
        let offset = line_offset.checked_add(position.col.checked_sub(1)?)?;
        (offset <= self.source.len()).then_some(offset)
    }
}

/// Source text and indexes shared by one batch of rendered diagnostics.
///
/// Create a fresh cache for each batch so subsequent builds observe file edits.
pub struct DiagnosticSources<'a> {
    files: HashMap<String, Option<DiagnosticSource>>,
    source_maps: HashMap<String, Option<SourceMap>>,
    patch_file: Option<&'a Path>,
    patch_sources: OnceCell<Option<HashMap<String, DiagnosticSource>>>,
}

impl<'a> DiagnosticSources<'a> {
    pub fn new(patch_file: Option<&'a Path>) -> Self {
        Self {
            files: HashMap::new(),
            source_maps: HashMap::new(),
            patch_file,
            patch_sources: OnceCell::new(),
        }
    }

    fn get_file(&mut self, path: &str) -> Option<&DiagnosticSource> {
        self.files
            .entry(path.to_owned())
            .or_insert_with(|| {
                std::fs::read_to_string(path)
                    .ok()
                    .map(|content| DiagnosticSource::new(path.to_owned(), content))
            })
            .as_ref()
    }

    fn get_diagnostic_source(&mut self, path: &str) -> Option<&DiagnosticSource> {
        self.get_file(path);
        if let Some(source) = self.files.get(path)?.as_ref() {
            return Some(source);
        }

        let patch_file = self.patch_file?;
        let patch_sources = self
            .patch_sources
            .get_or_init(|| {
                let content = std::fs::read_to_string(patch_file).ok()?;
                let patch: PatchJSON = serde_json_lenient::from_str(&content).ok()?;
                let mut sources = HashMap::new();
                for item in patch.patches {
                    sources
                        .entry(item.name.clone())
                        .or_insert_with(|| DiagnosticSource::new(item.name, item.content));
                }
                Some(sources)
            })
            .as_ref()?;
        patch_sources.get(Path::new(path).file_name()?.to_str()?)
    }

    fn remap(
        &mut self,
        path: &str,
        start_offset: usize,
        end_offset: usize,
    ) -> Option<(String, usize, usize)> {
        let path_to_map_json = format!("{path}.map.json");
        let map = self
            .source_maps
            .entry(path_to_map_json.clone())
            .or_insert_with(|| {
                let content = std::fs::read_to_string(&path_to_map_json).ok()?;
                serde_json_lenient::from_str::<SourceMap>(&content).ok()
            })
            .as_ref()?;
        let map_path = Path::new(&path_to_map_json);
        let (source1, start_offset) = map.to_original(start_offset, map_path)?;
        let (source2, end_offset) = map.to_original(end_offset, map_path)?;
        if source1 != source2 {
            return None;
        }
        let path = source1.display().to_string();
        self.get_file(&path)?;
        Some((path, start_offset, end_offset))
    }
}

/// Source map for diagnostics emitted on generated `.mbt` files.
///
/// This is used when a `.mbt` file is produced by a third-party generator
/// (for example `moonyacc` or other code generators), so we can map diagnostics
/// back to the original source definition locations.
///
/// Small demo of the idea:
///
/// ```text
/// source (toy.src):
///   start main
///   print "hello"
///   end
///
/// generated (main.mbt):
///   fn main {
///     println("hello")
///   }
/// ```
///
/// `main.mbt.map.json` then maps spans in `main.mbt` back to `toy.src` so
/// diagnostics are reported against the source DSL file.
#[derive(Deserialize)]
struct SourceMap {
    mappings: Vec<SourceMapping>,
}

impl SourceMap {
    fn to_original(&self, offset: usize, base_path: &Path) -> Option<(PathBuf, usize)> {
        let index = self
            .mappings
            .binary_search_by(|mapping| {
                if offset < mapping.generated_offset {
                    std::cmp::Ordering::Greater
                } else if offset > mapping.generated_offset + mapping.length {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        let mapping = &self.mappings[index];
        let path = dunce::canonicalize(base_path.parent()?.join(&mapping.source)).ok()?;
        Some((
            path,
            offset - mapping.generated_offset + mapping.original_offset,
        ))
    }
}

#[derive(Deserialize)]
struct SourceMapping {
    source: String,
    original_offset: usize,
    generated_offset: usize,
    length: usize,
}

impl MooncDiagnostic {
    // TODO: swap names for `render` and `render_diagnostics`
    pub fn render_diagnostics(
        &self,
        use_fancy: bool,
        sources: &mut DiagnosticSources<'_>,
        explain: bool,
        render_no_loc_level: DiagnosticLevel,
    ) -> Option<ReportKind<'static>> {
        let diagnostic = self;
        let bail_print_original = || {
            eprintln!(
                "{}",
                serde_json_lenient::to_string(diagnostic).unwrap_or_default()
            );
            None
        };
        let (kind, color) = diagnostic.get_level_and_color();

        // for no-location diagnostic, like Missing main function in the main package(4067)
        if diagnostic.path.is_empty() {
            // Check if this diagnostic level should be rendered based on the threshold
            if DiagnosticLevel::from_str(&diagnostic.level, true)
                .is_ok_and(|l| l >= render_no_loc_level)
            {
                eprintln!(
                    "{}",
                    format!(
                        "{}: [{}] {}",
                        kind,
                        diagnostic.formatted_error_code(),
                        diagnostic.message
                    )
                    .fg(color)
                );
            }
            return Some(kind);
        }

        let Some(source) = sources.get_diagnostic_source(&diagnostic.path) else {
            eprintln!(
                "failed to read file `{}`, [{}] {}: {}",
                diagnostic.path,
                diagnostic.formatted_error_code(),
                diagnostic.level,
                diagnostic.message
            );
            return Some(kind);
        };

        let (start_position, end_position) = diagnostic.loc.as_range();
        let Some(start_offset) = source.calculate_offset(start_position) else {
            error!("failed to calculate start offset for diagnostic");
            bail_print_original()?;
            return None;
        };
        let Some(end_offset) = source.calculate_offset(end_position) else {
            error!("failed to calculate end offset for diagnostic");
            bail_print_original()?;
            return None;
        };

        // Remapping if there's .map.json file
        // TODO: log reasons for `.map.json` exists but not works.
        let (source_path, start_offset, end_offset) = sources
            .remap(&diagnostic.path, start_offset, end_offset)
            .unwrap_or_else(|| (diagnostic.path.clone(), start_offset, end_offset));
        let source = sources
            .get_diagnostic_source(&source_path)
            .expect("diagnostic source was already loaded");
        let display_filename = &source.display_filename;

        let mut report_builder =
            ariadne::Report::build(kind, (display_filename, start_offset..end_offset)).with_label(
                ariadne::Label::new((display_filename, start_offset..end_offset))
                    .with_message((&diagnostic.message).fg(color))
                    .with_color(color),
            );

        if explain {
            let error_code_doc = get_error_code_doc(&diagnostic.formatted_error_code());
            if let Some(doc) = error_code_doc {
                report_builder = report_builder.with_help(doc.fg(color));
            } else {
                warn!(
                    "Failed to get doc for error code: {}",
                    diagnostic.formatted_error_code()
                );
            }
        } else {
            report_builder = report_builder
                .with_message(format!("[{}]", diagnostic.formatted_error_code()).fg(color));
        }

        if !use_fancy {
            report_builder =
                report_builder.with_config(ariadne::Config::default().with_color(false));
        }

        match report_builder
            .finish()
            .eprint((display_filename, &source.source))
        {
            Ok(_) => {}
            Err(e) => {
                eprintln!("internal rendering error: {e:?}");
            }
        };

        Some(kind)
    }

    fn get_level_and_color(&self) -> (ariadne::ReportKind<'static>, ariadne::Color) {
        if self.level == "error" {
            (ariadne::ReportKind::Error, ariadne::Color::Red)
        } else if self.level == "warning" {
            (ariadne::ReportKind::Warning, ariadne::Color::BrightYellow)
        } else {
            (ariadne::ReportKind::Advice, ariadne::Color::Blue)
        }
    }

    pub fn formatted_error_code(&self) -> String {
        format!("{:04}", self.error_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_diagnostics_accepts_no_location_diagnostic_json() {
        // Stable compiler repro: run `moonc check` on any valid `.mbt` file and
        // pass that same existing non-`.mi` file to `-check-mi`. This emits a
        // no-location diagnostic with `path: ""` and `loc: "0:0-0:0"`.
        let diagnostic_json = r#"{"$message_type":"diagnostic","level":"error","error_code":4049,"path":"","loc":"0:0-0:0","message":"Magic number mismatch"}"#;

        let diagnostic = serde_json_lenient::from_str::<MooncDiagnostic>(diagnostic_json).unwrap();
        assert!(diagnostic.path.is_empty());

        let rendered = diagnostic.render_diagnostics(
            false,
            &mut DiagnosticSources::new(None),
            false,
            DiagnosticLevel::Error,
        );

        assert!(rendered.is_some());
    }

    #[test]
    fn diagnostic_batch_reuses_source_and_observes_edits_in_the_next_batch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.mbt");
        std::fs::write(&path, "original\n").unwrap();
        let path = path.to_str().unwrap();

        let mut sources = DiagnosticSources::new(None);
        let source = sources.get_diagnostic_source(path).unwrap();
        let source_address = &source.source as *const _;
        let index_address = source.line_offsets.as_ptr();
        std::fs::write(path, "edited\n").unwrap();

        let source = sources.get_diagnostic_source(path).unwrap();
        assert_eq!(source.source.text(), "original\n");
        assert_eq!(&source.source as *const _, source_address);
        assert_eq!(source.line_offsets.as_ptr(), index_address);

        let mut next_batch = DiagnosticSources::new(None);
        assert_eq!(
            next_batch
                .get_diagnostic_source(path)
                .unwrap()
                .source
                .text(),
            "edited\n"
        );
    }

    #[test]
    fn diagnostic_positions_use_unicode_scalar_columns_and_lf_lines() {
        let source =
            DiagnosticSource::new("source.mbt".to_owned(), "é🦀\r\nx\u{2028}z\n".to_owned());
        for (line, col, offset) in [(1, 1, 0), (1, 3, 2), (2, 1, 4), (2, 3, 6), (3, 1, 8)] {
            assert_eq!(
                source.calculate_offset(&Position { line, col }),
                Some(offset)
            );
        }
        for (line, col) in [(0, 1), (1, 0), (4, 1), (3, 2), (1, usize::MAX)] {
            assert_eq!(source.calculate_offset(&Position { line, col }), None);
        }
        let empty = DiagnosticSource::new("empty.mbt".to_owned(), String::new());
        assert_eq!(
            empty.calculate_offset(&Position { line: 1, col: 1 }),
            Some(0)
        );
    }

    #[test]
    fn diagnostic_batch_reuses_patch_sources() {
        let dir = tempfile::tempdir().unwrap();
        let patch_file = dir.path().join("patch.json");
        std::fs::write(
            &patch_file,
            r#"{"drops":[],"patches":[{"name":"generated.mbt","content":"é🦀\n"},{"name":"second.mbt","content":"second\n"},{"name":"generated.mbt","content":"duplicate\n"}]}"#,
        )
        .unwrap();
        let generated = dir.path().join("generated.mbt");
        let generated = generated.to_str().unwrap();
        let second = dir.path().join("second.mbt");
        let second = second.to_str().unwrap();
        let mut sources = DiagnosticSources::new(Some(&patch_file));
        let source = sources.get_diagnostic_source(generated).unwrap();
        assert_eq!(source.display_filename, "generated.mbt");
        assert_eq!(source.source.text(), "é🦀\n");
        assert_eq!(
            source.calculate_offset(&Position { line: 1, col: 2 }),
            Some(1)
        );

        std::fs::write(
            &patch_file,
            r#"{"drops":[],"patches":[{"name":"generated.mbt","content":"edited\n"}]}"#,
        )
        .unwrap();
        assert_eq!(
            sources
                .get_diagnostic_source(generated)
                .unwrap()
                .source
                .text(),
            "é🦀\n"
        );
        assert_eq!(
            sources.get_diagnostic_source(second).unwrap().source.text(),
            "second\n"
        );
        let mut next_batch = DiagnosticSources::new(Some(&patch_file));
        assert_eq!(
            next_batch
                .get_diagnostic_source(generated)
                .unwrap()
                .source
                .text(),
            "edited\n"
        );
    }

    #[test]
    fn diagnostic_batch_reuses_source_maps_and_original_sources() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.mbt");
        std::fs::write(&original, "é🦀\n").unwrap();
        let map_file = dir.path().join("generated.mbt.map.json");
        std::fs::write(
            &map_file,
            r#"{"mappings":[{"source":"original.mbt","original_offset":1,"generated_offset":10,"length":5}]}"#,
        )
        .unwrap();
        let generated = dir.path().join("generated.mbt");
        let generated = generated.to_str().unwrap();
        let original = dunce::canonicalize(original).unwrap().display().to_string();
        let mut sources = DiagnosticSources::new(None);
        assert_eq!(
            sources.remap(generated, 10, 12),
            Some((original.clone(), 1, 3))
        );

        std::fs::write(&original, "edited\n").unwrap();
        std::fs::write(
            &map_file,
            r#"{"mappings":[{"source":"original.mbt","original_offset":0,"generated_offset":10,"length":5}]}"#,
        )
        .unwrap();
        assert_eq!(
            sources.remap(generated, 10, 12),
            Some((original.clone(), 1, 3))
        );
        assert_eq!(sources.get_file(&original).unwrap().source.text(), "é🦀\n");

        let mut next_batch = DiagnosticSources::new(None);
        assert_eq!(
            next_batch.remap(generated, 10, 12),
            Some((original.clone(), 0, 2))
        );
        assert_eq!(
            next_batch.get_file(&original).unwrap().source.text(),
            "edited\n"
        );
    }

    #[test]
    fn diagnostic_batch_caches_missing_files_and_source_maps() {
        let dir = tempfile::tempdir().unwrap();
        let generated = dir.path().join("generated.mbt");
        let generated = generated.to_str().unwrap();
        let mut sources = DiagnosticSources::new(None);
        assert!(sources.get_diagnostic_source(generated).is_none());
        assert!(sources.remap(generated, 0, 1).is_none());

        std::fs::write(generated, "new\n").unwrap();
        std::fs::write(
            dir.path().join("generated.mbt.map.json"),
            r#"{"mappings":[{"source":"generated.mbt","original_offset":0,"generated_offset":0,"length":3}]}"#,
        )
        .unwrap();
        assert!(sources.get_diagnostic_source(generated).is_none());
        assert!(sources.remap(generated, 0, 1).is_none());

        let mut next_batch = DiagnosticSources::new(None);
        assert_eq!(
            next_batch
                .get_diagnostic_source(generated)
                .unwrap()
                .source
                .text(),
            "new\n"
        );
        assert!(next_batch.remap(generated, 0, 1).is_some());
    }
}
