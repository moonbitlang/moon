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
    let source = DiagnosticSource::new("source.mbt".to_owned(), "é🦀\r\nx\u{2028}z\n".to_owned());
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
